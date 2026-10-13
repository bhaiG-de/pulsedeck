---
name: pulsedeck-setup-and-troubleshooting
description: Inspect and configure PulseDeck audio routing, headphone cueing and MIDI connections, and diagnose silence, failed preparation or Spotify authentication through pulsedeckctl. Use for session setup and targeted recovery; distinguish software verification from listening or attached-hardware tests.
---

# PulseDeck setup and troubleshooting

Use for “Why is A playing but I can't hear it?”, “Route B to my headphones”, “Connect my FLX4”, and failed load/preparation/authentication requests. Produce a diagnosis from observed state, apply the requested targeted fix, and check its effect. A request to explain or plan remains read-only. Troubleshooting authorization covers the relevant repair; do not repeatedly ask approval for its individual steps. It does not authorize replacing a live source, launching account authorization, or changing unrelated session preferences.

Read [shared CLI reference](../references/cli.md) for endpoint binding, JSON stages, source guards, jobs and takeover. Bind `pd` to the requested running instance before these examples. Use [musical evidence](../references/musical-evidence.md) when a symptom could be a sparse passage, uncertain beatgrid, tempo error or difficult blend. Software meters cannot establish what the user actually hears.

## Observe first

```sh
pd status
pd commands list
pd commands describe audio.start
pd commands describe midi.connect
pd commands describe engine.command
pd audio devices
pd midi ports
pd midi history
pd jobs list
```

Keep a compact record of the instance/socket, deck identities and `engine.load_revisions`, source duration, position, play state, pending preparation/source jobs, active ramps, relevant levels and routing. Re-read status twice across a short observation interval: increasing `rendered_frames` and deck position establish running software transport. An arbitrary shell delay establishes neither exact beat timing nor audible output. `engine.underruns` counts stream/device errors, not measured hardware underruns.

Audio state has two views: `status.result.audio` records the last successful device/configuration, while `engine.running` and `rendered_frames` reflect actual rendering. **A failed `audio.start` removes the old stream and may leave cached audio status advertising `running:true`.** Do not conclude audio runs from that cached field alone. Save the last known good configuration before a requested routing change; inspect the engine after failure and restore that configuration once if recovery is within the requested task.

## Trace silence along the signal path

| Observation | Interpretation and targeted next step |
| --- | --- |
| Missing source or stalled source job | Inspect job/source error and identity. Repair the path/login prerequisite before a guarded load; protect playing decks. |
| `playing:false`, end reached, or no position advance | Determine whether paused, held by cue/jog, finished, or audio stopped. Start/resume only when requested. |
| `preparation_pending:true` | Wait preparation and inspect its error/timeout before changing pitch/key lock again. |
| Deck `peak` is zero | Inspect the current timestamp's waveform range, gain, EQ/filter and source validity. A quiet intro/break is plausible; zero meter alone is not proof of broken loading. |
| Deck peak present but master peak absent | Check channel fader, crossfader side and `master_gain`, then aggressive EQ/filter/FX. Deck signal precedes channel/cross/master mixing. |
| Master peak present and routing valid | Software produces signal. Check actual selected device, OS volume/mute, amplifier/cables and output channel mapping with the user/hardware. Do not claim the speakers work. |
| Cue selection present but cue peak/output absent | Check cue mix/gain and discrete cue routing; selecting a deck alone does not configure a physical headphone output. |

Explain causal controls before changing them. A fader at zero can be intentional incoming-deck isolation; a crossfader at the opposite side can be intentional performance. Do not solve silence by resetting every knob or raising all gains. Preserve current deck identities, source revisions, other-deck controls, recording and intended master level. Prefer a brief conservative correction to a broad gain increase; positive meters cannot verify safe perceived volume or lack of distortion.

Inspect existing analysis when the source itself appears quiet:

```sh
pd call track.inspect --args '{"deck":"A","points":64}'
pd call track.inspect --args '{"deck":"A","start_seconds":8,"end_seconds":24,"points":128}'
```

Replace the example range with the observed position and valid source bounds; the 8–24 second range requires a loaded track at least 24 seconds long. Low amplitude or sparse bass around that point supports a quiet-region hypothesis, not a certain named break or vocal judgment. Reliable playback with silence in the stored waveform calls for inspecting another passage or listening, rather than pushing gain to compensate for a musical pause.

For an authorized A-fader correction, after observing its source revision and intended level:

```sh
export A_REV=1                 # Replace with current engine.load_revisions[0].
pd deck A fader set 0.7 --expected-load-revision "$A_REV"
pd status
```

Compare the returned stage and fresh actual control value; allow native smoothing to settle before interpreting meters. If master is deliberately muted for a trial, keep it muted and report only the software fader repair. If a manual adjustment disagrees with your last observation, yield instead of overwriting it. Source guards do not guard knob or position changes.

## Headphones and routing

PulseDeck's selected cue feed is pre-channel-fader/pre-crossfader. `cue_mix=0` selects cued decks; `cue_mix=1` selects the master mix. `cue_gain=0` mutes headphones. Keep the incoming channel fader at zero while preparing it; starting its transport for preview requires a preview/playback request. Routing headphones is not a request to load, seek or start the deck.

For B-only cue selection, when muting A's cue matches the requested monitoring intent:

```sh
export B_REV=1                 # Replace with current engine.load_revisions[1].
pd deck B headphones on --expected-load-revision "$B_REV"
pd deck A headphones off --expected-load-revision "$A_REV"
pd mixer cue-mix set 0
pd status
```

Observe `cue_enabled`, `cue_mix`, `cue_gain`, cue meter and `audio.routing`. Do not silently raise a deliberately muted cue level. Both cue channels must be assigned together, all channel indices are zero-based, and master/cue channels must be distinct and supported by the selected device. With only two output channels and null cue routing, discrete headphones are unavailable on that output; report the need for a suitable multi-output device. Never route cue onto master channels to simulate headphones.

The following is executable CLI syntax **only for an observed device supporting at least four outputs and a verified mapping of 0/1 to master, 2/3 to phones**. Reconfiguring audio interrupts rendering; do not experiment during a protected live performance. Read the chosen device name from discovery and retain its existing supported buffer size if appropriate.

```sh
export DEVICE_NAME='DDJ-FLX4'   # Replace with exact observed audio device name.
python3 - <<'PY' > /tmp/pulsedeck-audio-routing.json
import json, os
print(json.dumps({"device_name": os.environ["DEVICE_NAME"], "buffer_frames": 256,
                  "routing": {"master_left": 0, "master_right": 1,
                              "cue_left": 2, "cue_right": 3}}))
PY
pd call audio.start --args "$(cat /tmp/pulsedeck-audio-routing.json)"
pd status
```

Check engine running, frame advance, actual device/configuration and cue routing after start. Confirm physical channel mapping and audibility by listening/hardware observation; advertised channel count does not prove the expected headphone connector assignment.

## MIDI and FLX4

Choose exact names from `midi ports`, not a guessed display name. Check `status.result.midi` for selected ports, `connected`, `feedback_enabled`, received/dropped messages and `last_error`. An open connection is software evidence; USB communication and LEDs require actual hardware activity. Empty ports means no observed available endpoint, not a reason to fabricate a successful connection or continuously reconnect.

When the observed ports are literally named DDJ-FLX4 and connection is requested:

```sh
pd midi connect --input 'DDJ-FLX4' --output 'DDJ-FLX4'
pd status
pd midi history
```

After a user physically moves a control, inspect message-count/history change and resulting engine state; distinguish that from invoking synthetic `controller.action`. If input is present without output, connect the selected input alone and report feedback unavailable. Diagnose disappearance, dropped messages and last error before reconnecting once; do not disconnect a working controller just to test it. Check returned port names rather than assuming auto-discovery selected the requested FLX4.

## Failed preparation, jobs and authentication

```sh
pd commands describe engine.wait
pd deck B wait --expected-load-revision "$B_REV" --timeout 30s
pd jobs list
pd spotify status
pd spotify native status
```

For an observed job ID, inspect `jobs get` before retrying:

```sh
export JOB_ID='job-123-0'       # Replace with the actual accepted job_id.
pd jobs get "$JOB_ID"
pd jobs wait "$JOB_ID" --timeout 30s
pd status
```

`jobs wait` can return an `ok:true` query envelope while exiting 5 and reporting `result.state:"failed"`. Check process exit code **and** job state/results. Earlier successful batch steps remain applied; later steps after failure were skipped. A timeout is uncertain completion, not permission to submit the mutation again. Re-read state/receipts/source jobs; use the same request ID for an identical uncertain mutation when applicable. Fix malformed arguments from returned hints/schema. For missing-file errors, verify path and source identity before another load; for revision conflicts, reassess the changed source before using a new guard. Keep the live deck playing while incoming preparation fails; prefer a simpler supported transition or report inability rather than waiting through the outgoing end for a failed job.

Spotify library/Connect login and native deck-streaming login are separate. Inspect each `configured`, `connected`, `authorizing`, `restoring`, permissions, `last_error` and setup notice. A configured native client ID does not mean signed in. Library success does not establish native Load A/B eligibility; Connect play controls a provider device, not local deck transport. Explain the missing prerequisite and retry only after it changes. Configure/login only when the user requests account setup, using their client ID and observed registered redirect URI; don't disconnect a working account or open browser authorization as a speculative repair. Never bypass source recording restrictions.

## Takeover and output

On “Stop automation; I'm taking over”:

```sh
pd automation stop
pd jobs list
pd status
```

Verify active ramps are cleared, holds released and relevant jobs no longer progress. This does not pause transport or stop recording. Cancelling a job stops future batch steps and interrupts engine waits; it does not universally abort an active load/import/provider call. Use discovered operation-specific cancellation when needed, then inspect whether that operation actually stopped. Stop only the automation within the requested session; do not quietly restart it after manual intervention.

Return: observed symptom and source/session identity; likely cause with supporting state; targeted changes and their completion stages; verified resulting controls, transport and jobs; unresolved issues and the specific listening, device or account test required. For planning-only requests, give proposed commands and their conditions without applying them. Say “software signal/routing verified” when that is all the evidence supports, never “I listened” or “headphones work” from JSON alone.
