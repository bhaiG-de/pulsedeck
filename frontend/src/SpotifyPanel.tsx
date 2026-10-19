import { createPortal } from "react-dom";
import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronLeft,
  ExternalLink,
  Headphones,
  Heart,
  LoaderCircle,
  Music2,
  Pause,
  Play,
  Radio,
  RefreshCw,
  Settings2,
  X,
} from "lucide-react";
import { isDesktop, control } from "./api";
import VirtualTrackRows from "./VirtualTrackRows";
import TrackArtwork from "./TrackArtwork";
import { useControl } from "./control-bridge";
import { controlString, controlBoolean, controlObject } from "./control-state";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { spotifyTrackId, spotifyWidgetUrl } from "./spotify-links";
import {
  browseIndex,
  scrollBrowserSelection,
  type ControllerBrowserAction,
  type ControllerBrowserHandle,
  browserPane,
  type BrowserLayer,
  type BrowserPane,
} from "./controller-browser";

export interface SpotifyStatus {
  configured: boolean;
  connected: boolean;
  authorizing?: boolean;
  restoring?: boolean;
  client_id?: string | null;
  account?: { id: string; display_name: string; external_url: string } | null;
  last_error?: string | null;
  registered_redirect_uri?: string;
  setup_notice?: string;
  playback_mode?: string;
  dj_streaming_available?: boolean;
  playback_permission?: boolean;
}
export interface SpotifyTrack {
  id: string;
  uri: string;
  title: string;
  artists: string[];
  album: string;
  album_id?: string | null;
  duration_ms: number;
  available: boolean;
  artwork_url?: string | null;
  external_url: string;
  is_local: boolean;
  playable_in_deck: false;
}
interface CachedAnalysis {
  uri: string;
  bpm: number | null;
  key: string | null;
  camelot: string | null;
  bpm_ambiguous: boolean;
  bpm_confidence: number;
  key_confidence: number | null;
}
interface BatchStatus {
  active: boolean;
  total: number;
  completed: number;
  cached: number;
  failed: number;
  cancelled?: boolean;
  current_uri?: string | null;
  last_error?: string | null;
}
export interface SpotifyPlaylist {
  id: string;
  name: string;
  description?: string;
  owner_id?: string;
  is_album?: boolean;
  total: number | null;
  external_url: string;
  artwork_url?: string | null;
  collaborative: boolean;
}
export interface SpotifyPage<T> {
  items: T[];
  total: number;
  offset: number;
  next_offset: number | null;
}
export interface SpotifyDevice {
  id: string | null;
  name: string;
  device_type: string;
  is_active: boolean;
  is_restricted: boolean;
  volume_percent: number | null;
}
const formatTime = (ms: number) =>
  `${Math.floor(ms / 60000)}:${Math.floor((ms / 1000) % 60)
    .toString()
    .padStart(2, "0")}`;

function SpotifyPanel({
  collectionHost,
  onActivate,
  settingsOnly = false,
  compact = false,
  onLoadDeck,
  deckJobs = [],
  registerControllerBrowser,
  controllerLayer,
  onBrowserPaneChange,
}: {
  collectionHost?: HTMLElement | null;
  onActivate?: () => void;
  settingsOnly?: boolean;
  compact?: boolean;
  registerControllerBrowser?: (handle: ControllerBrowserHandle | null) => void;
  controllerLayer?: BrowserLayer;
  onBrowserPaneChange?: (pane: BrowserPane) => void;
  onLoadDeck?: (deck: number, track: SpotifyTrack) => Promise<void>;
  deckJobs?: {
    preparing: boolean;
    uri?: string | null;
    error?: string | null;
  }[];
}) {
  const [status, setStatus] = useState<SpotifyStatus>({
    configured: false,
    connected: false,
  });
  const [nativeStatus, setNativeStatus] = useState<SpotifyStatus>({
    configured: true,
    connected: false,
  });
  const [widgetTrack, setWidgetTrack] = useState<{
    id: string;
    title: string;
  } | null>(null);
  const [widgetLink, setWidgetLink] = useState("");
  const [clientId, setClientId] = useState("");
  const [error, setError] = useState("");
  // A load may come from row buttons, a controller or drag-and-drop. A fresh
  // preparation supersedes the old drawer failure on every path; a new failure
  // is reported after this preparation ends.
  useEffect(() => {
    if (deckJobs[0]?.preparing || deckJobs[1]?.preparing) setError("");
  }, [
    deckJobs[0]?.preparing,
    deckJobs[0]?.uri,
    deckJobs[1]?.preparing,
    deckJobs[1]?.uri,
  ]);

  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [showSetup, setShowSetup] = useState(settingsOnly);
  const [playlists, setPlaylists] = useState<SpotifyPlaylist[]>([]);
  const [playlistTotal, setPlaylistTotal] = useState(0);
  const [playlistOffset, setPlaylistOffset] = useState<number | null>(null);
  const [tracks, setTracks] = useState<SpotifyTrack[]>([]);
  const [analysisPage, setAnalysisPage] = useState<SpotifyTrack[]>([]);
  const [analysisPageNumber, setAnalysisPageNumber] = useState(1);
  const [cachedAnalysis, setCachedAnalysis] = useState<
    Record<string, CachedAnalysis>
  >({});
  const [batch, setBatch] = useState<BatchStatus>({
    active: false,
    total: 0,
    completed: 0,
    cached: 0,
    failed: 0,
  });
  const [autoAnalyze, setAutoAnalyze] = useState(false);

  const [trackTotal, setTrackTotal] = useState(0);
  const [trackOffset, setTrackOffset] = useState<number | null>(null);
  const [selectedPlaylist, setSelectedPlaylist] = useState<
    SpotifyPlaylist | "liked" | null
  >(null);
  const [devices, setDevices] = useState<SpotifyDevice[]>([]);
  const [deviceId, setDeviceId] = useState("");
  const [playingUri, setPlayingUri] = useState("");
  const [authorization, setAuthorization] = useState<{
    authorization_url: string;
    redirect_uri: string;
  } | null>(null);
  const generation = useRef(0);
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
  const collectionFetchTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const setCollectionSelection = useCallback((id: string) => {
    collectionSelectionRef.current = id;
    updateCollectionSelection(id);
  }, []);
  const browserRoot = useRef<HTMLElement>(null);
  const tracksPane = useRef<HTMLDivElement>(null);
  const playlistsPane = useRef<HTMLElement>(null);
  const playlistSentinel = useRef<HTMLDivElement>(null);
  const playlistPending = useRef(false);
  const trackPending = useRef<{ key: string; request: number } | null>(null);
  const nextTrackPage = useRef<() => void>(() => {});
  const loadMoreTracks = useCallback(() => nextTrackPage.current(), []);
  const trackKey = useCallback(
    (index: number) => `${tracks[index].uri}-${index}`,
    [tracks],
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
    if (
      controllerLayer === "close" ||
      controllerLayer === "footer" ||
      controllerLayer === "home"
    )
      return;
    if (!collectionFocus) return; // Virtual rows reveal off-screen track selections.
    const focusedId = collectionSelection;
    if (!focusedId) {
      if (controllerLayer === "tracks")
        browserRoot.current
          ?.querySelector<HTMLElement>('[data-browser-pane="spotify-tracks"]')
          ?.focus({ preventScroll: true });
      return;
    }
    const element = [
      ...(browserRoot.current?.querySelectorAll("[data-browser-id]") ?? []),
    ].find((row) => row.getAttribute("data-browser-id") === focusedId);
    scrollBrowserSelection(element ?? null);
    const frame = requestAnimationFrame(() => {
      if (controllerLayer === "playlists" || controllerLayer === "tracks")
        (element as HTMLElement | undefined)?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [
    browserSelection,
    collectionSelection,
    collectionFocus,
    selectedPlaylist,
    controllerLayer,
  ]);
  const fail = useCallback(
    (e: unknown) => setError(String(e instanceof Error ? e.message : e)),
    [],
  );
  useEffect(() => {
    if (!isDesktop || !tracks.length) return;
    let disposed = false;
    control<CachedAnalysis[]>("spotify.analysis.get", {
      uris: tracks
        .slice(-100)
        .filter((t) => !t.is_local && t.available !== false)
        .map((t) => t.uri),
    })
      .then((items) => {
        if (!disposed)
          setCachedAnalysis((old) => ({
            ...old,
            ...Object.fromEntries(items.map((a) => [a.uri, a])),
          }));
      })
      .catch(fail);
    return () => {
      disposed = true;
    };
  }, [tracks, fail]);
  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stop: UnlistenFn | undefined;
    listen<CachedAnalysis>("spotify-analysis-updated", (event) => {
      setCachedAnalysis((old) => ({
        ...old,
        [event.payload.uri]: event.payload,
      }));
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(fail);
    const poll = () =>
      control<BatchStatus>("spotify.analysis.status")
        .then((status) => {
          if (!disposed) setBatch(status);
        })
        .catch(fail);
    void poll();
    const timer = setInterval(poll, 1200);
    return () => {
      disposed = true;
      stop?.();
      clearInterval(timer);
    };
  }, [fail]);
  async function analyzePage(items = analysisPage) {
    try {
      const uris = items
        .slice(0, 50)
        .filter((t) => !t.is_local && t.available !== false)
        .map((t) => t.uri);
      if (!uris.length) return;
      setBatch(await control<BatchStatus>("spotify.analysis.start", { uris }));
    } catch (e) {
      fail(e);
    }
  }
  const refreshStatus = useCallback(async () => {
    if (!isDesktop) return;
    try {
      const next = await control<SpotifyStatus>("spotify.status");
      setStatus(next);
      const playback = await control<SpotifyStatus>("spotify.native.status");
      setNativeStatus(playback);
      if (next.client_id) setClientId((old) => old || next.client_id!);
      if (next.last_error) setError(next.last_error);
    } catch (e) {
      fail(e);
    }
  }, [fail]);
  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);
  useEffect(() => {
    if (
      !status.authorizing &&
      !status.restoring &&
      !nativeStatus.authorizing &&
      !nativeStatus.restoring
    )
      return;
    const timer = setInterval(refreshStatus, 1500);
    return () => clearInterval(timer);
  }, [
    status.authorizing,
    status.restoring,
    nativeStatus.authorizing,
    nativeStatus.restoring,
    refreshStatus,
  ]);
  const refreshDevices = useCallback(async () => {
    try {
      const result = await control<SpotifyDevice[]>("spotify.devices");
      setDevices(result);
      setDeviceId((old) =>
        result.some((d) => d.id === old)
          ? old
          : result.find((d) => d.is_active && !d.is_restricted)?.id ||
            result.find((d) => !d.is_restricted)?.id ||
            "",
      );
    } catch (e) {
      fail(e);
    }
  }, [fail]);
  const loadPlaylists = useCallback(
    async (offset = 0, throwErrors = false) => {
      if (playlistPending.current) return;
      playlistPending.current = true;
      setLoading(true);
      try {
        const page = await control<SpotifyPage<SpotifyPlaylist>>(
          "spotify.playlists",
          { offset },
        );
        setPlaylists((old) => (offset ? [...old, ...page.items] : page.items));
        setPlaylistTotal(page.total);
        setPlaylistOffset(page.next_offset);
      } catch (e) {
        fail(e);
        if (throwErrors) throw e;
      } finally {
        playlistPending.current = false;
        setLoading(false);
      }
    },
    [fail],
  );
  useEffect(() => {
    if (status.connected && !settingsOnly) {
      loadPlaylists();
      refreshDevices();
    }
  }, [status.connected, settingsOnly, loadPlaylists, refreshDevices]);
  useEffect(() => {
    const sentinel = playlistSentinel.current;
    if (!sentinel || playlistOffset === null || loading || error) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting))
          void loadPlaylists(playlistOffset);
      },
      {
        root:
          collectionHost?.closest(".controller-playlists-pane") ??
          playlistsPane.current,
        rootMargin: "300px",
      },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [playlistOffset, loading, error, loadPlaylists, collectionHost]);
  async function openPlaylist(
    playlist: SpotifyPlaylist | "liked",
    offset = 0,
    enterTracks = true,
    throwErrors = false,
  ) {
    if (collectionFetchTimer.current !== null)
      clearTimeout(collectionFetchTimer.current);
    collectionFetchTimer.current = null;
    const key = `${playlist === "liked" ? "liked" : playlist.id}:${offset}`;
    if (offset && trackPending.current?.key === key) return;
    const request = ++generation.current;
    trackPending.current = { key, request };
    if (!offset) {
      if (playlist === "liked" || !playlist.is_album)
        setCollectionSelection(playlist === "liked" ? "liked" : playlist.id);
      if (enterTracks) setCollectionFocus(false);
      setBrowserSelection(null);
    }
    setSelectedPlaylist(playlist);
    setLoading(true);
    setError("");
    if (!offset) {
      setTracks([]);
      setTrackOffset(0);
      setTrackTotal(0);
    }
    try {
      const page = await control<SpotifyPage<SpotifyTrack>>(
        playlist === "liked"
          ? "spotify.liked"
          : playlist.is_album
            ? "spotify.album_tracks"
            : "spotify.playlist_tracks",
        playlist === "liked"
          ? { offset }
          : playlist.is_album
            ? { album_id: playlist.id, album_name: playlist.name, offset }
            : { playlist_id: playlist.id, offset },
      );
      if (generation.current !== request) return;
      setTracks((old) => (offset ? [...old, ...page.items] : page.items));
      if (!offset) setBrowserSelection(page.items[0]?.uri ?? null);
      setTrackTotal(page.total);
      setTrackOffset(page.next_offset);
      setAnalysisPage(page.items);
      setAnalysisPageNumber(Math.floor(page.offset / 50) + 1);
      if (!offset && autoAnalyze && nativeStatus.connected && !batch.active)
        void analyzePage(page.items);
      return page.items;
    } catch (e) {
      if (generation.current === request) fail(e);
      if (throwErrors) throw e;
    } finally {
      if (generation.current === request) {
        setLoading(false);
        trackPending.current = null;
      }
    }
  }
  nextTrackPage.current = () => {
    if (trackOffset !== null && selectedPlaylist && !loading && !error)
      void openPlaylist(selectedPlaylist, trackOffset);
  };
  useEffect(() => {
    if (!status.connected || settingsOnly || !collectionFocusRef.current)
      return;
    const currentId =
      selectedPlaylist === "liked" ? "liked" : selectedPlaylist?.id;
    if (currentId === collectionSelection) return;
    const timer = setTimeout(() => {
      collectionFetchTimer.current = null;
      if (
        !collectionFocusRef.current ||
        collectionSelectionRef.current !== collectionSelection
      )
        return;
      const chosen =
        collectionSelection === "liked"
          ? "liked"
          : playlists.find((p) => p.id === collectionSelection);
      if (chosen) void openPlaylist(chosen, 0, false);
    }, 220);
    collectionFetchTimer.current = timer;
    return () => {
      clearTimeout(timer);
      if (collectionFetchTimer.current === timer)
        collectionFetchTimer.current = null;
    };
  }, [collectionSelection, status.connected, settingsOnly, playlists]);
  function backToCollection() {
    setCollectionFocus(true);
    onBrowserPaneChange?.("playlists");
  }
  browserHandler.current = async (action) => {
    if (action.type === "browse") {
      if (action.shifted) return;
      if (collectionFocusRef.current || !selectedPlaylist) {
        const ids = ["liked", ...playlists.map((p) => p.id)];
        const index = browseIndex(
          ids,
          collectionSelectionRef.current,
          action.delta,
        );
        if (index >= 0) setCollectionSelection(ids[index]);
      } else if (!loading) {
        const ids = tracks.map((track) => track.uri);
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
      const chosen =
        collectionSelectionRef.current === "liked"
          ? "liked"
          : playlists.find((p) => p.id === collectionSelectionRef.current);
      if (!chosen) return;
      const currentId =
        selectedPlaylist === "liked" ? "liked" : selectedPlaylist?.id;
      if (currentId !== collectionSelectionRef.current || !tracks.length)
        await openPlaylist(chosen);
      else setCollectionFocus(false);
    } else if (action.type === "load") {
      const currentId =
        selectedPlaylist === "liked" ? "liked" : selectedPlaylist?.id;
      let fetched: SpotifyTrack[] | undefined;
      if (
        collectionFocusRef.current &&
        (currentId !== collectionSelectionRef.current || loading)
      ) {
        const chosen =
          collectionSelectionRef.current === "liked"
            ? "liked"
            : playlists.find((p) => p.id === collectionSelectionRef.current);
        if (chosen) fetched = await openPlaylist(chosen, 0, false);
        if (!fetched) return;
      }
      const track =
        fetched?.[0] ??
        tracks.find((t) => t.uri === selectedRow.current) ??
        tracks[0];
      if (!track || (loading && !fetched)) {
        fail("Choose a Spotify playlist or Liked songs, then select a track.");
        return;
      }
      if (action.shifted && action.deck === 0) {
        if (track.album_id)
          await openPlaylist({
            id: track.album_id,
            name: track.album,
            total: null,
            external_url: "",
            is_album: true,
            collaborative: false,
          });
        else fail("This track has no browsable album.");
      } else if (!action.shifted) {
        if (!nativeStatus.connected) {
          fail("Sign in to the control Spotify player before loading a deck.");
          return;
        }
        if (track.is_local || track.available === false) {
          fail("This Spotify track is unavailable for deck playback.");
          return;
        }
        if (deckJobs[action.deck]?.preparing) return;
        setBrowserSelection(track.uri);
        await onLoadDeck?.(action.deck, track);
      }
    }
  };
  async function configure() {
    setBusy(true);
    setError("");
    try {
      await control("spotify.configure", { client_id: clientId.trim() });
      await refreshStatus();
      setAuthorization(null);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function login(playback = false) {
    setBusy(true);
    setError("");
    try {
      const result = await control<{
        authorization_url: string;
        redirect_uri: string;
      }>("spotify.login", { playback });
      setAuthorization(result);
      await refreshStatus();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function loginNative() {
    setBusy(true);
    setError("");
    try {
      const result = await control<{
        authorization_url: string;
        redirect_uri: string;
      }>("spotify.native.login");
      setAuthorization(result);
      await refreshStatus();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }
  async function play(uri: string) {
    setError("");
    try {
      await control("spotify.play", { uri, device_id: deviceId || null });
      setPlayingUri(uri);
    } catch (e) {
      fail(e);
    }
  }
  async function pause() {
    setError("");
    try {
      await control("spotify.pause", { device_id: deviceId || null });
      setPlayingUri("");
    } catch (e) {
      fail(e);
    }
  }
  const setup = settingsOnly || showSetup || !status.connected;
  useControl(
    settingsOnly ? "spotify-settings" : "spotify-browser",
    {
      setup: (v) => setShowSetup(controlBoolean(v)),
      "client-id": (v) => setClientId(controlString(v)),
      configure: async () => {
        await control("spotify.configure", { client_id: clientId.trim() });
        await refreshStatus();
      },
      login: async () => {
        setAuthorization(await control("spotify.login", { playback: false }));
        await refreshStatus();
      },
      "control-login": async () => {
        setAuthorization(await control("spotify.native.login"));
        await refreshStatus();
      },
      disconnect: async () => {
        await control("spotify.disconnect");
        await refreshStatus();
      },
      "control-disconnect": async () => {
        await control("spotify.native.disconnect");
        await refreshStatus();
      },
      "error-dismiss": () => setError(""),
      "status-refresh": refreshStatus,
      "playlists-refresh": () => loadPlaylists(0, true),
      "playlists-more": () => {
        if (playlistOffset === null) throw new Error("No further playlists");
        return loadPlaylists(playlistOffset, true);
      },
      collection: (v) => {
        const id = controlString(v);
        const chosen =
          id === "liked"
            ? "liked"
            : playlists.find((p) => p.id === id.replace(/^playlist:/, ""));
        if (!chosen)
          throw new Error("Collection not found in loaded playlists");
        return openPlaylist(chosen, 0, true, true);
      },
      album: (v) => {
        const album = controlObject(v);
        return openPlaylist(
          {
            id: controlString(album.id),
            name: controlString(album.name ?? "Album"),
            total: null,
            external_url: "",
            collaborative: false,
            is_album: true,
          },
          0,
          true,
          true,
        );
      },
      "tracks-refresh": () => {
        if (!selectedPlaylist) throw new Error("Choose a collection");
        return openPlaylist(selectedPlaylist, 0, false, true);
      },
      "tracks-more": () => {
        if (!selectedPlaylist || trackOffset === null)
          throw new Error("No further tracks");
        return openPlaylist(selectedPlaylist, trackOffset, false, true);
      },
      selection: (v) => {
        const uri = controlString(v);
        if (!tracks.some((t) => t.uri === uri))
          throw new Error("Track is not in loaded browser");
        setBrowserSelection(uri);
        setCollectionFocus(false);
      },
      focus: (v) => {
        const target = controlString(v);
        if (target !== "playlists" && target !== "tracks")
          throw new Error("Focus must be playlists or tracks");
        setCollectionFocus(target === "playlists");
      },
      "auto-analyze": (v) => setAutoAnalyze(controlBoolean(v)),
      analyze: async () => {
        const uris = analysisPage
          .filter((t) => t.available && !t.is_local)
          .slice(0, 50)
          .map((t) => t.uri);
        setBatch(await control("spotify.analysis.start", { uris }));
      },
      "analysis-cancel": () => control("spotify.analysis.cancel"),
      device: (v) => {
        const id = controlString(v);
        if (id && !devices.some((d) => d.id === id))
          throw new Error("Connect device not found");
        setDeviceId(id);
      },
      "devices-refresh": refreshDevices,
      play: async (v) => {
        const uri = controlString(v);
        await control("spotify.play", { uri, device_id: deviceId || null });
        setPlayingUri(uri);
      },
      pause: async () => {
        await control("spotify.pause", { device_id: deviceId || null });
        setPlayingUri("");
      },
      "widget-link": (v) => setWidgetLink(controlString(v)),
      widget: (v) => {
        if (v === null || v === false) {
          setWidgetTrack(null);
          return;
        }
        const uri = controlString(v);
        const id = spotifyTrackId(uri);
        if (!id) throw new Error("Invalid Spotify track link");
        setWidgetTrack({ id, title: "Spotify track player" });
      },
      "widget-submit": () => {
        const id = spotifyTrackId(widgetLink);
        if (!id) throw new Error("Invalid Spotify track link");
        setWidgetTrack({ id, title: "Spotify track player" });
      },
      "controller-browser": (v) =>
        browserHandler.current(v as ControllerBrowserAction),
    },
    {
      setup: showSetup,
      client_id: clientId,
      error,
      collection: selectedPlaylist === "liked" ? "liked" : selectedPlaylist?.id,
      selection: browserSelection,
      focus: collectionFocus ? "playlists" : "tracks",
      auto_analyze: autoAnalyze,
      device: deviceId,
      playing_uri: playingUri,
      widget: widgetTrack,
      widget_link: widgetLink,
      playlists: playlists.map((p) => ({ id: p.id, name: p.name })),
      playlist_offset: playlistOffset,
      track_uris: tracks.map((t) => t.uri),
      track_offset: trackOffset,
      analysis_page: analysisPageNumber,
      batch,
    },
  );
  const collectionTree = (
    <aside
      className={`controller-playlists-pane ${collectionFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
      aria-label="Spotify playlists"
      ref={playlistsPane}
    >
      <div className="browser-pane-heading">
        COLLECTIONS{" "}
        <button
          className="subtle"
          onClick={() => loadPlaylists()}
          disabled={loading}
        >
          <RefreshCw size={12} />
        </button>
      </div>
      <button
        className={`spotify-playlist liked ${collectionSelection === "liked" ? "controller-selected" : ""}`}
        data-browser-id="liked"
        aria-pressed={collectionSelection === "liked"}
        onClick={() => {
          onActivate?.();
          setCollectionFocus(true);
          onBrowserPaneChange?.("playlists");
          setCollectionSelection("liked");
          void openPlaylist("liked", 0, false);
        }}
      >
        <Heart size={16} />
        <strong>Liked songs</strong>
      </button>
      {playlists.map((p) => (
        <button
          key={p.id}
          className={`spotify-playlist ${collectionSelection === p.id ? "controller-selected" : ""}`}
          title={
            p.total !== null
              ? `${p.name} · ${p.total} track${p.total === 1 ? "" : "s"}`
              : p.name
          }
          data-browser-id={p.id}
          aria-pressed={collectionSelection === p.id}
          onClick={() => {
            onActivate?.();
            setCollectionFocus(true);
            onBrowserPaneChange?.("playlists");
            setCollectionSelection(p.id);
            void openPlaylist(p, 0, false);
          }}
        >
          <TrackArtwork url={p.artwork_url} />
          <strong>{p.name}</strong>
          <span>{p.total ?? ""}</span>
        </button>
      ))}
      {playlistOffset !== null && (
        <div ref={playlistSentinel} role="status" className="infinite-status">
          {loading ? "Loading playlists…" : ""}
        </div>
      )}
    </aside>
  );
  return (
    <section
      ref={browserRoot}
      className={`spotify-panel ${settingsOnly ? "spotify-settings" : ""} ${compact ? "spotify-compact" : ""}`}
    >
      {!settingsOnly && (
        <div className="spotify-heading">
          <div className="spotify-title">
            <span className="spotify-symbol">
              <Radio size={22} />
            </span>
            <div>
              <h2>Spotify library</h2>
              <p>Experimental control deck loading · Spotify Connect</p>
            </div>
          </div>
          <div>
            <span
              className={`spotify-status ${status.connected ? "connected" : ""}`}
            >
              <i className="tiny-dot" />
              {status.connected
                ? status.account?.display_name || "Account connected"
                : status.authorizing
                  ? "Waiting for sign-in"
                  : "Not connected"}
            </span>
            <button
              className="icon-button"
              aria-label="Spotify account settings"
              onClick={() => setShowSetup(!showSetup)}
            >
              <Settings2 size={17} />
            </button>
          </div>
        </div>
      )}
      {!compact && (
        <p className="spotify-boundary">
          <Headphones size={14} />
          Load A / B prepares a track for the control decks after a separate
          control-player sign-in. Use the deck controls above once loading
          completes. Connect plays on the selected Spotify device.
        </p>
      )}
      {(!compact || !nativeStatus.connected || !!nativeStatus.last_error) && (
        <div className="spotify-auth-note">
          <Headphones size={14} />
          <span>
            Native player:{" "}
            {nativeStatus.restoring
              ? "restoring saved sign-in"
              : nativeStatus.authorizing
                ? "waiting for Spotify consent"
                : nativeStatus.connected
                  ? "signed in"
                  : "sign-in required"}
            .{" "}
            {!nativeStatus.connected && (
              <button
                disabled={
                  busy ||
                  nativeStatus.restoring ||
                  nativeStatus.authorizing ||
                  !isDesktop
                }
                onClick={loginNative}
              >
                Sign in to control player
              </button>
            )}
            {nativeStatus.last_error && <span>{nativeStatus.last_error}</span>}
          </span>
        </div>
      )}
      {error && (
        <div className="spotify-error" role="alert">
          <span>{error}</span>
          <button
            aria-label="Dismiss Spotify error"
            onClick={() => setError("")}
          >
            <X size={13} />
          </button>
        </div>
      )}
      {widgetTrack && (
        <div className="spotify-widget-card">
          <div>
            <strong>Spotify widget</strong>
            <button
              aria-label="Close Spotify widget"
              onClick={() => setWidgetTrack(null)}
            >
              <X size={15} />
            </button>
          </div>
          <iframe
            key={widgetTrack.id}
            src={spotifyWidgetUrl(widgetTrack.id)!}
            width="100%"
            height="152"
            allow="autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture"
            loading="lazy"
            title={widgetTrack.title}
            frameBorder="0"
          />
          <p>
            Spotify manages this player. In-app playback may be limited to
            previews; use Connect for full playback on your Spotify device.
          </p>
        </div>
      )}
      {setup ? (
        <div className="spotify-setup">
          <div className="settings-section-title">
            <Radio size={18} />
            <div>
              <h3>
                {status.connected
                  ? "Spotify account"
                  : "Connect your Spotify account"}
              </h3>
              <p>
                OAuth with PKCE. Your account tokens stay in the control app.
              </p>
            </div>
          </div>
          <ol className="spotify-steps">
            <li>
              Create your own application in the{" "}
              <a
                href="https://developer.spotify.com/dashboard"
                target="_blank"
                rel="noreferrer"
              >
                Spotify developer dashboard <ExternalLink size={10} />
              </a>
              .
            </li>
            <li>
              Register{" "}
              <code>
                {status.registered_redirect_uri ||
                  "http://127.0.0.1:8765/callback"}
              </code>{" "}
              as its redirect URI and enable Web API and Web Playback SDK.
            </li>
            <li>
              Paste the public Client ID, save it, then sign in to authorize
              this application.
            </li>
          </ol>
          {status.connected && (
            <p>
              Library account connected. Native playback uses a separate Spotify
              consent through librespot's built-in control client and requests
              streaming permission. Use the same Premium account for both
              sign-ins. Native playback remains experimental.
            </p>
          )}
          <label className="field">
            Public Spotify Client ID
            <input
              autoComplete="off"
              placeholder="Client ID from your Spotify developer app"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              spellCheck={false}
            />
          </label>
          <div className="spotify-setup-actions">
            <button
              className="secondary"
              disabled={busy || !clientId.trim() || !isDesktop}
              onClick={configure}
            >
              {busy ? (
                <LoaderCircle size={13} className="spin" />
              ) : (
                <Check size={13} />
              )}
              Save Client ID
            </button>
            {!status.connected && (
              <button
                className="spotify-primary"
                disabled={busy || !status.configured || !isDesktop}
                onClick={() => login()}
              >
                <ExternalLink size={13} />
                {status.authorizing
                  ? "Restart sign-in"
                  : "Sign in with Spotify"}
              </button>
            )}
            {status.connected && (
              <button
                className="spotify-primary"
                disabled={
                  busy ||
                  nativeStatus.restoring ||
                  nativeStatus.authorizing ||
                  !isDesktop
                }
                onClick={loginNative}
              >
                <ExternalLink size={13} />
                {nativeStatus.connected
                  ? "Reauthorize control player"
                  : "Sign in to control player"}
              </button>
            )}
            {status.connected && (
              <button
                className="secondary"
                onClick={async () => {
                  try {
                    await control("spotify.disconnect");
                    setPlayingUri("");
                    setTracks([]);
                    setPlaylists([]);
                    await refreshStatus();
                  } catch (e) {
                    fail(e);
                  }
                }}
              >
                Disconnect account
              </button>
            )}
            {status.connected && !settingsOnly && (
              <button className="secondary" onClick={() => setShowSetup(false)}>
                Back to Spotify library
              </button>
            )}
          </div>
          {nativeStatus.connected && (
            <button
              className="secondary"
              onClick={async () => {
                try {
                  await control("spotify.native.disconnect");
                  await refreshStatus();
                } catch (e) {
                  fail(e);
                }
              }}
            >
              Disconnect control player
            </button>
          )}
          {nativeStatus.authorizing && (
            <p>
              Complete the separate control-player Spotify consent in your
              browser, then return here.
            </p>
          )}
          {authorization && !status.connected && (
            <div className="spotify-auth-note">
              <LoaderCircle size={14} className="spin" />
              <span>
                Complete authorization in your browser. Return here when Spotify
                confirms access.{" "}
                <button onClick={refreshStatus}>Check connection</button>
              </span>
            </div>
          )}
          <p className="help-text">
            Remote playback requires a Spotify Premium account and an available
            Spotify Connect device. Open Spotify on that device, then refresh
            devices here. Development-mode account access follows your developer
            application's Spotify settings.
          </p>
          {!!status.setup_notice && (
            <p className="help-text">{status.setup_notice}</p>
          )}
          {!settingsOnly && (
            <form
              className="spotify-widget-form"
              onSubmit={(e) => {
                e.preventDefault();
                const id = spotifyTrackId(widgetLink);
                if (!id) {
                  setError("Paste a Spotify track URL or spotify:track URI.");
                  return;
                }
                setError("");
                setWidgetTrack({ id, title: "Spotify track player" });
              }}
            >
              <label className="field">
                Open a Spotify track widget
                <input
                  aria-label="Spotify track link"
                  placeholder="https://open.spotify.com/track/…"
                  value={widgetLink}
                  onChange={(e) => setWidgetLink(e.target.value)}
                />
              </label>
              <button className="secondary">
                <Play size={12} />
                Open widget
              </button>
            </form>
          )}
          {!isDesktop && (
            <p className="help-text">
              Open the control desktop app to connect your account.
            </p>
          )}
        </div>
      ) : (
        <>
          {!compact && (
            <div className="spotify-player-bar">
              <div>
                <Radio size={14} />
                <label>
                  PLAYBACK DEVICE
                  <select
                    aria-label="Spotify playback device"
                    value={deviceId}
                    onChange={(e) => setDeviceId(e.target.value)}
                  >
                    <option value="">Choose a Spotify device…</option>
                    {devices.map((d) => (
                      <option
                        value={d.id || ""}
                        key={d.id || `${d.name}-${d.device_type}`}
                        disabled={d.is_restricted || !d.id}
                      >
                        {d.name}
                        {d.is_active ? " · active" : ""}
                        {d.is_restricted ? " · restricted" : ""}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <button className="secondary" onClick={refreshDevices}>
                <RefreshCw size={12} />
                Refresh devices
              </button>
              <button
                className="secondary"
                onClick={pause}
                disabled={!deviceId}
              >
                <Pause size={12} />
                Pause Spotify
              </button>
            </div>
          )}
          {!compact && !devices.length && (
            <p className="help-text">
              No Spotify device is available. Open Spotify on your computer or
              phone and refresh devices.
            </p>
          )}

          <div className="controller-browser-split">
            {collectionHost
              ? createPortal(collectionTree, collectionHost)
              : collectionTree}
            <div
              className={`controller-tracks-pane ${!collectionFocus && controllerLayer !== "close" ? "pane-active" : ""}`}
              data-browser-pane="spotify-tracks"
              ref={tracksPane}
              tabIndex={-1}
              onPointerDown={() => {
                setCollectionFocus(false);
                onBrowserPaneChange?.("tracks");
              }}
            >
              {selectedPlaylist ? (
                <>
                  <div className="spotify-list-heading">
                    <button className="subtle" onClick={backToCollection}>
                      <ChevronLeft size={13} />
                      Playlists
                    </button>
                    <h3>
                      {selectedPlaylist === "liked"
                        ? "Liked songs"
                        : selectedPlaylist.name}
                    </h3>
                    <span>{trackTotal} tracks</span>
                  </div>
                  <div className="spotify-analysis-toolbar">
                    <button
                      className="secondary"
                      disabled={
                        batch.active ||
                        loading ||
                        !nativeStatus.connected ||
                        !tracks.length
                      }
                      onClick={() => analyzePage()}
                    >
                      <RefreshCw size={12} /> Analyze page {analysisPageNumber}{" "}
                      <small>{Math.min(50, analysisPage.length)} tracks</small>
                    </button>
                    <label>
                      <input
                        type="checkbox"
                        checked={autoAnalyze}
                        onChange={(e) => setAutoAnalyze(e.target.checked)}
                      />{" "}
                      Analyze albums / playlists on selection
                    </label>
                    {batch.active ? (
                      <span role="status">
                        <LoaderCircle size={12} className="spin" />{" "}
                        {batch.completed} / {batch.total} processed
                        {batch.cached > 0 ? ` · ${batch.cached} reused` : ""}
                        <button
                          onClick={() =>
                            control("spotify.analysis.cancel").catch(fail)
                          }
                        >
                          Cancel
                        </button>
                      </span>
                    ) : batch.total > 0 ? (
                      <span role="status">
                        {batch.cancelled ? "Cancelled · " : ""}
                        {batch.completed} / {batch.total} processed
                        {batch.cached > 0 ? ` · ${batch.cached} reused` : ""}
                        {batch.failed
                          ? ` · ${batch.failed} failed`
                          : " · analysis saved"}
                      </span>
                    ) : (
                      <span title="Loading a deck prepares playback audio and reuses current saved analysis.">
                        Analysis saved between app launches
                      </span>
                    )}
                  </div>
                  {batch.last_error && (
                    <p className="spotify-analysis-error" role="alert">
                      {batch.last_error}
                    </p>
                  )}
                  <div className="spotify-tracks">
                    <table role="table" className="track-table">
                      <thead>
                        <tr role="row">
                          <th role="columnheader">#</th>
                          <th role="columnheader">TRACK / ARTIST</th>
                          <th role="columnheader">ALBUM</th>
                          <th role="columnheader">BPM</th>
                          <th role="columnheader">KEY</th>
                          <th role="columnheader">TIME</th>
                          <th role="columnheader">PLAYBACK</th>
                        </tr>
                      </thead>
                      <VirtualTrackRows
                        count={tracks.length}
                        getKey={trackKey}
                        scrollRef={tracksPane}
                        selectedIndex={tracks.findIndex(
                          (t) => t.uri === browserSelection,
                        )}
                        focusSelection={
                          !collectionFocus && controllerLayer === "tracks"
                        }
                        queryKey={
                          selectedPlaylist === "liked"
                            ? "liked"
                            : selectedPlaylist.id
                        }
                        hasMore={trackOffset !== null}
                        loading={loading}
                        onMore={loadMoreTracks}
                      >
                        {(i, measure) => {
                          const track = tracks[i];
                          return (
                            <tr
                              ref={measure}
                              data-index={i}
                              aria-rowindex={i + 2}
                              key={`${track.id}-${i}`}
                              data-browser-id={track.uri}
                              tabIndex={-1}
                              aria-selected={browserSelection === track.uri}
                              onClick={() => {
                                setBrowserSelection(track.uri);
                                setCollectionFocus(false);
                              }}
                              draggable={
                                nativeStatus.connected &&
                                !!onLoadDeck &&
                                track.available !== false &&
                                !track.is_local
                              }
                              onDragStart={(e) => {
                                e.dataTransfer.effectAllowed = "copy";
                                e.dataTransfer.setData(
                                  "application/pulsedeck-spotify-track",
                                  JSON.stringify(track),
                                );
                              }}
                              className={[
                                browserSelection === track.uri
                                  ? "selected controller-selected"
                                  : "",
                                playingUri === track.uri
                                  ? "spotify-commanded"
                                  : "",
                              ]
                                .filter(Boolean)
                                .join(" ")}
                            >
                              <td className="track-number">{i + 1}</td>
                              <td>
                                <div className="track-cell">
                                  <TrackArtwork url={track.artwork_url} />
                                  <div>
                                    <strong>{track.title}</strong>
                                    <span>{track.artists.join(", ")}</span>
                                  </div>
                                </div>
                              </td>
                              <td className="spotify-album">
                                {track.album_id ? (
                                  <button
                                    className="album-link"
                                    onClick={() =>
                                      openPlaylist({
                                        id: track.album_id!,
                                        name: track.album,
                                        total: null,
                                        external_url: "",
                                        is_album: true,
                                        collaborative: false,
                                      })
                                    }
                                  >
                                    {track.album}
                                  </button>
                                ) : (
                                  track.album
                                )}
                              </td>
                              <td
                                className="numeric cached-tempo"
                                title={
                                  cachedAnalysis[track.uri]?.bpm_ambiguous
                                    ? "Estimated BPM; rhythm subdivisions are ambiguous"
                                    : cachedAnalysis[track.uri]
                                      ? "Cached analysis"
                                      : "Load or analyze this song to detect BPM"
                                }
                              >
                                {cachedAnalysis[track.uri]?.bpm?.toFixed(2) ||
                                  "—"}
                                {cachedAnalysis[track.uri]?.bpm_ambiguous && (
                                  <small>~</small>
                                )}
                              </td>
                              <td
                                className="numeric cached-key"
                                title={
                                  cachedAnalysis[track.uri]?.key ||
                                  "Load or analyze this song to detect key"
                                }
                              >
                                {cachedAnalysis[track.uri]?.camelot ||
                                  cachedAnalysis[track.uri]?.key ||
                                  "—"}
                              </td>
                              <td className="numeric">
                                {formatTime(track.duration_ms)}
                              </td>
                              <td className="spotify-track-actions">
                                <button
                                  className="spotify-play load-a"
                                  aria-label="Load on deck A"
                                  disabled={
                                    !nativeStatus.connected ||
                                    !onLoadDeck ||
                                    deckJobs[0]?.preparing ||
                                    track.available === false ||
                                    track.is_local
                                  }
                                  onClick={() =>
                                    onLoadDeck?.(0, track).catch(fail)
                                  }
                                  title="Prepare playback audio for deck A; reuse current saved analysis"
                                >
                                  A
                                </button>
                                <button
                                  className="spotify-play load-b"
                                  aria-label="Load on deck B"
                                  disabled={
                                    !nativeStatus.connected ||
                                    !onLoadDeck ||
                                    deckJobs[1]?.preparing ||
                                    track.available === false ||
                                    track.is_local
                                  }
                                  onClick={() =>
                                    onLoadDeck?.(1, track).catch(fail)
                                  }
                                  title="Prepare playback audio for deck B; reuse current saved analysis"
                                >
                                  B
                                </button>
                                <button
                                  className="spotify-play"
                                  disabled={
                                    !deviceId || track.available === false
                                  }
                                  onClick={() => play(track.uri)}
                                  title="Play on selected Spotify device"
                                >
                                  <Play size={12} />
                                  {playingUri === track.uri
                                    ? "Sent"
                                    : "Connect"}
                                </button>
                                <button
                                  className="spotify-widget-button"
                                  disabled={!spotifyWidgetUrl(track.id)}
                                  title="Open the official Spotify player"
                                  onClick={() =>
                                    setWidgetTrack({
                                      id: track.id,
                                      title: `Spotify — ${track.title}`,
                                    })
                                  }
                                >
                                  <Play size={11} />
                                  Widget
                                </button>
                              </td>
                            </tr>
                          );
                        }}
                      </VirtualTrackRows>
                    </table>
                    {!tracks.length && !loading && (
                      <p className="spotify-empty">
                        No tracks are available in this collection.
                      </p>
                    )}
                  </div>
                </>
              ) : (
                <p className="spotify-empty">
                  Choose a playlist to see its tracks.
                </p>
              )}
            </div>
          </div>
          {loading && (
            <div className="spotify-loading" role="status">
              <LoaderCircle size={16} className="spin" />
              Loading from Spotify…
            </div>
          )}
        </>
      )}
    </section>
  );
}

export default memo(SpotifyPanel);
