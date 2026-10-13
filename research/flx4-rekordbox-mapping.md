# DDJ-FLX4 mapping against installed rekordbox

Inspected 13 October 2026 for PulseDeck. The installed rekordbox profile identifies a number of semantics that were missing or incorrectly assigned in PulseDeck's initial controller adapter. The byte parser already covers almost every publicly documented physical control; the larger gap is mapping those decoded actions to library navigation, source-aware loading, mode-specific deck behavior, and feedback.

## Evidence and scope

REA skill applied: `/Users/<local-user>/.codex/skills/reverse-engineer-anything/SKILL.md`, staged workflow steps 2–6. Current REA MCP/native decompiler tools are not available. Existing `research/evidence/rea-doctor.json` records registration drift and unavailable engines; this pass did not repeat setup or install an engine. Existing retained REA artifact inventory (`research/evidence/rekordbox-7-installed/artifact.json`, actual Evidence `ev_0f9e0cb5b5da4c2f5f25581bd40c12da6b40448650fec8d9d09ca0e908d16e86`) establishes target/package inventory, not control execution.

The new evidence is **ordinary shell CSV parsing and PDFKit extraction**, not REA decompilation or runtime MIDI observation. No fabricated Evidence IDs. Root owns controller runtime observation; this subtask opened no UI, connection, credentials, or traffic.

- Installed resource: `/Applications/rekordbox 7/rekordbox.app/Contents/Resources/MidiMappings/DDJ-FLX4.midi.csv`, 259 lines, 17,540 bytes, SHA-256 `f0ad95f8bd843bcec42e4de0041fd6f847cb86b1b7d0001270cb838c2a05ba09`. Format-aware row extraction including original line numbers: [shipped profile ledger](evidence/flx4-mapping/shipped-profile-ledger.json).
- [Official MIDI list v1.0](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf), locally retained under `research/flx4-reference`; SHA-256 `e03ac376ebaa2d461c1a2c832c3c182e3579c893a1531944ebac1b5ce620388f`. Public byte values below were cross-checked against this local document. Online source reopened in this pass.
- [Official rekordbox hardware diagram v1.0](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_HardwareDiagram_rekordbox_E1.pdf), SHA-256 `4ebf55d0e7a9db5df2ccf19325c15c5ff9d073fcea9e462232e8cfe6c7466f60`; local extracted text retained previously. Online source reopened in this pass.
- [Official operating manual](https://downloads.support.alphatheta.com/manuals/dj-controllers/DDJ-FLX4/DDJ-FLX4_DRI1804A_manual.pdf), SHA-256 `51153e85418bd21d1a53444458774efd6821a53d8c13aa30090acfa2d3af7feb`. Selected local page text in `evidence/flx4-mapping/manual-control-text.txt` and `manual-additional-text.txt` supports exact tempo-range and hardware-control distinctions. Online opening succeeded, subsequent page requests timed out; local PDF remained readable.

In the tables, `9d/Bd` means `90/B0` for left deck and `91/B1` for right. Hex values shown as status and data1; normal note data2 is 7F/00 for press/release. `u` is the 14-bit value assembled MSB then LSB: `(MSB << 7) | LSB`, divided by 16383 for normalized control. Shipped row declarations do not prove firmware output or which native code handles a control.

## Browser and load

Installed CSV lines 5–11 give unambiguous distinct actions:

| Physical input | Message | Intended action |
|---|---|---|
| Browse rotation | B6 40 | Move selected item in track list or source/folder list; signed relative delta |
| Shift + rotation | B6 64 | Change enlarged waveform zoom; clockwise enlarges |
| Browse click | 96 41 | Switch source/tree/list focus, open selected folder |
| Shift + browse click | 96 42 | Close folder/go back, switch focus |
| Left/right LOAD | 96 46 / 47 | Load selected track into corresponding deck |
| Double LOAD | Same normal messages; host detects interval | Instant doubles from opposite deck, preserving current playback position |
| Shift + left LOAD | 96 68 | Focus related tracks |
| Shift + right LOAD | 96 7A | Toggle enlarged browse view |

The drawer layout is a PulseDeck design choice: click can open the selected Spotify playlist/album; shift-click can return to its parent. It must not silently revert to loading a local-only selection while Spotify is visible. Related tracks needs an explicit independently implemented ranking strategy; the profile names the feature but supplies no algorithm.

## Deck controls

Installed CSV lines 14–43; official manual pages 15–19 distinguish normal and shifted operations.

| Control | Normal message/action | Shift or alternate behavior |
|---|---|---|
| PLAY/PAUSE | 9d 0B toggles | 9d 0E also toggles; not forced pause |
| CUE | 9d 0C set/back cue; held cue preview | 9d 48 return to track start |
| SHIFT | 9d 3F tracks physical held state | Separate shifted notes are not inferred solely from this state |
| Platter touch | 9d 36; release stops touch | 9d 67 shifted touch |
| Platter rotation vinyl on | Bd 22 scratch delta centered 40 | Bd 29 fast search |
| Vinyl-off rotation | Bd 23 pitch bend | Search via Bd 29 |
| Wheel rim rotation | Bd 21 pitch bend | Public sheet retains same CC; keep rim bend distinct from platter search |
| TEMPO | Bd 00 / 20 high-resolution | Shipped CSV also declares Bd 05 / 25; public sheet shows 00 / 20 even shifted. Support both; hardware probe must settle emission |
| IN | 9d 10 sets loop-in | While looping enters loop-in adjustment; 4C shift note has no assigned diagram function |
| OUT | 9d 11 sets loop-out | While looping enters loop-out adjustment; 4E shift note has no assigned diagram function |
| 4 BEAT / EXIT | 9d 4D creates 4-beat loop or exits | 9d 50 toggles active-loop state, distinct from immediate reloop |
| CUE/LOOP left | 9d 51 previous saved cue/loop; 12 half loop | 9d 3E delete saved cue/loop |
| CUE/LOOP right | 9d 53 next saved cue/loop; 13 double loop | 9d 3D save cue/loop |
| BEAT SYNC | 9d 58 toggle BPM/beat sync | Long press 5C sets master; shift 60 changes tempo range |

The Sync button sends its message on release, unlike normal button presses. Avoid a general release-trigger rule for unrelated controls. Tempo range for **rekordbox** is ±6%, ±10%, ±16%, WIDE, then ±6% (manual printed page 17). ±8/16/50 is the **Serato** sequence on page 21 and must not be attributed to rekordbox. WIDE's exact engine bounds require independent selection/verification; the word alone does not establish a numeric ±100% mapping.

Jog MIDI deltas are centered at hexadecimal 40; 41 is one forward step and 3F one reverse step. Frame counts per revolution, speed curve, scratch latency, search multiplier, hold smoothing, and firmware backspin physics were not measured here.

## Mixer and monitoring

Installed CSV lines 46–66. Public profile has MSB/LSB pairs for all knobs/faders:

| Physical control | MSB / LSB | Semantic target |
|---|---|---|
| TRIM | Bd 04 / 24 | Per-channel input gain |
| HI / MID / LOW | Bd 07/27, 0B/2B, 0F/2F | Three-band channel EQ |
| Channel fader | Bd 13 / 33 | Channel level |
| Channel fader start | 9d 66 / 52 | Start play / return cue; hardware emits when shifted and crossing endpoint |
| CFX left / right | B6 17/37 / 18/38 | Sound Color FX or enabled Smart CFX |
| Channel headphones CUE | 9d 54 | Monitor selected channel toggle |
| Shift + channel headphones CUE | 9d 68 | Tap that deck's BPM |
| Crossfader | B6 1F / 3F | Deck balance |
| Master level | B6 08 / 28 | Master output volume; hardware/host division unverified |
| Master headphones CUE | 96 63 | Master headphone monitor toggle |
| Shift + master CUE | 96 78 | Shipped profile PanelForward; diagram lists no shifted assignment |
| Headphones mix | B6 0C / 2C | Cue/master balance; hardware/host division unverified |
| Headphones level | B6 0D / 2D | Monitor gain; hardware/host division unverified |
| MIC LEVEL | B6 05 / 25 | Microphone level; host input/monitor routing needs verification |
| SMART CFX | 96 00 | Enable/disable Smart CFX |
| Shift + SMART CFX | 96 08 | Select Smart CFX preset |
| SMART FADER | 96 01 | Enable/disable Smart Fader |
| Shift + SMART FADER | 96 09 emitted publicly | Unassigned in rekordbox diagram; Serato has second preset, do not infer rekordbox parity |
| Android MONO/STEREO | 96 6D | 00 stereo, 7F mono for Android output; not an ordinary Mac deck function |
| Rear MIC ATT | No published MIDI | Physical input attenuation; no host command inferred |

Master/headphones controls have shipped comments `No function assigned here` and normal MasterCue row is commented. That is a concrete reason to check actual device gain/routing rather than double-applying software gain. It is not proof there is no native handler. Channel meters are described as **before channel faders** in the manual; feedback should reflect that signal point if aiming at the reference behavior.

## Beat FX

Installed CSV lines 69–89 and public MIDI FX block:

| Control | Bytes | Meaning |
|---|---|---|
| FX channel switch | 94 10 on selects A; 95 11 on selects B | A, B, or both; other companion notes carry zero |
| FX SELECT | 94 63 | Next effect |
| Shift + FX SELECT | 94 64 | Previous effect |
| BEAT left/right | 94 4A / 4B | Beat division down/up |
| Shift + BEAT left | 94 66 | Automatic BPM timing |
| Shift + BEAT right | 94 6B | Tap BPM timing |
| LEVEL/DEPTH | B4 02 / 22 | Effect depth; B5 pair also shipped for second FX bank |
| ON/OFF | 94 or95 47 | Toggle effect for selected target(s); both targets emit bank94 |
| Shift + ON/OFF | 94 or95 43 | Release FX |

The official sheet prints decimal channel 6 beside B4 for depth. The shipped profile corroborates B4 and declares B5 for a second software bank; actual firmware output still requires a hardware sample. Do not interpret bank94 as always deck A: in both-target switch position one message must affect both decks. Shipped pad/FX names do not recover the reference echo/reverb/filter algorithms, effect set, division range, or Release FX DSP.

## Eight pad modes, all pads and shift actions

Mode selector notes are 9d 1B HotCue, 1E PadFx1, 20 BeatJump, 22 Sampler; shifted selectors 69 Keyboard, 6B PadFx2, 6D BeatLoop, 6F KeyShift. Input/output pad statuses are 97 (A normal), 98 (A shifted), 99 (B normal), 9A (B shifted). Data1 is mode bank ×16 + pad index 0..7. These agree between the public MIDI sheet and installed profile rows 103–237.

| Bank/mode | Normal pads 1..8 | Shift behavior |
|---|---|---|
| 0 HotCue | Set/trigger A..H | Delete A..H |
| 1 PadFx1 | Slots1..8 | Slots9..16; momentary effect behavior |
| 2 BeatJump | −1,+1,−2,+2,−4,+4,−8,+8 beats initially | Pads1..6 unused; pad7/8 change jump-size setting |
| 3 Sampler | A uses slots1..8; B slots9..16 | Stop corresponding slot, or load selected track to slot |
| 4 Keyboard | +4,+5,+6,+7,0,+1,+2,+3 semitones initially; play from selected HotCue | Pads1..6 unused; pad7/8 change pitch-bank setting |
| 5 PadFx2 | Slots17..24 | Slots25..32; momentary effect behavior |
| 6 BeatLoop | ¼,½,1,2,4,8,16,32 beats | No assigned shifted action |
| 7 KeyShift | +4,+5,+6,+7,0,+1,+2,+3 semitones initially | Pads1..6 unused; pad7/8 change pitch-bank setting |

Installed `pad/BeatJump.pad.csv` extends jump magnitudes through128; `pad/BeatLoop.pad.csv` extends loop sizes to512. `Keyboard.pad.csv`/`KeyShift.pad.csv` declare alternate negative pitch rows. These are configurable templates; the exact parameter-page order/wrap policy remains unmeasured. PadFx1/2 files define slot indices without DSP identity, so echo-strength-by-pad is an independent approximation, not recovered reference behavior. Streaming sources may have different sampler/recording capabilities; the MIDI address does not grant a capability.

## LED, meter, and settings ledger

- Deck transport LEDs: 9d 0B/0E play; 0C/48 cue; 54/68 monitor/tap; 58 sync;60 range. Normal 00/7F convention. Loop IN/OUT 10/11 and shifted4C/4E have public LED addresses.
- Eight mode LEDs mirror the selector notes above. Pad LED addresses mirror the normal/shifted mode bank addresses; sending only HotCue bank cannot reflect all pad modes.
- Smart LEDs: 96 00 CFX,96 01 Fader.
- Channel meter: B0/B1 02. Public five ranges: 26–40,41–56,57–64,65–76,77–7F. Actual amplitude/dB calibration is unknown.
- Loaded illumination: 9F 00/01 7F is a **trigger for each load**, not a permanently latched loaded flag. Replacement of an already loaded track must retrigger.
- Vinyl: 90/91 17 00/7F; public default enabled. The hardware has no physical vinyl toggle.
- FX47 LED is special: **NOTE ON blinks; NOTE OFF lights steadily**. Ordinary note127/0 interpretation alone cannot select blink versus solid correctly.
- Shipped LoopState 9F04/05, BackSpinLength BF45, FaderStart9F42, Reverse9F43, CutLag BF41, Meter9F4B, MicInput9F4A, SmartFaderFader BF4E, MasterMonoStereo9F47, Limiter9F49 are named declarations. Payload enums, startup order, persistence, firmware support and acknowledgments are unknown. Do not send guessed values.
- Shipped FFF2/FFF3 pseudo-parameters are not MIDI statuses; CSV priority/blink/interval options are not measured host timing guarantees.

## Initial PulseDeck discrepancies handed off

This audit is against the source read at task start. Root and other agents own implementation changes; this section is not a claim that these issues remain after their work.

1. `Flx4Parser` emits shifted browse/load/masterCue under generic actions; consumer omitted zoom, source/list focus, RelatedTracks, BrowseView, PanelForward. Add semantic handling without confusing shifted load with normal load.
2. `controller.rs` loaded only local paths. Visible Spotify selection needs source-aware load and double-load must duplicate RAM source/state without a second provider preparation.
3. Shift Play forced Pause, shifted channelCue toggled monitoring, active-loop note50 treated Reloop. These semantics differ from the reference declarations.
4. Tempo range8/16/50 is Serato rather than rekordbox; shifted-tempo05/25 missing despite shipped declaration (public firmware mismatch unresolved).
5. Loop in/out adjustment, saved loops/active-loop behavior, FX select/beat/target and release handling absent or approximated. A complete physical mapping needs explicit engine features or visible limits, not silent no-ops.
6. Shift BeatJump scaled a jump by4; shift Keyboard/KeyShift subtracted8; sampler right deck reused left slots. Correct reference parameter and deck-bank behavior.
7. FX on affected only decoded bank deck, ignoring both-target switch state. FX depth is not the chosen effect identity; FXSelect is presently unhandled.
8. Feedback lacked Smart/FX and mode-aware pad states; loadedfalse→true only missed replacements; channel meter signal point/calibration and hardware monitor gain require runtime validation.
9. Jog parser considered wheel rim shifted when globalShift held, potentially invoking search even though rim remains pitch bend in diagram. Distinguish the physical CC.

## Remaining acceptance evidence

Static mapping checklist answered; live input decoding, LED operation, master/cue output channel routing, hardware gain division, jog resolution/latency, release-button timing and audible feature behavior are pending root runtime checks. Native decompilation of the reference DSP and exact proprietary Smart/effect equivalence were not performed. No claim of full rekordbox parity follows from mapping every input.

## Jog follow-up: position did not move from the platter

Follow-up requested after the first physical mapping build. This section reflects ordinary **focused ARM64 disassembly** of the same installed main executable, along with a read of the current independent PulseDeck code. No reference application execution or hardware capture was performed by this worker. Full command, selected symbols, output size and hashes are in [jog-disassembly-manifest.json](evidence/flx4-mapping/jog-disassembly-manifest.json). LLVM left some atomic instructions as `.long`; unknown indirect player-control targets and state enums remain explicit.

Concrete code risk: at the follow-up read, PulseDeck's controller scratch path required `(scratch || shifted) && jog_target.is_some() && matching_load_revision`. `jog_target` was initialized **only by a touch note**. A rotation arriving without a previously seen touch therefore fell back to non-scratch `Jog`. On a paused deck a temporary speed bend cannot advance source position. This can occur if the controller connection starts after a hand is already resting on the platter, a touch packet is lost, the firmware is in vinyl-off mode, or the user turns the rim. It is a mechanism consistent with the reported symptom, not a confirmed cause until root's actual raw MIDI samples identify which case occurred.

Current connect/autoconnect code does send publicly documented `90/91 17 7F` to enable vinyl when a matching output is connected. CoreMIDI input alone does not establish that feedback reached hardware. Verify normal platter CC22 and touch36 before claiming vinyl state. Shift platter uses CC29 and touch67; rim remains CC21 and must not be classified as fast search solely because Shift is held.

Native evidence:

| Function/address | Observation | Meaning and boundary |
|---|---|---|
| `ScratchProcessor::propertySetCountPerRotate`, `0x101b4fd64` | Stores incoming integer count/revolution; computes `1000 / (count * rpm / 60)` into offset F0, selecting a configured RPM field | Scratch conversion is calibrated to count/revolution and RPM; actual FLX4 setter value and selected RPM not recovered |
| `ScratchProcessor::processValue`, `0x101b501cc` | Uses `F0 * count_delta / elapsed_value`; maintains count/time state; dispatches `execValueBend` or `execValueScratch` according to processor state | Timestamped movement/speed path; fixed milliseconds-per-event is not established reference behavior |
| `ControllerPerformanceProcessor::getNextScratchSpeed`, `0x1001c18f0` | For integer1..512 computes `(d1 - n*d0)/n`; otherwise zero | Short interpolation helper; caller meanings/units not recovered |
| `ScratchProcessor::processOnPause`, `0x101b501c8` | Branches directly to `execPause(bool)` | Explicit pause lifecycle entry |
| `ScratchProcessor::execPause`, `0x101b516e8` | Stores pause state and branches to scratch or bend transition functions depending on internal state | Pause changes processor behavior; enums unresolved |
| `ScratchProcessor::wheelOn`, `0x101b51788` | Queries player-control state flags; branches to `execOnBendPrepare` or `execOnScratch` | Wheel handling is state-dependent; plausible paused-wheel audition path, but flag values and indirect control semantics unresolved |
| `ScratchProcessor::execValueBend`, `0x101b5187c` | Stops scratch where necessary and issues a scaled float through a player-control virtual slot; literals label bend | Supports separate bend handling; virtual call not fully resolved |
| `ScratchProcessor::execValueScratch`, `0x101b51a48` | Maintains scratch state and uses player-control virtual slots; logs scratch/stopScratch | Separate scratch lifecycle; exact audible algorithm/latency not recovered |

The public manual printed pages19–20 names top/vinyl scratch, rim/vinyl-off bend, and Shift+top accelerated search. It does not explicitly say how paused rim movement should behave. Do not present paused rim seeking as verified public-contract parity. A practical independent app should still allow finding a cue point while paused; choose/verify that behavior against a live reference.

Actionable fixes to validate in root's scope:

1. Preserve every incoming jog count using an accumulated absolute target rather than restarting a fixed-duration delta grain from a stale transport snapshot. Root's new `ScrubTo` path addresses this separate displacement-loss problem.
2. If a scratch CC is received without previously observed touch, recover a bounded gesture rather than silently bending a paused deck. Avoid permanent synthetic hold: release on documented touch-off, inactivity timeout, disconnect, and source change.
3. Treat paused rim/vinyl-off movement separately from a playing pitch bend; audition/scrub while preserving paused intent if that is the chosen PulseDeck behavior. Confirm the reference live before calling it exact parity.
4. Use input timestamps and measure signed counts for a marked full platter revolution, then select the count/RPM calibration. The initial2ms/count scratch and100ms/count search scales were independent design choices, not recovered rekordbox constants.
5. Test positive/reverse motion while paused and playing, touch/release, shift search, source replacement during a hold, disconnect/reconnect, and movement after a connection made mid-touch. Observe both audible output and source-position movement.


Local captures are excluded from the public repository; evidence paths above refer to the investigator’s local workspace.
