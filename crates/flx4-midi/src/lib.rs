//! DDJ-FLX4 adapter based on AlphaTheta's public MIDI message list, version 1.0.
//! No vendor code, undocumented settings, or proprietary mappings are included.
//! CoreMIDI callbacks enqueue fixed-sized packets; parsing, application callbacks,
//! LED writes, and diagnostic history run on a dedicated worker.

use crossbeam_channel::{bounded, Receiver, Sender};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

const QUEUE_CAPACITY: usize = 2048;
const HISTORY_CAPACITY: usize = 256;
const MAX_PACKET_BYTES: usize = 64;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MidiPorts {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn list_ports() -> Result<MidiPorts, String> {
    let input = MidiInput::new("Flux DJ discovery").map_err(|e| e.to_string())?;
    let output = MidiOutput::new("Flux DJ discovery").map_err(|e| e.to_string())?;
    Ok(MidiPorts {
        inputs: input
            .ports()
            .iter()
            .map(|p| {
                input
                    .port_name(p)
                    .unwrap_or_else(|_| "Unknown input".into())
            })
            .collect(),
        outputs: output
            .ports()
            .iter()
            .map(|p| {
                output
                    .port_name(p)
                    .unwrap_or_else(|_| "Unknown output".into())
            })
            .collect(),
    })
}

/// Choose only one unambiguous advertised FLX4 endpoint; never a random MIDI device.
pub fn preferred_flx4_ports(ports: &MidiPorts) -> Option<(String, Option<String>)> {
    let inputs: Vec<_> = ports
        .inputs
        .iter()
        .filter(|n| n.to_uppercase().contains("DDJ-FLX4"))
        .collect();
    if inputs.len() != 1 {
        return None;
    }
    let outputs: Vec<_> = ports
        .outputs
        .iter()
        .filter(|n| n.to_uppercase().contains("DDJ-FLX4"))
        .collect();
    Some((
        inputs[0].clone(),
        if outputs.len() == 1 {
            Some(outputs[0].clone())
        } else {
            None
        },
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadMode {
    HotCue,
    PadFx1,
    BeatJump,
    Sampler,
    Keyboard,
    PadFx2,
    BeatLoop,
    KeyShift,
}
impl PadMode {
    fn from_bank(bank: u8) -> Option<Self> {
        Some(match bank {
            0 => Self::HotCue,
            1 => Self::PadFx1,
            2 => Self::BeatJump,
            3 => Self::Sampler,
            4 => Self::Keyboard,
            5 => Self::PadFx2,
            6 => Self::BeatLoop,
            7 => Self::KeyShift,
            _ => return None,
        })
    }
    pub fn bank(self) -> u8 {
        match self {
            Self::HotCue => 0,
            Self::PadFx1 => 1,
            Self::BeatJump => 2,
            Self::Sampler => 3,
            Self::Keyboard => 4,
            Self::PadFx2 => 5,
            Self::BeatLoop => 6,
            Self::KeyShift => 7,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Play,
    Cue,
    Shift,
    LoopIn,
    LoopOut,
    AutoLoop,
    Reloop,
    LoopHalf,
    LoopDouble,
    MemoryPrevious,
    MemoryNext,
    MemoryDelete,
    MemorySave,
    TempoRange,
    Sync,
    SyncOff,
    Master,
    HeadphoneCue,
    TapBpm,
    ActiveLoop,
    FaderStart,
    FaderCue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knob {
    Tempo,
    Trim,
    EqHigh,
    EqMid,
    EqLow,
    ChannelFader,
    Filter,
    Crossfader,
    MasterGain,
    HeadphoneMix,
    HeadphoneGain,
    MicGain,
    FxDepth,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalButton {
    MasterCue,
    BrowsePress,
    BrowseBack,
    PanelForward,
    SmartCfx,
    SmartCfxNext,
    SmartFader,
    SmartFaderNext,
    AndroidMono,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControllerAction {
    Button {
        deck: usize,
        button: Button,
        pressed: bool,
        shifted: bool,
    },
    Knob {
        deck: Option<usize>,
        knob: Knob,
        raw: u16,
        value: f32,
    },
    JogTouch {
        deck: usize,
        touched: bool,
        shifted: bool,
    },
    Jog {
        deck: usize,
        delta: i16,
        scratch: bool,
        shifted: bool,
    },
    PadMode {
        deck: usize,
        mode: PadMode,
    },
    Pad {
        deck: usize,
        mode: PadMode,
        pad: usize,
        pressed: bool,
        shifted: bool,
    },
    Browse {
        delta: i16,
        shifted: bool,
    },
    Load {
        deck: usize,
        shifted: bool,
    },
    GlobalButton {
        button: GlobalButton,
        pressed: bool,
        shifted: bool,
    },
    FxSelect {
        deck: usize,
        delta: i16,
    },
    FxBeat {
        deck: usize,
        delta: i16,
        shifted: bool,
    },
    FxEnable {
        deck: usize,
        pressed: bool,
        shifted: bool,
    },
    FxTarget {
        deck: usize,
        selected: bool,
    },
}

/// Public profile parser. Decks/pads are zero-based, raw high-resolution values
/// are 0..16383, jog delta is centered at 64, browser delta is signed 7-bit.
pub struct Flx4Parser {
    msb: [[Option<u8>; 32]; 16],
    shift: [bool; 2],
    modes: [PadMode; 2],
    touches: [bool; 2],
}
impl Default for Flx4Parser {
    fn default() -> Self {
        Self {
            msb: [[None; 32]; 16],
            shift: [false; 2],
            modes: [PadMode::HotCue; 2],
            touches: [false; 2],
        }
    }
}
impl Flx4Parser {
    pub fn pad_modes(&self) -> [PadMode; 2] {
        self.modes
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ControllerAction> {
        let mut result = Vec::new();
        // Midir gives complete packets. Skip system messages; never interpret
        // SysEx payload as controls. Preserve every complete channel message.
        let mut i = 0;
        while i < bytes.len() {
            let status = bytes[i];
            if status >= 0xf8 {
                i += 1;
                continue;
            }
            if status >= 0xf0 {
                break;
            }
            if status < 0x80 {
                i += 1;
                continue;
            }
            let len = if status & 0xf0 == 0xc0 || status & 0xf0 == 0xd0 {
                2
            } else {
                3
            };
            if i + len > bytes.len() {
                break;
            }
            if bytes[i + 1..i + len].iter().any(|b| *b >= 128) {
                i += 1;
                continue;
            }
            if len == 3 {
                let ch = (status & 15) as usize;
                let data = bytes[i + 1];
                let value = bytes[i + 2];
                match status & 0xf0 {
                    0x80 | 0x90 => {
                        self.note(ch, data, status & 0xf0 == 0x90 && value > 0, &mut result)
                    }
                    0xb0 => self.cc(ch, data, value, &mut result),
                    _ => {}
                }
            }
            i += len;
        }
        result
    }
    fn note(&mut self, ch: usize, n: u8, pressed: bool, out: &mut Vec<ControllerAction>) {
        if ch == 7 || ch == 8 || ch == 9 || ch == 10 {
            if n & 15 < 8 {
                if let Some(mode) = PadMode::from_bank(n >> 4) {
                    let deck = if ch <= 8 { 0 } else { 1 };
                    let shifted = ch == 8 || ch == 10;
                    out.push(ControllerAction::Pad {
                        deck,
                        mode,
                        pad: (n & 15) as usize,
                        pressed,
                        shifted,
                    });
                }
            }
            return;
        }
        if ch <= 1 {
            let deck = ch;
            if n == 0x36 || n == 0x67 {
                self.touches[deck] = pressed;
                out.push(ControllerAction::JogTouch {
                    deck,
                    touched: pressed,
                    shifted: n == 0x67,
                });
                return;
            }
            let mode = match n {
                0x1b => Some(PadMode::HotCue),
                0x1e => Some(PadMode::PadFx1),
                0x20 => Some(PadMode::BeatJump),
                0x22 => Some(PadMode::Sampler),
                0x69 => Some(PadMode::Keyboard),
                0x6b => Some(PadMode::PadFx2),
                0x6d => Some(PadMode::BeatLoop),
                0x6f => Some(PadMode::KeyShift),
                _ => None,
            };
            if let Some(mode) = mode {
                if pressed {
                    self.modes[deck] = mode;
                    out.push(ControllerAction::PadMode { deck, mode });
                }
                return;
            }
            let mapping = match n {
                0x0b => (Button::Play, false),
                0x0e => (Button::Play, true),
                0x0c => (Button::Cue, false),
                0x48 => (Button::Cue, true),
                0x3f => {
                    self.shift[deck] = pressed;
                    (Button::Shift, false)
                }
                0x10 => (Button::LoopIn, false),
                0x4c => (Button::LoopIn, true),
                0x11 => (Button::LoopOut, false),
                0x4e => (Button::LoopOut, true),
                0x4d => (Button::AutoLoop, false),
                0x50 => (Button::ActiveLoop, true),
                0x12 => (Button::LoopHalf, false),
                0x13 => (Button::LoopDouble, false),
                0x51 => (Button::MemoryPrevious, false),
                0x53 => (Button::MemoryNext, false),
                0x3e => (Button::MemoryDelete, true),
                0x3d => (Button::MemorySave, true),
                0x58 => (Button::Sync, false),
                0x5c => (Button::Master, false),
                0x60 => (Button::TempoRange, true),
                0x54 => (Button::HeadphoneCue, false),
                0x68 => (Button::TapBpm, true),
                0x66 => (Button::FaderStart, true),
                0x52 => (Button::FaderCue, true),
                _ => return,
            };
            // BEAT SYNC emits only when the physical button is released. A
            // message itself is the trigger even if its velocity is zero.
            let trigger = if matches!(
                mapping.0,
                Button::Sync | Button::SyncOff | Button::Master | Button::TempoRange
            ) {
                true
            } else {
                pressed
            };
            out.push(ControllerAction::Button {
                deck,
                button: mapping.0,
                pressed: trigger,
                shifted: mapping.1 || self.shift[deck],
            });
        } else if ch == 6 {
            match n {
                0x46 | 0x68 | 0x47 | 0x7a if pressed => out.push(ControllerAction::Load {
                    deck: if n == 0x46 || n == 0x68 { 0 } else { 1 },
                    shifted: n == 0x68 || n == 0x7a,
                }),
                _ => {
                    let mapping = match n {
                        0x63 => (GlobalButton::MasterCue, false),
                        0x78 => (GlobalButton::PanelForward, true),
                        0x41 => (GlobalButton::BrowsePress, false),
                        0x42 => (GlobalButton::BrowseBack, true),
                        0x00 => (GlobalButton::SmartCfx, false),
                        0x08 => (GlobalButton::SmartCfxNext, true),
                        0x01 => (GlobalButton::SmartFader, false),
                        0x09 => (GlobalButton::SmartFaderNext, true),
                        0x6d => (GlobalButton::AndroidMono, false),
                        _ => return,
                    };
                    out.push(ControllerAction::GlobalButton {
                        button: mapping.0,
                        pressed,
                        shifted: mapping.1,
                    });
                }
            }
        } else if ch == 4 || ch == 5 {
            let deck = ch - 4;
            match n {
                0x10 if ch == 4 => out.push(ControllerAction::FxTarget {
                    deck: 0,
                    selected: pressed,
                }),
                0x11 if ch == 5 => out.push(ControllerAction::FxTarget {
                    deck: 1,
                    selected: pressed,
                }),
                0x63 | 0x64 if pressed => out.push(ControllerAction::FxSelect {
                    deck,
                    delta: if n == 0x63 { 1 } else { -1 },
                }),
                0x4a | 0x4b | 0x66 | 0x6b if pressed => out.push(ControllerAction::FxBeat {
                    deck,
                    delta: if n == 0x4a || n == 0x66 { -1 } else { 1 },
                    shifted: n == 0x66 || n == 0x6b,
                }),
                0x47 | 0x43 => out.push(ControllerAction::FxEnable {
                    deck,
                    pressed,
                    shifted: n == 0x43,
                }),
                _ => {}
            }
        }
    }
    fn cc(&mut self, ch: usize, cc: u8, v: u8, out: &mut Vec<ControllerAction>) {
        if ch <= 1 && matches!(cc, 0x21 | 0x22 | 0x23 | 0x29) {
            let delta = v as i16 - 64;
            if delta != 0 {
                out.push(ControllerAction::Jog {
                    deck: ch,
                    delta,
                    scratch: cc == 0x22,
                    shifted: cc == 0x29,
                });
            }
            return;
        }
        if ch == 6 && (cc == 0x40 || cc == 0x64) {
            let delta = if v < 64 { v as i16 } else { v as i16 - 128 };
            if delta != 0 {
                out.push(ControllerAction::Browse {
                    delta,
                    shifted: cc == 0x64,
                });
            }
            return;
        }
        let base = if cc < 32 {
            cc
        } else if cc < 64 {
            cc - 32
        } else {
            return;
        };
        let mapping = match (ch, base) {
            (0 | 1, 0 | 5) => Some((Some(ch), Knob::Tempo)),
            (0 | 1, 4) => Some((Some(ch), Knob::Trim)),
            (0 | 1, 7) => Some((Some(ch), Knob::EqHigh)),
            (0 | 1, 11) => Some((Some(ch), Knob::EqMid)),
            (0 | 1, 15) => Some((Some(ch), Knob::EqLow)),
            (0 | 1, 19) => Some((Some(ch), Knob::ChannelFader)),
            (6, 23) => Some((Some(0), Knob::Filter)),
            (6, 24) => Some((Some(1), Knob::Filter)),
            (6, 31) => Some((None, Knob::Crossfader)),
            (6, 8) => Some((None, Knob::MasterGain)),
            (6, 12) => Some((None, Knob::HeadphoneMix)),
            (6, 13) => Some((None, Knob::HeadphoneGain)),
            (6, 5) => Some((None, Knob::MicGain)),
            // Public list prints channel 6 alongside status B4 for FX depth;
            // shipped mapping corroborates B4, support B5 as second FX bank.
            (4 | 5, 2) => Some((Some(ch - 4), Knob::FxDepth)),
            _ => None,
        };
        if let Some((deck, knob)) = mapping {
            if cc < 32 {
                self.msb[ch][base as usize] = Some(v);
            } else if let Some(msb) = self.msb[ch][base as usize] {
                let raw = ((msb as u16) << 7) | v as u16;
                out.push(ControllerAction::Knob {
                    deck,
                    knob,
                    raw,
                    value: raw as f32 / 16383.0,
                });
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMidiEvent {
    pub timestamp_micros: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MidiStatus {
    /// The selected CoreMIDI connection is open. Endpoint presence is reported
    /// separately; an open handle alone cannot establish USB communication.
    pub connected: bool,
    pub input_name: Option<String>,
    pub output_name: Option<String>,
    pub feedback_enabled: bool,
    pub messages_received: u64,
    pub dropped_messages: u64,
    pub last_error: Option<String>,
    pub pad_modes: [Option<PadMode>; 2],
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeckFeedback {
    pub playing: bool,
    pub cue_active: bool,
    pub headphone_cue: bool,
    pub sync: bool,
    pub master: bool,
    pub loop_active: bool,
    pub loaded: bool,
    pub fx_active: bool,
    pub smart_fader: bool,
    pub smart_cfx: bool,
    pub meter: f32,
    pub hot_cues: [bool; 8],
    pub pad_mode: PadMode,
}
impl Default for DeckFeedback {
    fn default() -> Self {
        Self {
            playing: false,
            cue_active: false,
            headphone_cue: false,
            sync: false,
            master: false,
            loop_active: false,
            loaded: false,
            fx_active: false,
            smart_fader: false,
            smart_cfx: false,
            meter: 0.0,
            hot_cues: [false; 8],
            pad_mode: PadMode::HotCue,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FeedbackSnapshot {
    pub decks: [DeckFeedback; 2],
}

struct Shared {
    status: Mutex<MidiStatus>,
    history: Mutex<VecDeque<RawMidiEvent>>,
    received: AtomicU64,
    dropped: AtomicU64,
}
#[derive(Clone, Copy)]
struct Packet {
    timestamp: u64,
    len: usize,
    bytes: [u8; MAX_PACKET_BYTES],
}
enum Work {
    Midi(Packet),
    Feedback(FeedbackSnapshot),
    Vinyl { deck: usize, enabled: bool },
    Loaded { deck: usize },
    Stop,
}
struct Connection {
    input: MidiInputConnection<()>,
    tx: Sender<Work>,
    worker: JoinHandle<()>,
}

pub struct MidiController {
    callback: Arc<dyn Fn(ControllerAction) + Send + Sync>,
    shared: Arc<Shared>,
    connection: Option<Connection>,
    last_selection: Option<(String, Option<String>)>,
}
impl MidiController {
    pub fn new(callback: impl Fn(ControllerAction) + Send + Sync + 'static) -> Self {
        Self {
            callback: Arc::new(callback),
            shared: Arc::new(Shared {
                status: Mutex::new(MidiStatus::default()),
                history: Mutex::new(VecDeque::with_capacity(HISTORY_CAPACITY)),
                received: AtomicU64::new(0),
                dropped: AtomicU64::new(0),
            }),
            connection: None,
            last_selection: None,
        }
    }
    /// Exact names come from list_ports. Output is optional and is never selected
    /// automatically. Opening an output sends no initialization/configuration.
    pub fn connect(
        &mut self,
        input_name: String,
        output_name: Option<String>,
    ) -> Result<(), String> {
        self.disconnect();
        self.last_selection = Some((input_name.clone(), output_name.clone()));
        let result = self.open_connection(&input_name, output_name.as_deref());
        if let Err(error) = &result {
            let mut status = self.shared.status.lock().unwrap_or_else(|e| e.into_inner());
            status.connected = false;
            status.feedback_enabled = false;
            status.last_error = Some(error.clone());
        }
        result
    }
    fn open_connection(
        &mut self,
        input_name: &str,
        output_name: Option<&str>,
    ) -> Result<(), String> {
        let mut input = MidiInput::new("Flux DJ FLX4 input").map_err(|e| e.to_string())?;
        input.ignore(Ignore::All);
        let candidates = input.ports();
        let mut matches = candidates
            .iter()
            .filter(|p| input.port_name(p).ok().as_deref() == Some(input_name));
        let selected = matches
            .next()
            .ok_or_else(|| {
                format!(
                    "MIDI input '{input_name}' is unavailable. Refresh devices after reconnecting."
                )
            })?
            .clone();
        if matches.next().is_some() {
            return Err(format!(
                "Multiple MIDI inputs named '{input_name}'; use a unique endpoint name."
            ));
        }
        let output = if let Some(name) = output_name {
            let midi = MidiOutput::new("Flux DJ FLX4 feedback").map_err(|e| e.to_string())?;
            let ports = midi.ports();
            let mut matches = ports
                .iter()
                .filter(|p| midi.port_name(p).ok().as_deref() == Some(name));
            let port = matches
                .next()
                .ok_or_else(|| format!("MIDI output '{name}' is unavailable."))?
                .clone();
            if matches.next().is_some() {
                return Err(format!(
                    "Multiple MIDI outputs named '{name}'; use a unique endpoint name."
                ));
            }
            Some(
                midi.connect(&port, "Flux DJ FLX4 feedback")
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let (tx, rx) = bounded(QUEUE_CAPACITY);
        let callback_tx = tx.clone();
        let callback_shared = self.shared.clone();
        let connection = input
            .connect(
                &selected,
                "Flux DJ FLX4 controls",
                move |timestamp, bytes, _| {
                    callback_shared.received.fetch_add(1, Ordering::Relaxed);
                    if bytes.len() > MAX_PACKET_BYTES {
                        callback_shared.dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    let mut packet = Packet {
                        timestamp,
                        len: bytes.len(),
                        bytes: [0; MAX_PACKET_BYTES],
                    };
                    packet.bytes[..bytes.len()].copy_from_slice(bytes);
                    if callback_tx.try_send(Work::Midi(packet)).is_err() {
                        callback_shared.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        self.shared.received.store(0, Ordering::Relaxed);
        self.shared.dropped.store(0, Ordering::Relaxed);
        self.shared
            .history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        {
            let mut status = self.shared.status.lock().unwrap_or_else(|e| e.into_inner());
            *status = MidiStatus {
                connected: true,
                input_name: Some(input_name.into()),
                output_name: output_name.map(Into::into),
                feedback_enabled: output.is_some(),
                pad_modes: [Some(PadMode::HotCue); 2],
                ..Default::default()
            };
        }
        let shared = self.shared.clone();
        let callback = self.callback.clone();
        let worker = thread::Builder::new()
            .name("flx4-midi-worker".into())
            .spawn(move || worker_loop(rx, output, shared, callback))
            .map_err(|e| e.to_string())?;
        self.connection = Some(Connection {
            input: connection,
            tx,
            worker,
        });
        Ok(())
    }
    pub fn reconnect(&mut self) -> Result<(), String> {
        let (input, output) = self
            .last_selection
            .clone()
            .ok_or("Select a MIDI input first.")?;
        self.connect(input, output)
    }
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection.input);
            let _ = connection.tx.send(Work::Stop);
            let _ = connection.worker.join();
        }
        let mut status = self.shared.status.lock().unwrap_or_else(|e| e.into_inner());
        status.connected = false;
        status.feedback_enabled = false;
    }
    pub fn status(&self) -> MidiStatus {
        let mut status = self
            .shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        status.messages_received = self.shared.received.load(Ordering::Relaxed);
        status.dropped_messages = self.shared.dropped.load(Ordering::Relaxed);
        status
    }
    pub fn history(&self) -> Vec<RawMidiEvent> {
        self.shared
            .history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
    pub fn feedback(&self, state: FeedbackSnapshot) -> Result<(), String> {
        let connection = self
            .connection
            .as_ref()
            .ok_or("No MIDI input is connected.")?;
        if !self.status().feedback_enabled {
            return Err("Select a MIDI output to enable feedback.".into());
        }
        connection
            .tx
            .try_send(Work::Feedback(state))
            .map_err(|_| "MIDI feedback queue is full or disconnected.".into())
    }
    /// The only settings write supported is the documented vinyl-mode note.
    /// Call explicitly after the user opts in; connect itself sends nothing.
    pub fn loaded(&self, deck: usize) -> Result<(), String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1".into());
        }
        self.connection
            .as_ref()
            .ok_or("No MIDI input is connected")?
            .tx
            .try_send(Work::Loaded { deck })
            .map_err(|_| "MIDI queue full".into())
    }
    pub fn set_vinyl_mode(&self, deck: usize, enabled: bool) -> Result<(), String> {
        if deck > 1 {
            return Err("Deck must be 0 or 1.".into());
        }
        let connection = self
            .connection
            .as_ref()
            .ok_or("No MIDI input is connected.")?;
        if !self.status().feedback_enabled {
            return Err("Select a MIDI output to set vinyl mode.".into());
        }
        connection
            .tx
            .try_send(Work::Vinyl { deck, enabled })
            .map_err(|_| "MIDI queue is full or disconnected.".into())
    }
}
impl Drop for MidiController {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn worker_loop(
    rx: Receiver<Work>,
    mut output: Option<MidiOutputConnection>,
    shared: Arc<Shared>,
    callback: Arc<dyn Fn(ControllerAction) + Send + Sync>,
) {
    let mut parser = Flx4Parser::default();
    let mut last_output = [[None; 128]; 16];
    let mut loaded = [false; 2];
    while let Ok(work) = rx.recv() {
        match work {
            Work::Stop => break,
            Work::Midi(packet) => {
                {
                    let mut history = shared.history.lock().unwrap_or_else(|e| e.into_inner());
                    if history.len() == HISTORY_CAPACITY {
                        history.pop_front();
                    }
                    history.push_back(RawMidiEvent {
                        timestamp_micros: packet.timestamp,
                        bytes: packet.bytes[..packet.len].to_vec(),
                    });
                }
                for action in parser.feed(&packet.bytes[..packet.len]) {
                    callback(action);
                }
                shared
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .pad_modes = parser.pad_modes().map(Some);
            }
            Work::Feedback(snapshot) => {
                if let Some(output) = output.as_mut() {
                    for message in feedback_messages(snapshot, &mut loaded) {
                        let index = (message[0] & 15) as usize;
                        let key = message[1] as usize;
                        if message[0] == 0x9f
                            || last_output[index][key] != Some((message[0], message[2]))
                        {
                            if let Err(error) = output.send(&message) {
                                shared
                                    .status
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .last_error = Some(error.to_string());
                            } else {
                                last_output[index][key] = Some((message[0], message[2]));
                            }
                        }
                    }
                }
            }
            Work::Loaded { deck } => {
                loaded[deck] = true;
                if let Some(output) = output.as_mut() {
                    if let Err(error) = output.send(&[0x9f, deck as u8, 127]) {
                        shared
                            .status
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .last_error = Some(error.to_string());
                    }
                }
            }
            Work::Vinyl { deck, enabled } => {
                if let Some(output) = output.as_mut() {
                    if let Err(error) =
                        output.send(&[0x90 + deck as u8, 0x17, if enabled { 127 } else { 0 }])
                    {
                        shared
                            .status
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .last_error = Some(error.to_string());
                    }
                }
            }
        }
    }
}

/// Pure documented LED encoder, also usable by tests/custom adapters. Loaded is
/// a pulse, emitted on false->true; callers can reset the edge when loading a new
/// track. Meter normalization intentionally makes no claimed dB calibration.
pub fn feedback_messages(
    snapshot: FeedbackSnapshot,
    previous_loaded: &mut [bool; 2],
) -> Vec<[u8; 3]> {
    let mut messages = Vec::with_capacity(48);
    for (deck, state) in snapshot.decks.iter().enumerate() {
        let status = 0x90 + deck as u8;
        for (note, on) in [
            (0x0b, state.playing),
            (0x0c, state.cue_active),
            (0x54, state.headphone_cue),
            (0x58, state.sync || state.master),
            (0x10, state.loop_active),
            (0x11, state.loop_active),
        ] {
            messages.push([status, note, if on { 127 } else { 0 }]);
        }
        for (mode, note) in [
            (PadMode::HotCue, 0x1b),
            (PadMode::PadFx1, 0x1e),
            (PadMode::BeatJump, 0x20),
            (PadMode::Sampler, 0x22),
            (PadMode::Keyboard, 0x69),
            (PadMode::PadFx2, 0x6b),
            (PadMode::BeatLoop, 0x6d),
            (PadMode::KeyShift, 0x6f),
        ] {
            messages.push([status, note, if mode == state.pad_mode { 127 } else { 0 }]);
        }
        for pad in 0..8 {
            messages.push([
                if deck == 0 { 0x97 } else { 0x99 },
                pad as u8,
                if state.hot_cues[pad] { 127 } else { 0 },
            ]);
        }
        messages.push([
            0x94 + deck as u8,
            0x47,
            if state.fx_active { 127 } else { 0 },
        ]);
        if deck == 0 {
            messages.push([0x96, 0x00, if state.smart_cfx { 127 } else { 0 }]);
            messages.push([0x96, 0x01, if state.smart_fader { 127 } else { 0 }]);
        }
        let level = if state.meter.is_finite() {
            state.meter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let meter = if level < 0.001 {
            0
        } else {
            (26.0 + level * 101.0).round() as u8
        };
        messages.push([0xb0 + deck as u8, 0x02, meter]);
        if state.loaded && !previous_loaded[deck] {
            messages.push([0x9f, deck as u8, 127]);
        }
        previous_loaded[deck] = state.loaded;
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_controller_can_be_owned_by_tauri_mutex_state() {
        fn assert_send<T: Send>() {}
        assert_send::<MidiController>();
    }
    #[test]
    fn transport_press_release_and_shift_are_distinct() {
        let mut p = Flx4Parser::default();
        assert_eq!(
            p.feed(&[0x90, 0x0b, 127]),
            vec![ControllerAction::Button {
                deck: 0,
                button: Button::Play,
                pressed: true,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0x81, 0x0c, 64]),
            vec![ControllerAction::Button {
                deck: 1,
                button: Button::Cue,
                pressed: false,
                shifted: false
            }]
        );
        p.feed(&[0x90, 0x3f, 127]);
        assert_eq!(
            p.feed(&[0x90, 0x48, 127]),
            vec![ControllerAction::Button {
                deck: 0,
                button: Button::Cue,
                pressed: true,
                shifted: true
            }]
        );
        p.feed(&[0x90, 0x3f, 0]);
        assert_eq!(
            p.feed(&[0x90, 0x0c, 0]),
            vec![ControllerAction::Button {
                deck: 0,
                button: Button::Cue,
                pressed: false,
                shifted: false
            }]
        );
    }
    #[test]
    fn sync_release_only_message_triggers_action() {
        assert_eq!(
            Flx4Parser::default().feed(&[0x91, 0x58, 0]),
            vec![ControllerAction::Button {
                deck: 1,
                button: Button::Sync,
                pressed: true,
                shifted: false
            }]
        );
    }
    #[test]
    fn sync_master_tempo_range_and_memory_functions_follow_hardware_diagram() {
        let mut p = Flx4Parser::default();
        for (note, button, shifted) in [
            (0x5c, Button::Master, false),
            (0x60, Button::TempoRange, true),
            (0x3e, Button::MemoryDelete, true),
            (0x3d, Button::MemorySave, true),
            (0x12, Button::LoopHalf, false),
            (0x13, Button::LoopDouble, false),
        ] {
            assert_eq!(
                p.feed(&[0x90, note, 127]),
                vec![ControllerAction::Button {
                    deck: 0,
                    button,
                    pressed: true,
                    shifted
                }]
            );
        }
    }
    #[test]
    fn high_resolution_complete_pair_retains_every_bit() {
        let mut p = Flx4Parser::default();
        assert!(p.feed(&[0xb0, 0x20, 127]).is_empty());
        assert!(p.feed(&[0xb0, 0x00, 64]).is_empty());
        assert_eq!(
            p.feed(&[0xb0, 0x20, 3]),
            vec![ControllerAction::Knob {
                deck: Some(0),
                knob: Knob::Tempo,
                raw: 8195,
                value: 8195.0 / 16383.0
            }]
        );
        assert!(p.feed(&[0xb1, 0, 127]).is_empty());
        assert_eq!(
            p.feed(&[0xb1, 32, 127]),
            vec![ControllerAction::Knob {
                deck: Some(1),
                knob: Knob::Tempo,
                raw: 16383,
                value: 1.0
            }]
        );
        assert!(p.feed(&[0xb6, 31, 0]).is_empty());
        assert_eq!(
            p.feed(&[0xb6, 63, 0]),
            vec![ControllerAction::Knob {
                deck: None,
                knob: Knob::Crossfader,
                raw: 0,
                value: 0.0
            }]
        );
    }
    #[test]
    fn every_public_knob_routes_to_correct_deck() {
        let cases = [
            (0, 4, Some(0), Knob::Trim),
            (1, 7, Some(1), Knob::EqHigh),
            (0, 11, Some(0), Knob::EqMid),
            (1, 15, Some(1), Knob::EqLow),
            (0, 19, Some(0), Knob::ChannelFader),
            (6, 23, Some(0), Knob::Filter),
            (6, 24, Some(1), Knob::Filter),
            (6, 8, None, Knob::MasterGain),
            (6, 12, None, Knob::HeadphoneMix),
            (6, 13, None, Knob::HeadphoneGain),
            (6, 5, None, Knob::MicGain),
            (4, 2, Some(0), Knob::FxDepth),
            (5, 2, Some(1), Knob::FxDepth),
        ];
        for (ch, cc, deck, knob) in cases {
            let mut p = Flx4Parser::default();
            assert!(p.feed(&[0xb0 + ch, cc, 127]).is_empty());
            assert_eq!(
                p.feed(&[0xb0 + ch, cc + 32, 127]),
                vec![ControllerAction::Knob {
                    deck,
                    knob,
                    raw: 16383,
                    value: 1.0
                }]
            );
        }
    }
    #[test]
    fn jog_touch_scratch_bend_and_shift_preserve_delta() {
        let mut p = Flx4Parser::default();
        assert_eq!(
            p.feed(&[0x90, 54, 127]),
            vec![ControllerAction::JogTouch {
                deck: 0,
                touched: true,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0xb0, 34, 67]),
            vec![ControllerAction::Jog {
                deck: 0,
                delta: 3,
                scratch: true,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0xb0, 33, 61]),
            vec![ControllerAction::Jog {
                deck: 0,
                delta: -3,
                scratch: false,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0xb1, 41, 65]),
            vec![ControllerAction::Jog {
                deck: 1,
                delta: 1,
                scratch: false,
                shifted: true
            }]
        );
        p.feed(&[0x90, 54, 0]);
        assert_eq!(
            p.feed(&[0xb0, 34, 65]),
            vec![ControllerAction::Jog {
                deck: 0,
                delta: 1,
                scratch: true,
                shifted: false
            }]
        );
        assert!(p.feed(&[0xb1, 34, 64]).is_empty());
    }
    #[test]
    fn pads_use_transmitted_mode_not_only_last_mode_button() {
        let mut p = Flx4Parser::default();
        assert_eq!(
            p.feed(&[0x91, 0x6d, 127]),
            vec![ControllerAction::PadMode {
                deck: 1,
                mode: PadMode::BeatLoop
            }]
        );
        assert_eq!(p.pad_modes()[1], PadMode::BeatLoop);
        assert_eq!(
            p.feed(&[0x97, 7, 127]),
            vec![ControllerAction::Pad {
                deck: 0,
                mode: PadMode::HotCue,
                pad: 7,
                pressed: true,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0x9a, 0x62, 0]),
            vec![ControllerAction::Pad {
                deck: 1,
                mode: PadMode::BeatLoop,
                pad: 2,
                pressed: false,
                shifted: true
            }]
        );
        assert!(p.feed(&[0x97, 0x0f, 127]).is_empty());
    }
    #[test]
    fn browser_relative_encoding_differs_from_jog() {
        let mut p = Flx4Parser::default();
        assert_eq!(
            p.feed(&[0xb6, 64, 127]),
            vec![ControllerAction::Browse {
                delta: -1,
                shifted: false
            }]
        );
        assert_eq!(
            p.feed(&[0xb6, 100, 3]),
            vec![ControllerAction::Browse {
                delta: 3,
                shifted: true
            }]
        );
        assert_eq!(
            p.feed(&[0x96, 0x47, 127]),
            vec![ControllerAction::Load {
                deck: 1,
                shifted: false
            }]
        );
    }

    #[test]
    fn browse_push_and_shift_back_are_single_press_edges() {
        let mut parser = Flx4Parser::default();
        let actions = parser.feed(&[0x96, 0x41, 127, 0x96, 0x41, 0, 0x96, 0x42, 127]);
        assert_eq!(
            actions,
            vec![
                ControllerAction::GlobalButton {
                    button: GlobalButton::BrowsePress,
                    pressed: true,
                    shifted: false
                },
                ControllerAction::GlobalButton {
                    button: GlobalButton::BrowsePress,
                    pressed: false,
                    shifted: false
                },
                ControllerAction::GlobalButton {
                    button: GlobalButton::BrowseBack,
                    pressed: true,
                    shifted: true
                },
            ]
        );
    }

    #[test]
    fn shifted_tempo_pair_and_shifted_rim_do_not_become_search() {
        let mut parser = Flx4Parser::default();
        assert!(parser.feed(&[0xb1, 0x05, 64]).is_empty());
        assert!(matches!(
            parser.feed(&[0xb1, 0x25, 0])[0],
            ControllerAction::Knob {
                deck: Some(1),
                knob: Knob::Tempo,
                raw: 8192,
                ..
            }
        ));
        parser.feed(&[0x91, 0x3f, 127]);
        assert_eq!(
            parser.feed(&[0xb1, 0x21, 65]),
            vec![ControllerAction::Jog {
                deck: 1,
                delta: 1,
                scratch: false,
                shifted: false
            }]
        );
    }

    #[test]
    fn fx_reverse_and_shifted_auto_tap_are_distinct() {
        let mut parser = Flx4Parser::default();
        assert_eq!(
            parser.feed(&[0x94, 0x64, 127, 0x94, 0x66, 127, 0x94, 0x6b, 127]),
            vec![
                ControllerAction::FxSelect { deck: 0, delta: -1 },
                ControllerAction::FxBeat {
                    deck: 0,
                    delta: -1,
                    shifted: true
                },
                ControllerAction::FxBeat {
                    deck: 0,
                    delta: 1,
                    shifted: true
                },
            ]
        );
    }
    #[test]
    fn unknown_system_and_incomplete_bytes_do_not_trigger_controls() {
        let mut p = Flx4Parser::default();
        for bytes in [
            &[][..],
            &[0x90][..],
            &[0x90, 12][..],
            &[0xf0, 0x90, 12, 127][..],
            &[0x90, 12, 0xff][..],
            &[0xb0, 123, 0][..],
        ] {
            assert!(p.feed(bytes).is_empty());
        }
        assert_eq!(p.feed(&[0xf8, 0x90, 12, 127, 0x91, 11, 127]).len(), 2);
    }
    #[test]
    fn led_feedback_uses_public_status_and_load_pulse_edges() {
        let mut previous = [false; 2];
        let mut feedback = FeedbackSnapshot::default();
        feedback.decks[0].playing = true;
        feedback.decks[0].loaded = true;
        feedback.decks[1].hot_cues[7] = true;
        feedback.decks[1].meter = 1.0;
        let messages = feedback_messages(feedback, &mut previous);
        assert!(messages.contains(&[0x90, 11, 127]));
        assert!(messages.contains(&[0x99, 7, 127]));
        assert!(messages.contains(&[0xb1, 2, 127]));
        assert!(messages.contains(&[0x9f, 0, 127]));
        assert!(!messages.iter().any(|m| m[1] == 0x17 && m[0] < 0x92));
        assert!(!feedback_messages(feedback, &mut previous).contains(&[0x9f, 0, 127]));
    }
}
