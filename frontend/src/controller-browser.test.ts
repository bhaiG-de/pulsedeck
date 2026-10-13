import { test } from "node:test";
import assert from "node:assert/strict";
import {
  browseIndex,
  browserPane,
  isBrowserAction,
  isInstantDouble,
  zoomBeats,
  navigateBrowser,
  type BrowserNavigation,
} from "./controller-browser.ts";
test("browse follows row identity after filtering and clamps at either end", () => {
  assert.equal(browseIndex(["a", "b", "c"], "b", 1), 2);
  assert.equal(browseIndex(["c", "b"], "b", -1), 0);
  assert.equal(browseIndex(["a", "b"], null, 1), 0);
  assert.equal(browseIndex(["a", "b"], null, -1), 0);
  assert.equal(browseIndex(["a", "b"], "b", 40), 1);
  assert.equal(browseIndex([], null, 1), -1);
});
test("knob traverses footer, playlists, songs, playlists, close, then footer", () => {
  let state: BrowserNavigation = { layer: "home", returning: false };
  state = navigateBrowser(state, "rotate");
  assert.equal(state.layer, "footer");
  for (const expected of [
    "playlists",
    "tracks",
    "playlists",
    "close",
    "footer",
  ]) {
    state = navigateBrowser(state, "press");
    assert.equal(state.layer, expected);
  }
});
test("back escapes every browser layer and rotating playlists starts a new inward path", () => {
  let state = navigateBrowser({ layer: "tracks", returning: false }, "back");
  assert.deepEqual(state, { layer: "playlists", returning: true });
  assert.equal(navigateBrowser(state, "back").layer, "close");
  assert.equal(
    navigateBrowser(navigateBrowser(state, "back"), "back").layer,
    "footer",
  );
  state = navigateBrowser(state, "rotate");
  assert.equal(navigateBrowser(state, "press").layer, "tracks");
  assert.equal(
    navigateBrowser({ layer: "close", returning: true }, "rotate").layer,
    "playlists",
  );
});
test("shifted browse zoom stays within the shared waveform zoom bounds", () => {
  assert.equal(zoomBeats(32, 1), 16);
  assert.equal(zoomBeats(32, -2), 128);
  assert.equal(zoomBeats(8, 8), 8);
  assert.equal(zoomBeats(128, -8), 128);
});
test("browser action classifier ignores unrelated knobs and invalid decks", () => {
  assert.ok(isBrowserAction({ type: "load", deck: 1, shifted: false }));
  assert.ok(
    isBrowserAction({
      type: "global_button",
      button: "browse_press",
      pressed: false,
    }),
  );
  assert.ok(
    isBrowserAction({
      type: "global_button",
      button: "panel_forward",
      pressed: true,
    }),
  );
  assert.equal(isBrowserAction({ type: "knob", knob: "trim" }), false);
  assert.equal(isBrowserAction({ type: "load", deck: 3 }), false);
  assert.equal(isBrowserAction({ type: "browse", delta: NaN }), false);
});

test("instant doubles require two presses on the same deck inside 500ms", () => {
  assert.equal(isInstantDouble(null, 0, 100), false);
  assert.equal(isInstantDouble({ deck: 0, time: 100 }, 0, 599), true);
  assert.equal(isInstantDouble({ deck: 0, time: 100 }, 0, 600), false);
  assert.equal(isInstantDouble({ deck: 0, time: 100 }, 1, 200), false);
  assert.equal(isInstantDouble({ deck: 0, time: 100 }, 0, 99), false);
});

test("push enters songs from playlists and returns from the deepest song pane", () => {
  assert.equal(browserPane("playlists", "press"), "tracks");
  assert.equal(browserPane("tracks", "press"), "playlists");
  assert.equal(browserPane("tracks", "back"), "playlists");
  assert.equal(browserPane("playlists", "back"), "playlists");
});
test("playlist and song wheel selections stay independent when focus changes", () => {
  const playlists = ["Liked", "House", "Techno"];
  const songs = ["one", "two", "three"];
  const playlistIndex = browseIndex(playlists, "Liked", 1);
  assert.equal(playlistIndex, 1);
  assert.equal(browserPane("playlists", "press"), "tracks");
  const songIndex = browseIndex(songs, null, 1);
  assert.equal(songIndex, 0); // Opening rotation is applied, not dropped.
  assert.equal(browserPane("tracks", "press"), "playlists");
  assert.equal(browseIndex(playlists, playlists[playlistIndex], 1), 2);
  assert.equal(songs[songIndex], "one");
});
