# Timed performances and continuity

Choose direct controls, individually selected native moves, a persistent command session, or the optional successor queue as the performance needs. These are execution choices; they do not dictate songs, phrasing, tempo relationships or techniques.

## Keep the requested finish time

`mix.session.start` owns an application-monotonic endpoint, independent of either song's clock. It accepts `duration_seconds` or an absolute `finish_unix_ms`; `prepared_id` is optional. Start it once while a ready carrier is playing. Keep the absolute finish timestamp for recovery instead of starting another full duration. A session never resumes playback after an app restart.

```sh
pd call mix.session.start --args '{"duration_seconds":3600}'
pd call mix.session.status --args '{}'
```

CLI performance gestures (tempo, loops, EQ, faders, FX and ramps) preserve this endpoint. Human deck/mixer gestures yield the automation; headphone controls and positive master-level adjustments do not cancel it. Explicit session/queue/automation stops and a Pause of the only carrier revoke it. `mix.session.stop` cancels the endpoint and its pending owned move while ordinary music keeps playing; pause the decks too when the user requests music to stop.

A failed transition or missing audible route is attention, not successful completion. The timer remains active through a recoverable gap and still pauses the decks at the original endpoint. Engine stop/restart, stalled audio or an audio device error interrupts the session. Inspect `continuity_notice`, the actual decks and the event stream. Do not resurrect playback after a genuine user stop.

## Prepare a chosen successor ahead

Once the incoming song and entry are chosen, load and cue it while the released deck is isolated. Plan the normal musical transition before the current carrier exhausts its usable material. A loaded track or Play behind a closed fader is not an audible handover. Native moves report completion when their final controls and role transfer have been verified; use that event to prepare the released deck.

For direct performance, `mix.session.successor` optionally arms that prepared source as a one-shot emergency handover. It requires an active timed session, a ready paused isolated target and a playing carrier. It does not require analysed phrase/grid evidence. For example, after preparing B:

```sh
pd call mix.session.successor --args '{"deck":"B"}'
```

The normal mix takes priority. If the carrier reaches its last second without an active native transition, this fallback disables incoming Sync, restores its gain/EQ/filter/FX to neutral, plays its current prepared entry and opens its fader. It pauses the outgoing carrier only after the incoming route succeeds. This prevents EOF silence; it is not a phrase-aware blend. It preserves the current output/crossfader route and reports failure if that route cannot carry the incoming track.

Loading, seeking or playing the target invalidates the reservation. A normal handover consumes it without replay. Rearm after choosing/changing the next entry. Cancel with `mix.session.successor` arguments `deck` and `cancel:true`, or stop the session. It never substitutes an arbitrary idle track.

## Follow actionable feedback

`mix.session.updated` reports `successor_due` when an unarmed carrier has at most a minute remaining, `no_audible_carrier_route` for a gap, and the armed fallback's result. These are prompts to assess the live state, not musical timing rules. Use `events.get` with `session_id` and the last `since` cursor for bounded event waits. A passive observer log alone does not wake an idle performer.

If silence occurs, restore sound from the intended ready source using current controls rather than an ended outgoing clock. Preserve the original endpoint, report the gap and continue only while the performance remains authorized. Runtime receipts establish execution; listening and musical judgment establish the blend's quality.
