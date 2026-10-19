import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EngineStore,
  stabilizeTrack,
  withTrackMetadata,
} from "./engine-state.ts";
import type { EngineSnapshot, TrackRecord } from "./types";
test("transport ticks retain waveform identity and clear explicitly unloaded metadata", () => {
  const waveform = [0.1, 0.2];
  const track: TrackRecord = {
    id: "a",
    path: "a",
    title: "a",
    waveform,
    waveform_bands: { low: waveform, mid: waveform, high: waveform },
  };
  const tick = {
    decks: [
      { position_seconds: 1, playing: true, track: null },
      { position_seconds: 0, track: null },
    ],
  } as EngineSnapshot;
  const first = withTrackMetadata(tick, [track, null]);
  const next = withTrackMetadata(
    {
      ...tick,
      decks: [{ ...tick.decks[0], position_seconds: 1.1 }, tick.decks[1]],
    },
    [track, null],
  );
  assert.equal(first.decks[0].track, next.decks[0].track);
  assert.equal(next.decks[0].track?.waveform, waveform);
  assert.equal(next.decks[0].position_seconds, 1.1);
  assert.equal(withTrackMetadata(tick, [null, null]).decks[0].track, null);
});
test("position and meter ticks do not notify application or control subscribers", () => {
  const store = new EngineStore();
  let root = 0,
    controls = 0,
    transport = 0;
  store.subscribeStructural(() => root++);
  store.subscribeControls(() => controls++);
  store.subscribeTransport(() => transport++);
  const initial = {
    decks: [
      {
        playing: true,
        position_seconds: 1,
        peak: [0.1, 0.2],
        channel_fader: 1,
        actual_playback_rate: 1,
        target_playback_bpm: 120,
        playback_bpm: 120,
        local_bpm: 120,
        actual_playback_bpm: 120,
      },
      {},
    ],
    load_revisions: [1, 0],
  };
  store.publish(initial, 100);
  const structure = store.getStructural(),
    control = store.getControls();
  store.publish(
    {
      ...initial,
      decks: [
        {
          ...initial.decks[0],
          position_seconds: 2,
          peak: [0.4, 0.6],
          actual_playback_rate: 1.005,
          playback_bpm: 120.2,
          local_bpm: 120.2,
          actual_playback_bpm: 120.8,
        },
        {},
      ],
    },
    133,
  );
  assert.equal(root, 1);
  assert.equal(controls, 1);
  assert.equal(transport, 2);
  assert.equal(store.getStructural(), structure);
  assert.equal(store.getControls(), control);
  assert.equal(store.getTransport().decks[0].position_seconds, 2);
  assert.equal(store.getTransport().decks[0].actual_playback_rate, 1.005);
  assert.equal(store.receivedAt(), 133);
  store.publish(
    { ...initial, decks: [{ ...initial.decks[0], channel_fader: 0.5 }, {}] },
    166,
  );
  assert.equal(root, 1);
  assert.equal(controls, 2);
  store.publish(
    { ...initial, decks: [{ ...initial.decks[0], playing: false }, {}] },
    199,
  );
  assert.equal(root, 2);
});
test("old metadata cannot be attached to a replacement or same-source reload", () => {
  const track = { id: "a", path: "a", title: "a" };
  const tick = { decks: [{ position_seconds: 1 }], load_revisions: [3] };
  assert.equal(withTrackMetadata(tick, [track], [2]).decks[0].track, null);
  assert.equal(withTrackMetadata(tick, [track], [3]).decks[0].track, track);
});
test("cue-only metadata retains raster inputs; changed audio invalidates them", () => {
  const old = {
    id: "a",
    path: "a",
    title: "a",
    waveform: [0.1, 0.2],
    waveform_bands: { low: [0.1, 0.2], mid: [0.2, 0.1], high: [0.3, 0.4] },
  };
  const copy = JSON.parse(JSON.stringify(old));
  copy.title = "renamed cue source";
  const next = stabilizeTrack(old, copy)!;
  assert.equal(next.waveform, old.waveform);
  assert.equal(next.waveform_bands, old.waveform_bands);
  copy.waveform[0] = 0.7;
  assert.notEqual(stabilizeTrack(old, copy)!.waveform, old.waveform);
});

test("native diagnostics and scheduled receipts do not reconcile controls on every tick", () => {
  const store = new EngineStore();
  let controls = 0;
  store.subscribeControls(() => controls++);
  const base = {
    running: false,
    sample_rate: 48000,
    decks: [{}, {}],
    scheduled_batch: { state: "idle", frame: 0 },
    audio_quality: { callback_count: 0 },
    sampler_positions_seconds: [null, null],
  };
  store.publish(base, 0);
  store.publish(
    {
      ...base,
      scheduled_batch: { state: "running", frame: 1 },
      audio_quality: { callback_count: 1 },
    },
    33,
  );
  assert.equal(controls, 1);
  let roots = 0;
  store.subscribeStructural(() => roots++);
  store.publish({ ...base, sampler_positions_seconds: [0.1, null] }, 66);
  store.publish({ ...base, sampler_positions_seconds: [0.2, null] }, 99);
  assert.equal(roots, 1);
});

test("variable device callback sizes stay in transport without reconciling controls or library", () => {
  const store = new EngineStore();
  let roots = 0,
    controls = 0;
  store.subscribeStructural(() => roots++);
  store.subscribeControls(() => controls++);
  const base = {
    running: true,
    sample_rate: 44100,
    decks: [{}, {}],
    buffer_frames: 278,
  };
  store.publish(base, 0);
  const root = store.getStructural(),
    state = store.getControls();
  store.publish({ ...base, buffer_frames: 279 }, 33);
  assert.equal(store.getTransport().buffer_frames, 279);
  assert.equal(store.getStructural(), root);
  assert.equal(store.getControls(), state);
  assert.equal(roots, 1);
  assert.equal(controls, 1);
});

test("mixer ramp ticks do not reconcile the deck's static controls", () => {
  const store = new EngineStore();
  const base = {
    running: true,
    decks: [
      { playing: true, channel_fader: 0, eq_low: 0, tempo_percent: 0 },
      {},
    ],
  };
  store.publish(base, 0);
  const deck = store.getDeckView(0);
  store.publish(
    {
      ...base,
      decks: [{ ...base.decks[0], channel_fader: 0.5, eq_low: 0.5 }, {}],
    },
    33,
  );
  assert.equal(store.getDeckView(0), deck);
  assert.equal(store.getControls().decks[0].channel_fader, 0.5);
  store.publish(
    { ...base, decks: [{ ...base.decks[0], tempo_percent: 2 }, {}] },
    66,
  );
  assert.notEqual(store.getDeckView(0), deck);
});
