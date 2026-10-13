# Rekordbox controller resources: read-only findings

Target: `/Applications/rekordbox 7/rekordbox.app/Contents/Resources`. Inspected 13 October 2026.

## Evidence boundary

All findings below are **shell filesystem observations**, produced by `rg`, Python CSV/XML/ZIP parsers, SHA-256 hashing, and numbered file reads. **They are not REA Evidence records and have no REA Evidence IDs.** The main investigation handles REA/native code separately. No app execution, controller connection changes, or asset/code copying into an implementation occurred.

Resource identities are retained in [rekordbox-controller-resource-hashes.json](rekordbox-controller-resource-hashes.json). File names, mapping labels, configuration options and localized strings establish shipped declarations only. They do not establish which resources are loaded, code paths executed, exact hardware behavior, DSP quality or latency.

## FLX4 mapping is shipped as CSV

[DDJ-FLX4.midi.csv](</Applications/rekordbox 7/rekordbox.app/Contents/Resources/MidiMappings/DDJ-FLX4.midi.csv:1>) is 17,540 bytes, SHA-256 `f0ad95f8bd843bcec42e4de0041fd6f847cb86b1b7d0001270cb838c2a05ba09`. It has 259 lines and 201 active, non-comment named rows. Header fields declare name, function, type, input, four deck columns, output, four deck columns, option, comment. Types counted: 55 Button, 108 Pad, 15 KnobSliderHiRes, 11 Indicator, 3 JogRotate, 2 JogTouch, 2 Rotary, 2 Value, 2 Parameter, 1 Difference.

The CSV is enough to guide a declarative Rust controller profile, but its parsing implementation is not inspected here. Standard protocol values should come from the public MIDI list first, with this file used as cross-check and a source of investigation candidates.

| Resource lines | Observed declaration | Interoperability use / limit |
|---|---|---|
| 5–11 | Browser `B640`, shift zoom `B664`; load `9646/9647`; related tracks `9668`; browser view `967A` | Plan browser actions and double-load behavior; comments mention instant doubles. |
| 14–25 | Play/cue; scratch `B022`, bend `B023`, search `B029`, touch `9036/9067`, wheel `B021` | Separate rotation from touch and search. Types/`RO` are labels, not a measured dispatch policy. |
| 28–29 | Tempo `B000` and shifted `B005`, both KnobSliderHiRes | **Mismatch candidate:** public PDF lists `B000` for shifted tempo too. Do not choose one without observing firmware output. |
| 30–42 | Loop in/out, active/4-beat loops, memory cue/loop recall, sync/master/range | LoopHalf `9012` and LoopDouble `9013` coexist with recall selectors; determine which firmware states emit each. |
| 46–55 | Crossfader `B61F`, channel fader `B013`, gain/EQ, CFX `B617/B618` | Preserve high-resolution controls. Public PDF supplies MSB/LSB pairs. |
| 56–62 | SmartCFX `9600`, selection `9608`, SmartFader `9601` | Button/action mapping exists; no DSP definition here. |
| 58–65 | Headphone mix/volume and master level comments say no function assigned; MasterCue row is commented | Do not infer that hardware entirely implements them. Native special handling may exist. |
| 69–87 | Two FX blocks use `94/95`; depth `B402/B502`; on/off `9447/9547`, release `9443/9543` | Corroborates `B402` as a mapping candidate for the public PDF's channel/status inconsistency. It does not prove firmware output. |
| 93–100 | Eight pad modes: Hot Cue, Keyboard, Pad FX1/2, Beat Jump/Loop, Sampler, Key Shift | Mode-aware profile, not eight globally interchangeable pads. |
| 103–237 | Pads contain deck-column values `7/9`, shifted `8/10`, mode note ranges `00–07`, `10–17`, through `70–77` | Corroborates public MIDI channels 8/10 and shifted 9/11 if deck columns are zero-based channel indexes. Confirm parser semantics. |
| 244–246 | Loaded output `9F00/9F01`, loop state `9F04/9F05`, vinyl `9017` | Independent feedback path needed; exact trigger values except publicly documented loaded/vinyl are unknown. |

## Settings selectors beyond the public MIDI sheet

[Lines 247–255](</Applications/rekordbox 7/rekordbox.app/Contents/Resources/MidiMappings/DDJ-FLX4.midi.csv:247>) contain these output selectors:

| Setting label | Selector |
|---|---|
| BackSpinLengthSetting | `BF45` |
| FaderStartSetting | `9F42` |
| CrossFaderReverseSetting | `9F43` |
| CrossFaderCutLagSetting | `BF41` |
| LevelMeterSetting | `9F4B` |
| MicInputSetting | `9F4A` |
| FaderForSmartFaderSetting | `BF4E` |
| MasterMonoStereoSetting | `9F47` (input and output) |
| MasterPeakLimiterSetting | `9F49` (input and output) |

**Unknown:** data-byte enums/ranges, startup defaults, ordering, firmware-version support, persistence, echo/ack semantics and whether special native code transforms these selectors. They are read-only investigation leads, not commands to send blindly. No initialization/SysEx definition was found in this CSV.

[Lines 258–259](</Applications/rekordbox 7/rekordbox.app/Contents/Resources/MidiMappings/DDJ-FLX4.midi.csv:258>) contain `JogRatency` parameter `FFF2`, `Value=4`, comment `unused`, and `MidiOutInterval` `FFF3`, `Value=2`. These pseudo-selectors are not ordinary MIDI statuses. Units and runtime interpretation are unknown. `Fast`, `Priority=50/100`, `RO`, and `Blink=600` options elsewhere likewise do not establish a real-time schedule or milliseconds.

## Pad templates and supporting assets

[Mode.pad.csv](</Applications/rekordbox 7/rekordbox.app/Contents/Resources/pad/Mode.pad.csv:1>) declares mode/template/pad visibility and numeric selection identifiers. The FLX4 modes use selections 1–8. It also declares Slicer, Active Censor, Sequence Call, Lighting, Transport, Sample Scratch, Stems, eight user slots and nine combinations. This is useful for the full roadmap, but does not establish FLX4 physical mappings for those additional modes.

Format-aware reads show:

- `PadFx1.pad.csv` / `PadFx2.pad.csv`: slot numbers 1–16; **no effect identity or DSP parameters**.
- `BeatJump.pad.csv`: ±1/2/4/8/16/32/64/128 values and corresponding user functions.
- `BeatLoop.pad.csv`: 1/64 through 512 values.
- `Keyboard.pad.csv` / `KeyShift.pad.csv`: ordered pitch values +4,+5,+6,+7,0,+1,+2,+3,−4,−3,−2,−1,−8,−7,−6,−5.
- `Stems.pad.csv`: VOCAL/INST/BASS/DRUMS, four FX variants, M/S; these are UI action declarations, not a separation model specification.

`tutorial/DDJFLX4` contains 39 JPG/PNG assets. No image semantics were inferred and none were copied. `MERGE FX.zip` inventory has 18 WAV members named MergeFX Sample Sound; no Smart CFX/Fader parameter file is present in that archive. Its hash is in the manifest. This does not exclude parameter data embedded elsewhere.

`skins/simplemixer.xml` parses as a `skin` tree with Channel1/Channel2, Browser and ProDJLink elements plus geometry/font/state fields. That supports the existence of packaged UI layout declarations only; it does not establish the runtime renderer or modern performance-view construction.

## Smart feature evidence and bounded negative results

[Spanish localization, lines 4224–4233](</Applications/rekordbox 7/rekordbox.app/Contents/Resources/locale/spanish.lang:4224>) contains English source keys for Smart Fader/Ableton Link mutual-exclusion messages, tempo-range and empty-deck warnings, fader selection/recommended curve, and Smart CFX/Touch FX mutual exclusion. Line 4311 contains a disabled-operation message for Smart Fader. These suggest acceptance-test cases; they do **not** prove this app version enforces each rule in every mode.

`english.lang` is only 741 bytes and lacks these keys; other translations retain English source keys. This observation is why Spanish was inspected, not a runtime language assumption. Spanish hash: `8d94af21955a568b03290eb1cc3b5d1885c92057b6102246d82d0c1244db9374`.

Bounded search across resource filenames and human-readable CSV/XML/LANG/SVG contents found Smart button/settings rows and localized labels, but **no explicit Smart CFX preset DSP graph, Smart Fader interpolation definition, or startup handshake**. Native binaries, opaque payloads and runtime state were not searched in this subtask. These required questions remain unresolved.

Bundle `product.db` is 24,576 bytes but Python SQLite rejects it as not a database; `master.db` begins with non-SQLite bytes. They were hashed, not decrypted. File extensions do not prove SQLite, encryption, schema or controller-record contents.

## Reconstruction decisions supported by this evidence

1. Put mapping definitions and LED feedback behind an independently authored controller profile. Keep raw MIDI channels, note/CC, high-resolution pairs and modes explicit.
2. Keep native Rust transport/deck actions separate from UI, including a feedback scheduler. CSV priority/interval declarations can motivate measurements but are not performance targets.
3. Implement core two-deck transport, sync, loops, EQ and basic FX before Smart presets. Validate public-MIDI controls first; treat undocumented settings as a later measured protocol task.
4. Record hardware MIDI for the shifted-tempo and FX-depth discrepancies, reconnect lifecycle and settings before deciding protocol details.
5. Recreate visuals, sounds, models and DSP independently. Do not embed vendor resources merely because they are shipped locally.

No claim of feature or performance parity is established by this resource inspection.
