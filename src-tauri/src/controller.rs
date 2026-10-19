use crate::{load_into_deck, AppState};
use deck_engine::{EngineCommand, EngineSnapshot, LoopBoundary};
use flx4_midi::{
    Button, ControllerAction, DeckFeedback, FeedbackSnapshot, GlobalButton, Knob, PadMode,
};
use serde::Serialize;
use std::{collections::VecDeque, time::Instant};
use tauri::{Emitter, Manager};

pub struct ControlState {
    pub selected: usize,
    pub browse_context: Vec<i64>,
    trim: [f32; 2],
    fader: [f32; 2],
    eq: [[f32; 3]; 2],
    pub modes: [PadMode; 2],
    range: [f64; 2],
    loop_beats: [f64; 2],
    pub sync: [bool; 2],
    pub master: usize,
    fx_depth: [f32; 2],
    fx_target: usize,
    fx_kind: [usize; 2],
    fx_enabled: [bool; 2],
    fx_beat_index: [usize; 2],
    jump_scale: [f64; 2],
    pitch_bank: [f64; 2],
    smart_cfx_preset: usize,
    taps: [VecDeque<f64>; 2],
    started: Instant,
    pub browser_ui_active: bool,
    jog_target: [Option<f64>; 2],
    jog_revision: [u64; 2],
    jog_time: [Option<Instant>; 2],
    jog_held: [bool; 2],
    pub smart_fader: bool,
    pub smart_cfx: bool,
}
impl Default for ControlState {
    fn default() -> Self {
        Self {
            selected: 0,
            browse_context: Vec::new(),
            trim: [1.; 2],
            fader: [1.; 2],
            eq: [[1.; 3]; 2],
            modes: [PadMode::HotCue; 2],
            range: [6.; 2],
            loop_beats: [4.; 2],
            sync: [false; 2],
            master: 0,
            fx_depth: [0.5; 2],
            fx_target: 0,
            fx_kind: [0; 2],
            fx_enabled: [false; 2],
            fx_beat_index: [4; 2],
            jump_scale: [1.; 2],
            pitch_bank: [0.; 2],
            smart_cfx_preset: 0,
            taps: std::array::from_fn(|_| VecDeque::new()),
            started: Instant::now(),
            browser_ui_active: true,
            jog_target: [None; 2],
            jog_revision: [0; 2],
            jog_time: [None; 2],
            jog_held: [false; 2],
            smart_fader: false,
            smart_cfx: false,
        }
    }
}

const FX_BEATS: [f64; 10] = [
    1. / 32.,
    1. / 16.,
    1. / 8.,
    1. / 4.,
    1. / 2.,
    1.,
    2.,
    4.,
    8.,
    16.,
];
#[derive(Clone, Serialize)]
pub struct ControllerSurface {
    pub tempo_ranges: [f64; 2],
    pub pad_modes: [PadMode; 2],
    pub fx_name: [&'static str; 2],
    pub fx_enabled: [bool; 2],
    pub fx_beats: [f64; 2],
    pub fx_depth: f32,
    pub fx_target: usize,
    pub smart_cfx_preset: &'static str,
}
impl ControlState {
    pub fn set_browse_context(
        &mut self,
        track_ids: Vec<i64>,
        selected: Option<i64>,
    ) -> Result<(), String> {
        if track_ids.len() > 100_000 {
            return Err("Browser context exceeds 100,000 tracks".into());
        }
        let index = selected
            .map(|id| {
                track_ids
                    .iter()
                    .position(|track| *track == id)
                    .ok_or("Selected track is not in the current browser")
            })
            .transpose()?;
        self.selected =
            index.unwrap_or_else(|| self.selected.min(track_ids.len().saturating_sub(1)));
        self.browse_context = track_ids;
        Ok(())
    }
    pub fn observe(&mut self, command: &EngineCommand, derive_fx: bool) {
        match command {
            EngineCommand::SetEq {
                deck,
                low,
                mid,
                high,
            } => self.eq[*deck] = [*low, *mid, *high],
            EngineCommand::SetDeckGain { deck, gain } => self.trim[*deck] = *gain,
            EngineCommand::SetChannelFader { deck, value } => self.fader[*deck] = *value,
            EngineCommand::SetSync { deck, enabled } => self.sync[*deck] = *enabled,
            EngineCommand::SetMaster { deck } => self.master = *deck,
            EngineCommand::SetFx { deck, echo, reverb } if derive_fx => {
                self.fx_enabled[*deck] = *echo > 0.0 || *reverb > 0.0;
                if *echo > 0.0 && *reverb == 0.0 {
                    self.fx_kind[*deck] = 0;
                }
                if *reverb > 0.0 && *echo == 0.0 {
                    self.fx_kind[*deck] = 1;
                }
                if self.fx_enabled[*deck] {
                    self.fx_depth[*deck] = echo.max(*reverb);
                }
            }
            EngineCommand::SetFxBeat { deck, beats } => {
                if let Some(i) = FX_BEATS.iter().position(|b| (b - beats).abs() < 1e-8) {
                    self.fx_beat_index[*deck] = i;
                }
            }
            EngineCommand::SetLoop {
                deck,
                beats: Some(beats),
            } => self.loop_beats[*deck] = *beats,
            _ => {}
        }
    }
    pub fn release_jogs(&mut self, engine: &deck_engine::EngineHandle) {
        for deck in 0..2 {
            if self.jog_held[deck] {
                let _ = engine.command(EngineCommand::JogTouch {
                    deck,
                    touched: false,
                });
            }
            self.jog_held[deck] = false;
            self.jog_target[deck] = None;
            self.jog_time[deck] = None;
        }
    }
    pub fn surface(&self) -> ControllerSurface {
        ControllerSurface {
            tempo_ranges: self.range,
            pad_modes: self.modes,
            fx_name: self.fx_kind.map(|k| ["Echo", "Reverb", "Filter"][k]),
            fx_enabled: self.fx_enabled,
            fx_beats: self.fx_beat_index.map(|i| FX_BEATS[i]),
            fx_depth: self.fx_depth[self.fx_target],
            fx_target: self.fx_target,
            smart_cfx_preset: ["Filter + Echo", "Filter + Reverb"][self.smart_cfx_preset],
        }
    }
    pub fn set_range(&mut self, deck: usize, range: f64) -> Result<(), String> {
        if deck > 1 || ![6., 10., 16., 100.].contains(&range) {
            return Err("Tempo range must be 6, 10, 16 or 100".into());
        }
        self.range[deck] = range;
        Ok(())
    }
    fn tap(&mut self, deck: usize, now: f64) -> Option<f64> {
        let times = &mut self.taps[deck];
        if times
            .back()
            .is_some_and(|last| now - last > 2.0 || now - last < 0.15)
        {
            times.clear();
        }
        times.push_back(now);
        while times.len() > 8 {
            times.pop_front();
        }
        if times.len() < 3 {
            return None;
        }
        let mut intervals: Vec<f64> = times
            .iter()
            .zip(times.iter().skip(1))
            .map(|(a, b)| b - a)
            .collect();
        intervals.sort_by(f64::total_cmp);
        let interval = intervals[intervals.len() / 2];
        Some((60. / interval).clamp(30., 300.))
    }
    fn fx_commands(&self, deck: usize) -> Vec<EngineCommand> {
        let depth = if self.fx_enabled[deck] {
            self.fx_depth[deck]
        } else {
            0.
        };
        let (echo, reverb) = match self.fx_kind[deck] {
            0 => (depth, 0.),
            1 => (0., depth),
            _ => (0., 0.),
        };
        let mut result = vec![
            EngineCommand::SetFx { deck, echo, reverb },
            EngineCommand::SetFxBeat {
                deck,
                beats: FX_BEATS[self.fx_beat_index[deck]],
            },
        ];
        if self.fx_kind[deck] == 2 {
            result.push(EngineCommand::SetFilter {
                deck,
                amount: if self.fx_enabled[deck] {
                    self.fx_depth[deck]
                } else {
                    0.
                },
            });
        }
        result
    }
}

pub fn handle(app: &tauri::AppHandle, action: ControllerAction) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _ = app.emit("controller_action", &action);
    let mut control = match state.controller.lock() {
        Ok(c) => c,
        Err(_) => return Err("Controller state unavailable".into()),
    };
    let snap = state.engine.transport_snapshot();
    let mut commands = Vec::new();
    let mut manual_bpm = None;
    let surface_changed = matches!(
        &action,
        ControllerAction::PadMode { .. }
            | ControllerAction::FxSelect { .. }
            | ControllerAction::FxBeat { .. }
            | ControllerAction::FxEnable { .. }
            | ControllerAction::FxTarget { .. }
            | ControllerAction::GlobalButton { .. }
            | ControllerAction::Knob {
                knob: Knob::FxDepth,
                ..
            }
            | ControllerAction::Button {
                button: Button::TempoRange,
                ..
            }
    );
    match action {
        ControllerAction::Button {
            deck,
            button,
            pressed,
            shifted,
        } if deck < 2 => match button {
            Button::Play if pressed => commands.push(EngineCommand::TogglePlay { deck }),
            Button::Cue => {
                if shifted {
                    if pressed {
                        commands.push(EngineCommand::Seek { deck, seconds: 0. });
                    }
                } else {
                    commands.push(EngineCommand::CuePress { deck, pressed });
                }
            }
            Button::HeadphoneCue if pressed => commands.push(EngineCommand::SetHeadphoneCue {
                deck,
                enabled: !snap.decks[deck].cue_enabled,
            }),
            Button::Sync if pressed => {
                control.sync[deck] = !snap.decks[deck].sync_enabled;
                commands.push(EngineCommand::SetSync {
                    deck,
                    enabled: control.sync[deck],
                });
            }
            Button::SyncOff if pressed => {
                control.sync[deck] = false;
                commands.push(EngineCommand::SetSync {
                    deck,
                    enabled: false,
                });
            }
            Button::Master if pressed => {
                control.master = deck;
                commands.push(EngineCommand::SetMaster { deck });
            }
            Button::TempoRange if pressed => {
                control.range[deck] = match control.range[deck] as i32 {
                    6 => 10.,
                    10 => 16.,
                    16 => 100.,
                    _ => 6.,
                };
                let _ = app.emit(
                    "controller_tempo_range",
                    serde_json::json!({"deck":deck,"range":control.range[deck]}),
                );
                state.message(format!(
                    "Deck {} tempo range ±{}%",
                    deck + 1,
                    control.range[deck]
                ));
            }
            Button::AutoLoop if pressed => {
                control.loop_beats[deck] = 4.;
                commands.push(EngineCommand::SetLoop {
                    deck,
                    beats: if snap.decks[deck].loop_start_seconds.is_some() {
                        None
                    } else {
                        Some(4.)
                    },
                });
            }
            Button::ActiveLoop | Button::Reloop if pressed => {
                commands.push(EngineCommand::ToggleActiveLoop { deck })
            }
            Button::LoopIn | Button::LoopOut if pressed && !shifted => {
                let boundary = if button == Button::LoopIn {
                    LoopBoundary::In
                } else {
                    LoopBoundary::Out
                };
                if snap.decks[deck].loop_start_seconds.is_some() {
                    commands.push(EngineCommand::SetLoopEdit {
                        deck,
                        boundary: if snap.decks[deck].loop_edit == Some(boundary) {
                            None
                        } else {
                            Some(boundary)
                        },
                    });
                } else {
                    commands.push(if button == Button::LoopIn {
                        EngineCommand::LoopIn { deck }
                    } else {
                        EngineCommand::LoopOut { deck }
                    });
                }
            }
            Button::TapBpm if pressed => {
                let now = control.started.elapsed().as_secs_f64();
                manual_bpm = control.tap(deck, now).map(|bpm| (deck, bpm));
            }
            Button::MemoryPrevious if pressed => {
                if snap.decks[deck].loop_start_seconds.is_some() {
                    control.loop_beats[deck] = (control.loop_beats[deck] / 2.).max(0.0625);
                    commands.push(EngineCommand::SetLoop {
                        deck,
                        beats: Some(control.loop_beats[deck]),
                    });
                } else {
                    commands.push(EngineCommand::MemoryCuePrevious { deck });
                }
            }
            Button::MemoryNext if pressed => {
                if snap.decks[deck].loop_start_seconds.is_some() {
                    control.loop_beats[deck] = (control.loop_beats[deck] * 2.).min(256.);
                    commands.push(EngineCommand::SetLoop {
                        deck,
                        beats: Some(control.loop_beats[deck]),
                    });
                } else {
                    commands.push(EngineCommand::MemoryCueNext { deck });
                }
            }
            Button::LoopHalf if pressed => {
                control.loop_beats[deck] = (control.loop_beats[deck] / 2.).max(0.0625);
                commands.push(EngineCommand::SetLoop {
                    deck,
                    beats: Some(control.loop_beats[deck]),
                });
            }
            Button::LoopDouble if pressed => {
                control.loop_beats[deck] = (control.loop_beats[deck] * 2.).min(256.);
                commands.push(EngineCommand::SetLoop {
                    deck,
                    beats: Some(control.loop_beats[deck]),
                });
            }
            Button::FaderStart if pressed => commands.push(EngineCommand::Play { deck }),
            Button::FaderCue if pressed => commands.push(EngineCommand::Cue { deck }),
            Button::MemorySave if pressed => commands.push(EngineCommand::MemoryCueSave { deck }),
            Button::MemoryDelete if pressed => {
                commands.push(EngineCommand::MemoryCueDelete { deck })
            }
            _ => {}
        },
        ControllerAction::Knob {
            deck, knob, value, ..
        } => {
            if let Some(deck) = deck.filter(|d| *d < 2) {
                match knob {
                    Knob::Tempo => commands.push(EngineCommand::SetTempo {
                        deck,
                        percent: ((value as f64 - 0.5) * 2. * control.range[deck])
                            .clamp(-90.0, 100.0),
                    }),
                    Knob::Trim => {
                        control.trim[deck] = value * 2.;
                        commands.push(EngineCommand::SetDeckGain {
                            deck,
                            gain: control.trim[deck],
                        });
                    }
                    Knob::ChannelFader => {
                        control.fader[deck] = value;
                        commands.push(EngineCommand::SetChannelFader { deck, value });
                    }
                    Knob::EqLow | Knob::EqMid | Knob::EqHigh => {
                        let band = match knob {
                            Knob::EqLow => 0,
                            Knob::EqMid => 1,
                            _ => 2,
                        };
                        control.eq[deck][band] = value * 2.;
                        let [low, mid, high] = control.eq[deck];
                        commands.push(EngineCommand::SetEq {
                            deck,
                            low,
                            mid,
                            high,
                        });
                    }
                    Knob::Filter => {
                        commands.push(EngineCommand::SetFilter {
                            deck,
                            amount: value * 2. - 1.,
                        });
                        if control.smart_cfx {
                            commands.push(EngineCommand::SetFx {
                                deck,
                                echo: if control.smart_cfx_preset == 0 {
                                    (value * 2. - 1.).abs() * 0.7
                                } else {
                                    0.
                                },
                                reverb: if control.smart_cfx_preset == 1 {
                                    (value * 2. - 1.).abs() * 0.7
                                } else {
                                    0.
                                },
                            });
                        }
                    }
                    Knob::FxDepth => {
                        let target = control.fx_target;
                        control.fx_depth[target] = value;
                        commands.extend(control.fx_commands(control.fx_target));
                    }
                    _ => {}
                }
            } else {
                match knob {
                    Knob::Crossfader => {
                        let position = value * 2. - 1.;
                        commands.push(EngineCommand::SetCrossfader { position });
                        if control.smart_fader {
                            let a = snap.decks[0].bpm.unwrap_or(120.);
                            let b = snap.decks[1].bpm.unwrap_or(120.);
                            let target = a + (b - a) * value as f64;
                            for deck in 0..2 {
                                let base = snap.decks[deck].bpm.unwrap_or(120.);
                                commands.push(EngineCommand::SetTempo {
                                    deck,
                                    percent: ((target / base - 1.) * 100.).clamp(-90., 100.),
                                });
                            }
                            commands.push(EngineCommand::SetFx {
                                deck: if value > 0.5 { 0 } else { 1 },
                                echo: (position.abs() * 0.7).clamp(0., 0.7),
                                reverb: 0.,
                            });
                        }
                    }
                    Knob::MasterGain => {
                        commands.push(EngineCommand::SetMasterGain { gain: value * 2. })
                    }
                    Knob::HeadphoneMix => commands.push(EngineCommand::SetCueMix { mix: value }),
                    Knob::HeadphoneGain => {
                        commands.push(EngineCommand::SetCueGain { gain: value * 2. })
                    }
                    Knob::FxDepth => {
                        let target = control.fx_target;
                        control.fx_depth[target] = value;
                    }
                    _ => {}
                }
            }
        }
        ControllerAction::Jog {
            deck,
            delta,
            scratch,
            shifted,
        } if deck < 2 => {
            if snap.decks[deck].loop_edit.is_some() {
                commands.push(EngineCommand::Jog {
                    deck,
                    delta_seconds: delta as f64 * 0.002,
                    scratch: false,
                });
            } else if scratch || shifted {
                let now = Instant::now();
                let revision = state.engine.load_revision(deck);
                let elapsed = control.jog_time[deck]
                    .map(|last| now.duration_since(last).as_secs_f64() * 1000.)
                    .unwrap_or(16.);
                let same_source = control.jog_revision[deck] == revision;
                let start = if same_source && (control.jog_held[deck] || elapsed < 150.) {
                    control.jog_target[deck].unwrap_or(snap.decks[deck].position_seconds)
                } else {
                    control.jog_held[deck] = false;
                    snap.decks[deck].position_seconds
                };
                let target = (start + delta as f64 * if shifted { 0.1 } else { 0.002 })
                    .clamp(0., state.engine.track_duration(deck).unwrap_or(0.));
                control.jog_target[deck] = Some(target);
                control.jog_revision[deck] = revision;
                control.jog_time[deck] = Some(now);
                // Scratch CC22 is itself evidence of vinyl movement. Recover a
                // finite audible gesture when the touch-on packet was missed.
                if !control.jog_held[deck] {
                    commands.push(EngineCommand::JogTouch {
                        deck,
                        touched: true,
                    });
                }
                commands.push(EngineCommand::ScrubTo {
                    deck,
                    seconds: target,
                    duration_ms: (elapsed + 8.).clamp(8., 100.),
                });
                if !control.jog_held[deck] {
                    commands.push(EngineCommand::ScrubEnd {
                        deck,
                        seconds: target,
                    });
                }
            } else {
                commands.push(EngineCommand::Jog {
                    deck,
                    delta_seconds: delta as f64 * 0.002,
                    scratch: false,
                });
            }
        }
        ControllerAction::JogTouch { deck, touched, .. } if deck < 2 => {
            if snap.decks[deck].loop_edit.is_none() {
                control.jog_held[deck] = touched;
                if touched {
                    control.jog_target[deck] = Some(snap.decks[deck].position_seconds);
                    control.jog_revision[deck] = state.engine.load_revision(deck);
                    control.jog_time[deck] = Some(Instant::now());
                    commands.push(EngineCommand::JogTouch {
                        deck,
                        touched: true,
                    });
                } else if let Some(target) = control.jog_target[deck]
                    .take()
                    .filter(|_| control.jog_revision[deck] == state.engine.load_revision(deck))
                {
                    commands.push(EngineCommand::ScrubEnd {
                        deck,
                        seconds: target,
                    });
                } else {
                    commands.push(EngineCommand::JogTouch {
                        deck,
                        touched: false,
                    });
                }
            }
        }
        ControllerAction::PadMode { deck, mode } if deck < 2 => control.modes[deck] = mode,
        ControllerAction::Pad {
            deck,
            mode,
            pad,
            pressed,
            shifted,
        } if deck < 2 && pad < 8 => match mode {
            PadMode::HotCue if pressed => commands.push(if shifted {
                EngineCommand::ClearHotCue { deck, index: pad }
            } else if snap.decks[deck].hot_cues[pad].is_some() {
                EngineCommand::TriggerHotCue { deck, index: pad }
            } else {
                EngineCommand::SetHotCue { deck, index: pad }
            }),
            PadMode::BeatJump if pressed && shifted => {
                if pad >= 6 {
                    control.jump_scale[deck] = (control.jump_scale[deck]
                        * if pad == 6 { 0.5 } else { 2. })
                    .clamp(1. / 32., 16.);
                }
            }
            PadMode::BeatJump if pressed => {
                let steps = [-1., 1., -2., 2., -4., 4., -8., 8.];
                commands.push(EngineCommand::BeatJump {
                    deck,
                    beats: steps[pad] * control.jump_scale[deck],
                });
            }
            PadMode::BeatLoop if pressed => {
                let beats = [0.25, 0.5, 1., 2., 4., 8., 16., 32.];
                control.loop_beats[deck] = beats[pad];
                commands.push(EngineCommand::SetLoop {
                    deck,
                    beats: if snap.decks[deck].loop_start_seconds.is_some_and(|start| {
                        snap.decks[deck].loop_end_seconds.is_some_and(|end| {
                            ((end - start) * snap.decks[deck].bpm.unwrap_or(120.) / 60.
                                - beats[pad])
                                .abs()
                                < 0.02
                        })
                    }) {
                        None
                    } else {
                        Some(beats[pad])
                    },
                });
            }
            PadMode::PadFx1 | PadMode::PadFx2 => commands.push(EngineCommand::SetFx {
                deck,
                echo: if pressed { 0.3 + pad as f32 * 0.08 } else { 0. },
                reverb: if pressed && mode == PadMode::PadFx2 {
                    0.6
                } else {
                    0.
                },
            }),
            PadMode::Keyboard | PadMode::KeyShift if pressed && shifted => {
                if pad >= 6 {
                    control.pitch_bank[deck] = (control.pitch_bank[deck]
                        + if pad == 6 { 1. } else { -1. })
                    .clamp(-12., 5.);
                }
            }
            PadMode::Keyboard | PadMode::KeyShift if pressed => {
                let pitch = [4., 5., 6., 7., 0., 1., 2., 3.];
                commands.push(EngineCommand::SetPitch {
                    deck,
                    semitones: (pitch[pad] + control.pitch_bank[deck]).clamp(-12., 12.),
                });
                if mode == PadMode::Keyboard {
                    commands.push(EngineCommand::TriggerHotCue { deck, index: 0 });
                }
            }
            PadMode::Sampler if pressed => commands.push(if shifted {
                EngineCommand::StopSample {
                    slot: deck * 8 + pad,
                }
            } else {
                EngineCommand::TriggerSample {
                    slot: deck * 8 + pad,
                }
            }),
            _ => {}
        },
        ControllerAction::Browse {
            delta,
            shifted: false,
        } if !control.browser_ui_active => {
            if !control.browse_context.is_empty() {
                control.selected = (control.selected as isize + delta as isize)
                    .rem_euclid(control.browse_context.len() as isize)
                    as usize;
                let _ = app.emit(
                    "controller_selection",
                    control.browse_context[control.selected],
                );
            } else if let Ok(lib) = state.library.lock() {
                if let Ok(tracks) = lib.track_ids() {
                    if !tracks.is_empty() {
                        control.selected = (control.selected as isize + delta as isize)
                            .rem_euclid(tracks.len() as isize)
                            as usize;
                        let _ = app.emit("controller_selection", tracks[control.selected]);
                    }
                }
            }
        }
        ControllerAction::Load {
            deck,
            shifted: false,
        } if deck < 2 && !control.browser_ui_active => {
            let path = state.library.lock().ok().and_then(|lib| {
                let id = control
                    .browse_context
                    .get(control.selected)
                    .copied()
                    .or_else(|| lib.track_ids().ok()?.get(control.selected).copied())?;
                lib.path_for_id(id).ok()
            });
            if let Some(path) = path {
                let app = app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = load_into_deck(&app, deck, std::path::Path::new(&path)) {
                        app.state::<AppState>().message(e);
                    }
                });
            }
        }
        ControllerAction::GlobalButton {
            button,
            pressed: true,
            ..
        } => match button {
            GlobalButton::SmartFader => {
                control.smart_fader = !control.smart_fader;
                state.message(format!(
                    "PulseDeck Smart Fader {} — independent transition curve",
                    if control.smart_fader {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            GlobalButton::SmartCfx => {
                control.smart_cfx = !control.smart_cfx;
                state.message(format!(
                    "PulseDeck Smart CFX {} — independent filter/echo/reverb preset",
                    if control.smart_cfx {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
            }
            GlobalButton::MasterCue => {
                let enabled = !snap.decks.iter().all(|d| d.cue_enabled);
                for deck in 0..2 {
                    commands.push(EngineCommand::SetHeadphoneCue { deck, enabled });
                }
            }
            GlobalButton::SmartCfxNext => {
                control.smart_cfx_preset = (control.smart_cfx_preset + 1) % 2;
            }
            _ => {}
        },
        ControllerAction::FxTarget {
            deck,
            selected: true,
        } if deck < 2 => {
            control.fx_target = deck;
        }
        ControllerAction::FxSelect { delta, .. } => {
            let deck = control.fx_target;
            if control.fx_kind[deck] == 2 {
                commands.push(EngineCommand::SetFilter { deck, amount: 0. });
            }
            control.fx_kind[deck] =
                (control.fx_kind[deck] as i32 + delta as i32).rem_euclid(3) as usize;
            commands.extend(control.fx_commands(deck));
        }
        ControllerAction::FxBeat { delta, shifted, .. } => {
            let deck = control.fx_target;
            if shifted {
                if delta > 0 {
                    let now = control.started.elapsed().as_secs_f64();
                    manual_bpm = control.tap(deck, now).map(|bpm| (deck, bpm));
                } else if let Some(bpm) = state.engine.metadata_snapshot().tracks[deck]
                    .as_ref()
                    .and_then(|t| t.bpm)
                {
                    commands.push(EngineCommand::SetBpm { deck, bpm });
                }
            } else {
                control.fx_beat_index[deck] = (control.fx_beat_index[deck] as i32 + delta as i32)
                    .clamp(0, FX_BEATS.len() as i32 - 1)
                    as usize;
                commands.extend(control.fx_commands(deck));
            }
        }
        ControllerAction::FxEnable {
            pressed: true,
            shifted,
            ..
        } => {
            let deck = control.fx_target;
            if shifted {
                control.fx_enabled[deck] = true;
                control.fx_kind[deck] = 0;
                commands.push(EngineCommand::Pause { deck });
            } else {
                control.fx_enabled[deck] = !control.fx_enabled[deck];
            }
            commands.extend(control.fx_commands(deck));
        }
        _ => {}
    }
    let surface = control.surface();
    drop(control);
    if surface_changed {
        let _ = app.emit("controller_surface", surface);
    }
    if let Some((deck, bpm)) = manual_bpm {
        crate::engine_command_inner(
            app.clone(),
            app.state::<AppState>(),
            EngineCommand::SetBpm { deck, bpm },
            false,
            None,
        )?;
    }
    for command in commands {
        crate::engine_command_inner(app.clone(), app.state(), command, false, None)?;
    }
    Ok(())
}

pub fn feedback(
    snapshot: &EngineSnapshot,
    modes: [PadMode; 2],
    loaded: [bool; 2],
) -> FeedbackSnapshot {
    FeedbackSnapshot {
        decks: std::array::from_fn(|deck| {
            let state = &snapshot.decks[deck];
            DeckFeedback {
                playing: state.playing,
                cue_active: !state.playing,
                headphone_cue: state.cue_enabled,
                sync: state.sync_enabled,
                master: state.is_master,
                loop_active: state.loop_start_seconds.is_some(),
                loaded: loaded[deck],
                fx_active: state.echo > 0. || state.reverb > 0.,
                smart_fader: false,
                smart_cfx: false,
                meter: state.peak[0].max(state.peak[1]),
                hot_cues: std::array::from_fn(|i| state.hot_cues[i].is_some()),
                pad_mode: modes[deck],
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paged_context_and_selection_commit_together_and_invalid_changes_roll_back() {
        let mut control = ControlState::default();
        control.set_browse_context(vec![1, 2], Some(2)).unwrap();
        control
            .set_browse_context(vec![1, 2, 3, 4], Some(4))
            .unwrap();
        assert_eq!(control.selected, 3);
        assert!(control.set_browse_context(vec![9], Some(4)).is_err());
        assert_eq!(control.browse_context, vec![1, 2, 3, 4]);
        assert_eq!(control.selected, 3);
        control.set_browse_context(vec![1, 2], None).unwrap();
        assert_eq!(control.selected, 1);
        assert!(control.set_browse_context(vec![0; 100001], None).is_err());
        assert_eq!(control.browse_context, vec![1, 2]);
    }

    #[test]
    fn tap_bpm_rejects_accidental_double_tap_and_resets_after_a_break() {
        let mut control = ControlState::default();
        assert_eq!(control.tap(0, 0.), None);
        assert_eq!(control.tap(0, 0.5), None);
        assert_eq!(control.tap(0, 1.), Some(120.));
        assert_eq!(control.tap(1, 1.), None); // Independent deck.
        assert_eq!(control.tap(0, 1.05), None);
        assert_eq!(control.tap(0, 1.55), None);
        assert_eq!(control.tap(0, 2.05), Some(120.));
        assert_eq!(control.tap(0, 5.), None);
    }

    #[test]
    fn tempo_ranges_validate_before_mutating_and_fx_off_clears_audio_effects() {
        let mut control = ControlState::default();
        control.set_range(1, 100.).unwrap();
        assert!(control.set_range(1, 8.).is_err());
        assert!(control.set_range(2, 6.).is_err());
        assert_eq!(control.surface().tempo_ranges, [6., 100.]);
        control.fx_kind[1] = 2;
        control.fx_depth[1] = 0.8;
        assert!(control.fx_commands(1).iter().any(|c| matches!(c,
            EngineCommand::SetFilter { deck: 1, amount } if *amount == 0.)));
        control.fx_enabled[1] = true;
        assert!(control.fx_commands(1).iter().any(|c| matches!(c,
            EngineCommand::SetFilter { deck: 1, amount } if *amount == 0.8)));
    }
}
