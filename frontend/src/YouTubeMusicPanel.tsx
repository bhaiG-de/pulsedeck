import { createPortal } from "react-dom";
import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronLeft,
  ExternalLink,
  Heart,
  ListMusic,
  LoaderCircle,
  RefreshCw,
  Settings2,
  X,
} from "lucide-react";
import { isDesktop, control, openExternal } from "./api";
import VirtualTrackRows from "./VirtualTrackRows";
import TrackArtwork from "./TrackArtwork";
import {
  browseIndex,
  browserPane,
  scrollBrowserSelection,
  type BrowserLayer,
  type BrowserPane,
  type ControllerBrowserAction,
  type ControllerBrowserHandle,
} from "./controller-browser";

interface YouTubeMusicStatus {
  configured: boolean;
  connected: boolean;
  authorizing?: boolean;
  restoring?: boolean;
  client_id?: string | null;
  account?: string | null;
  last_error?: string | null;
  spotify_matching?: boolean;
}
interface YouTubePlaylist {
  id: string;
  name: string;
  total: number | null;
  artwork_url?: string | null;
  external_url: string;
}
export interface YouTubeMatch {
  provider: "local" | "spotify";
  source: string;
  title: string;
  artists: string[];
  duration_ms: number;
  artwork_url?: string | null;
  confidence: number;
}
export interface YouTubeTrack {
  video_id: string;
  title: string;
  artists: string[];
  raw_title: string;
  channel: string;
  duration_ms: number;
  artwork_url?: string | null;
  external_url: string;
  music: boolean;
  match: YouTubeMatch | null;
  match_checked: boolean;
}
interface YouTubePage<T> {
  items: T[];
  next_page_token: string | null;
  total: number | null;
}
interface MatchReport {
  results: { video_id: string; match: YouTubeMatch | null }[];
  cancelled: boolean;
  error: string | null;
}
type Collection = YouTubePlaylist | "liked";

// Small requests keep rows updating while a page is matched.
const MATCH_CHUNK = 10;
const formatTime = (ms: number) =>
  ms > 0
    ? `${Math.floor(ms / 60000)}:${Math.floor((ms / 1000) % 60)
        .toString()
        .padStart(2, "0")}`
    : "—";

function YouTubeMusicPanel({
  active = false,
  collectionHost,
  onActivate,
  settingsOnly = false,
  onLoadSpotify,
  onLoadLocal,
  deckJobs = [],
  registerControllerBrowser,
  controllerLayer,
  onBrowserPaneChange,
}: {
  active?: boolean;
  collectionHost?: HTMLElement | null;
  onActivate?: () => void;
  settingsOnly?: boolean;
  onLoadSpotify?: (
    deck: number,
    track: { uri: string; title: string },
  ) => Promise<void>;
  onLoadLocal?: (deck: number, path: string) => void | Promise<void>;
  deckJobs?: { preparing: boolean }[];
  registerControllerBrowser?: (handle: ControllerBrowserHandle | null) => void;
  controllerLayer?: BrowserLayer;
  onBrowserPaneChange?: (pane: BrowserPane) => void;
}) {
  const [status, setStatus] = useState<YouTubeMusicStatus>({
    configured: false,
    connected: false,
  });
  const [nativeSpotify, setNativeSpotify] = useState(false);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [showSetup, setShowSetup] = useState(settingsOnly);
  const [loading, setLoading] = useState(false);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistToken, setPlaylistToken] = useState<string | null>(null);
  const [selected, setSelected] = useState<Collection | null>(null);
  const [tracks, setTracks] = useState<YouTubeTrack[]>([]);
  const [trackToken, setTrackToken] = useState<string | null>(null);
  const [trackTotal, setTrackTotal] = useState<number | null>(null);
  const [matchingLeft, setMatchingLeft] = useState(0);
  const [matchingIds, setMatchingIds] = useState<Set<string>>(new Set());
  const generation = useRef(0);
  const matchQueue = useRef<{ ids: string[]; force: boolean }[]>([]);
  const matchWorker = useRef(false);
  const [browserSelection, updateBrowserSelection] = useState<string | null>(
    null,
  );
  const selectedRow = useRef<string | null>(null);
  const setBrowserSelection = useCallback((id: string | null) => {
    selectedRow.current = id;
    updateBrowserSelection(id);
  }, []);
  const [collectionFocus, updateCollectionFocus] = useState(true);
  const collectionFocusRef = useRef(true);
  const setCollectionFocus = useCallback((value: boolean) => {
    collectionFocusRef.current = value;
    updateCollectionFocus(value);
  }, []);
  const [collectionSelection, updateCollectionSelection] = useState("liked");
  const collectionSelectionRef = useRef("liked");
  const setCollectionSelection = useCallback((id: string) => {
    collectionSelectionRef.current = id;
    updateCollectionSelection(id);
  }, []);
  const playlistsPane = useRef<HTMLElement>(null);
  const tracksPane = useRef<HTMLDivElement>(null);
  const fail = useCallback(
    (e: unknown) => setError(String(e instanceof Error ? e.message : e)),
    [],
  );

  const refreshStatus = useCallback(async () => {
    if (!isDesktop) return;
    try {
      const next = await control<YouTubeMusicStatus>("ytmusic.status");
      setStatus(next);
      setClientId((old) => old || next.client_id || "");
      const native = await control<{ connected: boolean }>(
        "spotify.native.status",
      );
      setNativeSpotify(native.connected);
    } catch (e) {
      fail(e);
    }
  }, [fail]);
  useEffect(() => {
    if (active || settingsOnly) void refreshStatus();
  }, [active, settingsOnly, refreshStatus]);
  useEffect(() => {
    if (!status.authorizing && !status.restoring) return;
    const timer = setInterval(refreshStatus, 1500);
    return () => clearInterval(timer);
  }, [status.authorizing, status.restoring, refreshStatus]);

  const loadPlaylists = useCallback(
    async (token: string | null = null) => {
      setLoading(true);
      try {
        const page = await control<YouTubePage<YouTubePlaylist>>(
          "ytmusic.playlists",
          { page_token: token },
        );
        setPlaylists((old) => (token ? [...old, ...page.items] : page.items));
        setPlaylistToken(page.next_page_token);
      } catch (e) {
        fail(e);
      } finally {
        setLoading(false);
      }
    },
    [fail],
  );
  useEffect(() => {
    if (!settingsOnly && status.connected && active && !playlists.length)
      void loadPlaylists();
  }, [settingsOnly, status.connected, active, playlists.length, loadPlaylists]);

  // One request at a time: the backend lets a newer match supersede an older one.
  const runMatches = useCallback(async () => {
    if (matchWorker.current) return;
    matchWorker.current = true;
    const left = () =>
      matchQueue.current.reduce((n, job) => n + job.ids.length, 0);
    try {
      while (matchQueue.current.length) {
        const job = matchQueue.current.shift()!;
        setMatchingIds(new Set(job.ids));
        setMatchingLeft(left() + job.ids.length);
        const report = await control<MatchReport>("ytmusic.match", {
          video_ids: job.ids,
          force: job.force,
        });
        const found = new Map(report.results.map((r) => [r.video_id, r.match]));
        setTracks((old) =>
          old.map((track) =>
            found.has(track.video_id)
              ? {
                  ...track,
                  match: found.get(track.video_id) ?? null,
                  match_checked: true,
                }
              : track,
          ),
        );
        if (report.error || report.cancelled) {
          if (report.error) fail(report.error);
          matchQueue.current = [];
        }
      }
    } catch (e) {
      fail(e);
      matchQueue.current = [];
    } finally {
      matchWorker.current = false;
      setMatchingIds(new Set());
      setMatchingLeft(0);
    }
  }, [fail]);
  const queueMatches = useCallback(
    (ids: string[], force = false) => {
      const unique = [...new Set(ids)];
      for (let i = 0; i < unique.length; i += MATCH_CHUNK)
        matchQueue.current.push({
          ids: unique.slice(i, i + MATCH_CHUNK),
          force,
        });
      if (unique.length) void runMatches();
    },
    [runMatches],
  );

  const openCollection = useCallback(
    async (collection: Collection, token: string | null = null) => {
      const request = token ? generation.current : ++generation.current;
      if (!token) {
        // Songs from the previous playlist no longer need matching.
        matchQueue.current = [];
        setSelected(collection);
        setTracks([]);
        setTrackToken(null);
        setTrackTotal(null);
        setBrowserSelection(null);
      }
      setLoading(true);
      try {
        const page = await control<YouTubePage<YouTubeTrack>>(
          "ytmusic.tracks",
          {
            playlist_id: collection === "liked" ? "liked" : collection.id,
            page_token: token,
          },
        );
        if (generation.current !== request) return;
        setTracks((old) => (token ? [...old, ...page.items] : page.items));
        setTrackToken(page.next_page_token);
        setTrackTotal(page.total);
        queueMatches(
          page.items
            .filter((t) => t.music && !t.match_checked)
            .map((t) => t.video_id),
        );
      } catch (e) {
        if (generation.current === request) fail(e);
      } finally {
        if (generation.current === request) setLoading(false);
      }
    },
    [fail, queueMatches, setBrowserSelection],
  );
  const loadMoreTracks = useCallback(() => {
    if (selected && trackToken && !loading)
      void openCollection(selected, trackToken);
  }, [selected, trackToken, loading, openCollection]);

  const loadMatch = useCallback(
    async (deck: number, track: YouTubeTrack) => {
      const found = track.match;
      if (!found) return;
      try {
        if (found.provider === "local") await onLoadLocal?.(deck, found.source);
        else if (!nativeSpotify)
          fail(
            "Sign in to the Spotify control player in Settings › Spotify to load Spotify matches.",
          );
        else
          await onLoadSpotify?.(deck, {
            uri: found.source,
            title: found.title,
          });
      } catch (e) {
        fail(e);
      }
    },
    [fail, nativeSpotify, onLoadLocal, onLoadSpotify],
  );

  const browserHandler = useRef<
    (action: ControllerBrowserAction) => void | Promise<void>
  >(() => {});
  useEffect(() => {
    if (!registerControllerBrowser || settingsOnly) return;
    registerControllerBrowser({
      handle: (action) => browserHandler.current(action),
    });
    return () => registerControllerBrowser(null);
  }, [registerControllerBrowser, settingsOnly]);
  useEffect(() => {
    if (controllerLayer === "playlists" || controllerLayer === "tracks")
      setCollectionFocus(controllerLayer === "playlists");
  }, [controllerLayer, setCollectionFocus]);
  useEffect(() => {
    if (!collectionFocus) return; // Virtual rows reveal off-screen track selections.
    const row = [
      ...(playlistsPane.current?.querySelectorAll("[data-browser-id]") ?? []),
    ].find((el) => el.getAttribute("data-browser-id") === collectionSelection);
    scrollBrowserSelection(row ?? null);
  }, [collectionFocus, collectionSelection]);
  const chosenCollection = (): Collection | undefined =>
    collectionSelectionRef.current === "liked"
      ? "liked"
      : playlists.find((p) => p.id === collectionSelectionRef.current);
  browserHandler.current = async (action) => {
    if (action.type === "browse") {
      if (action.shifted) return;
      if (collectionFocusRef.current || !selected) {
        const ids = ["liked", ...playlists.map((p) => p.id)];
        const index = browseIndex(
          ids,
          collectionSelectionRef.current,
          action.delta,
        );
        if (index >= 0) setCollectionSelection(ids[index]);
      } else {
        const ids = tracks.map((t) => t.video_id);
        const index = browseIndex(ids, selectedRow.current, action.delta);
        if (index >= 0) setBrowserSelection(ids[index]);
      }
    } else if (action.type === "global_button" && action.pressed) {
      const next = browserPane(
        collectionFocusRef.current ? "playlists" : "tracks",
        action.button === "browse_back" ? "back" : "press",
      );
      if (next === "playlists") {
        setCollectionFocus(true);
        return;
      }
      const chosen = chosenCollection();
      if (!chosen) return;
      const currentId = selected === "liked" ? "liked" : selected?.id;
      if (currentId !== collectionSelectionRef.current || !tracks.length)
        await openCollection(chosen);
      setCollectionFocus(false);
    } else if (action.type === "load" && !action.shifted) {
      const track =
        tracks.find((t) => t.video_id === selectedRow.current) ?? tracks[0];
      if (!track?.match) {
        fail("Select a YouTube Music song that has a local or Spotify match.");
        return;
      }
      await loadMatch(action.deck, track);
    }
  };

  const configure = async () => {
    setBusy(true);
    setError("");
    try {
      setStatus(
        await control<YouTubeMusicStatus>("ytmusic.configure", {
          client_id: clientId.trim(),
          client_secret: clientSecret.trim(),
        }),
      );
      setClientSecret("");
      setPlaylists([]);
      setTracks([]);
      setSelected(null);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  const login = async () => {
    setBusy(true);
    setError("");
    try {
      await control("ytmusic.login");
      await refreshStatus();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };
  const disconnect = async () => {
    try {
      matchQueue.current = [];
      setStatus(await control<YouTubeMusicStatus>("ytmusic.disconnect"));
      setPlaylists([]);
      setTracks([]);
      setSelected(null);
    } catch (e) {
      fail(e);
    }
  };

  const setup = showSetup || !status.connected;
  const collectionTree = status.connected && (
    <aside
      className={`controller-playlists-pane ${collectionFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
      aria-label="YouTube Music playlists"
      ref={playlistsPane}
    >
      <div className="browser-pane-heading">
        PLAYLISTS{" "}
        <button
          className="subtle"
          aria-label="Reload YouTube Music playlists"
          onClick={() => loadPlaylists()}
          disabled={loading}
        >
          <RefreshCw size={12} />
        </button>
      </div>
      {(["liked", ...playlists] as Collection[]).map((collection) => {
        const id = collection === "liked" ? "liked" : collection.id;
        return (
          <button
            key={id}
            className={`spotify-playlist ${collection === "liked" ? "liked" : ""} ${collectionSelection === id ? "controller-selected" : ""}`}
            data-browser-id={id}
            aria-pressed={collectionSelection === id}
            onClick={() => {
              onActivate?.();
              setCollectionFocus(true);
              onBrowserPaneChange?.("playlists");
              setCollectionSelection(id);
              void openCollection(collection);
            }}
          >
            {collection === "liked" ? (
              <>
                <Heart size={16} />
                <strong>Liked videos</strong>
              </>
            ) : (
              <>
                <TrackArtwork url={collection.artwork_url} />
                <strong>{collection.name}</strong>
                <span>{collection.total ?? ""}</span>
              </>
            )}
          </button>
        );
      })}
      {playlistToken && (
        <button
          className="subtle"
          disabled={loading}
          onClick={() => loadPlaylists(playlistToken)}
        >
          More playlists
        </button>
      )}
    </aside>
  );

  const matchCell = (track: YouTubeTrack) => {
    if (matchingIds.has(track.video_id))
      return (
        <span className="ytmusic-match pending">
          <LoaderCircle size={11} className="spin" /> Matching…
        </span>
      );
    if (track.match)
      return (
        <span
          className="ytmusic-match"
          title={`${Math.round(track.match.confidence * 100)}% match · ${track.match.provider === "local" ? track.match.source : "Spotify catalog"}`}
        >
          <i className={`ytmusic-provider ${track.match.provider}`}>
            {track.match.provider === "local" ? "LOCAL" : "SPOTIFY"}
          </i>
          <span>
            {track.match.title}
            {track.match.artists.length
              ? ` — ${track.match.artists.join(", ")}`
              : ""}
          </span>
        </span>
      );
    return (
      <span className="ytmusic-match missing">
        {track.match_checked
          ? "No match"
          : track.music
            ? "Waiting to match"
            : "Not a music upload"}
      </span>
    );
  };

  return (
    <section
      className={`spotify-panel ytmusic-panel ${settingsOnly ? "spotify-settings" : "spotify-compact"}`}
    >
      {!settingsOnly && (
        <div className="spotify-heading">
          <div className="spotify-title">
            <span className="spotify-symbol">
              <ListMusic size={22} />
            </span>
            <div>
              <h2>YouTube Music</h2>
              <p>Imported playlists · plays through local or Spotify matches</p>
            </div>
          </div>
          <div>
            <span
              className={`spotify-status ${status.connected ? "connected" : ""}`}
            >
              <i className="tiny-dot" />
              {status.connected
                ? status.account || "Account connected"
                : status.restoring
                  ? "Restoring sign-in"
                  : status.authorizing
                    ? "Waiting for sign-in"
                    : "Not connected"}
            </span>
            <button
              className="icon-button"
              aria-label="YouTube Music account settings"
              onClick={() => setShowSetup(!showSetup)}
            >
              <Settings2 size={17} />
            </button>
          </div>
        </div>
      )}
      {error && (
        <div className="spotify-error" role="alert">
          <span>{error}</span>
          <button
            aria-label="Dismiss YouTube Music error"
            onClick={() => setError("")}
          >
            <X size={13} />
          </button>
        </div>
      )}
      {setup ? (
        <div className="spotify-setup">
          <div className="settings-section-title">
            <ListMusic size={18} />
            <div>
              <h3>
                {status.connected
                  ? "YouTube Music account"
                  : "Connect YouTube Music"}
              </h3>
              <p>
                Read-only Google sign-in. Songs play through matching local
                files or Spotify tracks; YouTube audio is never downloaded.
              </p>
            </div>
          </div>
          <ol className="spotify-steps">
            <li>
              In the{" "}
              <a
                href="https://console.cloud.google.com/apis/library/youtube.googleapis.com"
                onClick={(e) =>
                  openExternal(e, e.currentTarget.href).catch(fail)
                }
                target="_blank"
                rel="noreferrer"
              >
                Google Cloud console <ExternalLink size={10} />
              </a>
              , enable YouTube Data API v3 for a project.
            </li>
            <li>
              Configure the OAuth consent screen as External, keep it in
              Testing, and add your Google account as a test user.
            </li>
            <li>
              Create an OAuth client ID of type <strong>Desktop app</strong>,
              paste its Client ID and secret here, save, then sign in.
            </li>
          </ol>
          <label className="field">
            Google OAuth Client ID
            <input
              autoComplete="off"
              placeholder="…apps.googleusercontent.com"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              spellCheck={false}
            />
          </label>
          <label className="field">
            Client secret
            <input
              type="password"
              autoComplete="off"
              placeholder={
                status.configured
                  ? "Saved · enter it again to change the client"
                  : "Client secret of the Desktop app client"
              }
              value={clientSecret}
              onChange={(e) => setClientSecret(e.target.value)}
              spellCheck={false}
            />
          </label>
          <div className="spotify-setup-actions">
            <button
              className="secondary"
              disabled={
                busy || !clientId.trim() || !clientSecret.trim() || !isDesktop
              }
              onClick={configure}
            >
              {busy ? (
                <LoaderCircle size={13} className="spin" />
              ) : (
                <Check size={13} />
              )}
              Save client
            </button>
            {!status.connected && (
              <button
                className="spotify-primary"
                disabled={
                  busy || !status.configured || status.restoring || !isDesktop
                }
                onClick={login}
              >
                <ExternalLink size={13} />
                {status.authorizing ? "Restart sign-in" : "Sign in with Google"}
              </button>
            )}
            {status.connected && (
              <button className="secondary" onClick={disconnect}>
                Disconnect account
              </button>
            )}
            {status.connected && !settingsOnly && (
              <button className="secondary" onClick={() => setShowSetup(false)}>
                Back to YouTube Music
              </button>
            )}
          </div>
          {status.authorizing && (
            <div className="spotify-auth-note">
              <LoaderCircle size={14} className="spin" />
              <span>
                Complete Google sign-in in your browser, then return here.{" "}
                <button onClick={refreshStatus}>Check connection</button>
              </span>
            </div>
          )}
          {status.last_error && (
            <p className="spotify-analysis-error" role="alert">
              {status.last_error}
            </p>
          )}
          <p className="help-text">
            Matching checks your local library first, then the Spotify catalog.{" "}
            {status.spotify_matching
              ? "Your Spotify library account is connected, so catalog matching is on."
              : "Connect your Spotify library account in Settings › Spotify to also match against the catalog."}{" "}
            Loading Spotify matches also needs the Spotify control player
            sign-in. Google ends Testing-mode sign-ins after 7 days; sign in
            again when asked.
          </p>
          {!isDesktop && (
            <p className="help-text">
              Open the PulseDeck desktop app to connect your account.
            </p>
          )}
        </div>
      ) : (
        <>
          <div className="controller-browser-split">
            {collectionHost
              ? collectionTree && createPortal(collectionTree, collectionHost)
              : collectionTree}
            <div
              className={`controller-tracks-pane ${!collectionFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
              data-browser-pane="ytmusic-tracks"
              ref={tracksPane}
              tabIndex={-1}
              onPointerDown={() => {
                setCollectionFocus(false);
                onBrowserPaneChange?.("tracks");
              }}
            >
              {selected ? (
                <>
                  <div className="spotify-list-heading">
                    <button
                      className="subtle"
                      onClick={() => {
                        setCollectionFocus(true);
                        onBrowserPaneChange?.("playlists");
                      }}
                    >
                      <ChevronLeft size={13} />
                      Playlists
                    </button>
                    <h3>
                      {selected === "liked" ? "Liked videos" : selected.name}
                    </h3>
                    <span>
                      {trackTotal ?? tracks.length} videos
                      {matchingLeft > 0 && ` · matching ${matchingLeft}…`}
                    </span>
                  </div>
                  {!status.spotify_matching && (
                    <p className="spotify-boundary">
                      Matching uses local files only. Connect your Spotify
                      library account to also match against the Spotify catalog.
                    </p>
                  )}
                  <div className="spotify-tracks ytmusic-tracks">
                    <table role="table" className="track-table">
                      <thead>
                        <tr role="row">
                          <th role="columnheader">#</th>
                          <th role="columnheader">YOUTUBE MUSIC</th>
                          <th role="columnheader">PLAYS AS</th>
                          <th role="columnheader">TIME</th>
                          <th role="columnheader">LOAD</th>
                        </tr>
                      </thead>
                      <VirtualTrackRows
                        count={tracks.length}
                        getKey={(i) => `${tracks[i].video_id}-${i}`}
                        scrollRef={tracksPane}
                        selectedIndex={tracks.findIndex(
                          (t) => t.video_id === browserSelection,
                        )}
                        focusSelection={
                          !collectionFocus && controllerLayer === "tracks"
                        }
                        queryKey={selected === "liked" ? "liked" : selected.id}
                        hasMore={trackToken !== null}
                        loading={loading}
                        onMore={loadMoreTracks}
                      >
                        {(i, measure) => {
                          const track = tracks[i];
                          const found = track.match;
                          const blocked = (deck: number) =>
                            !found ||
                            deckJobs[deck]?.preparing ||
                            (found.provider === "spotify" && !nativeSpotify);
                          return (
                            <tr
                              ref={measure}
                              data-index={i}
                              aria-rowindex={i + 2}
                              key={`${track.video_id}-${i}`}
                              data-browser-id={track.video_id}
                              tabIndex={-1}
                              aria-selected={
                                browserSelection === track.video_id
                              }
                              className={[
                                browserSelection === track.video_id
                                  ? "selected controller-selected"
                                  : "",
                                found ? "" : "ytmusic-unmatched",
                              ]
                                .filter(Boolean)
                                .join(" ")}
                              onClick={() => {
                                setBrowserSelection(track.video_id);
                                setCollectionFocus(false);
                              }}
                              draggable={!!found}
                              onDragStart={(e) => {
                                if (!found) return;
                                e.dataTransfer.effectAllowed = "copy";
                                if (found.provider === "local")
                                  e.dataTransfer.setData(
                                    "application/pulsedeck-track",
                                    found.source,
                                  );
                                else
                                  e.dataTransfer.setData(
                                    "application/pulsedeck-spotify-track",
                                    JSON.stringify({
                                      uri: found.source,
                                      title: found.title,
                                    }),
                                  );
                              }}
                            >
                              <td className="track-number">{i + 1}</td>
                              <td>
                                <div className="track-cell">
                                  <TrackArtwork url={track.artwork_url} />
                                  <div>
                                    <strong title={track.raw_title}>
                                      {track.title}
                                    </strong>
                                    <span>
                                      {track.artists.join(", ") ||
                                        track.channel}
                                    </span>
                                  </div>
                                </div>
                              </td>
                              <td>{matchCell(track)}</td>
                              <td className="numeric">
                                {formatTime(track.duration_ms)}
                              </td>
                              <td className="spotify-track-actions">
                                <button
                                  className="spotify-play load-a"
                                  aria-label="Load on deck A"
                                  disabled={blocked(0)}
                                  onClick={() => void loadMatch(0, track)}
                                  title="Load the matched track on deck A"
                                >
                                  A
                                </button>
                                <button
                                  className="spotify-play load-b"
                                  aria-label="Load on deck B"
                                  disabled={blocked(1)}
                                  onClick={() => void loadMatch(1, track)}
                                  title="Load the matched track on deck B"
                                >
                                  B
                                </button>
                                <button
                                  className="ytmusic-rematch"
                                  aria-label={
                                    track.match_checked
                                      ? "Search for a match again"
                                      : "Find a match"
                                  }
                                  title={
                                    track.match_checked
                                      ? "Search for a match again"
                                      : "Find a match"
                                  }
                                  disabled={matchingIds.has(track.video_id)}
                                  onClick={() =>
                                    queueMatches(
                                      [track.video_id],
                                      track.match_checked,
                                    )
                                  }
                                >
                                  <RefreshCw size={11} />
                                </button>
                                <a
                                  className="ytmusic-open"
                                  href={track.external_url}
                                  onClick={(e) =>
                                    openExternal(e, track.external_url).catch(
                                      fail,
                                    )
                                  }
                                  target="_blank"
                                  rel="noreferrer"
                                  aria-label="Open in YouTube Music"
                                  title="Open in YouTube Music"
                                >
                                  <ExternalLink size={11} />
                                </a>
                              </td>
                            </tr>
                          );
                        }}
                      </VirtualTrackRows>
                    </table>
                    {!tracks.length && !loading && (
                      <p className="spotify-empty">
                        No videos are available in this playlist.
                      </p>
                    )}
                  </div>
                </>
              ) : (
                <p className="spotify-empty">
                  Choose a playlist to see its songs and how they play here.
                </p>
              )}
            </div>
          </div>
          {loading && (
            <div className="spotify-loading" role="status">
              <LoaderCircle size={16} className="spin" />
              Loading from YouTube Music…
            </div>
          )}
        </>
      )}
    </section>
  );
}

export default memo(YouTubeMusicPanel);
