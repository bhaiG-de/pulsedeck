import { useCallback, useEffect, useRef, useState } from "react";
import { control, isDesktop } from "./api";
import type { LibraryFilters } from "./LibraryTools";
import type { TrackRecord } from "./types";

import { libraryPageTimings } from "./performance";

const PAGE_SIZE = 200;
type Page = {
  tracks: TrackRecord[];
  total: number;
  duration_seconds: number;
  next_offset: number | null;
};
export function useLocalLibrary(
  search: string,
  sort: string,
  playlist: string | number | null,
  onError: (error: unknown) => void,
  filters: LibraryFilters = {},
) {
  const [debouncedSearch, setDebouncedSearch] = useState(search);
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedSearch(search), 180);
    return () => clearTimeout(timer);
  }, [search]);
  const queryKey = JSON.stringify([debouncedSearch, sort, playlist, filters]);
  const query = useRef({
    key: queryKey,
    search: debouncedSearch,
    sort,
    playlist,
    filters,
  });
  query.current = {
    key: queryKey,
    search: debouncedSearch,
    sort,
    playlist,
    filters,
  };
  const [tracks, updateTracks] = useState<TrackRecord[]>([]);
  const [total, setTotal] = useState(0);
  const [duration, setDuration] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const items = useRef<TrackRecord[]>([]);
  const next = useRef<number | null>(0);
  const generation = useRef(0);
  const blocked = useRef(false);
  const pending = useRef<{
    key: string;
    epoch: number;
    promise: Promise<TrackRecord[]>;
  } | null>(null);
  const fetchPage = useCallback(
    (reset = false): Promise<TrackRecord[]> => {
      if (!isDesktop) return Promise.resolve([]);
      const q = query.current;
      if (
        pending.current?.key === q.key &&
        pending.current.epoch === generation.current
      )
        return pending.current.promise;
      if (!reset && (next.current === null || blocked.current))
        return Promise.resolve(items.current);
      const epoch = reset ? ++generation.current : generation.current;
      const offset = reset ? 0 : next.current!;
      if (reset) blocked.current = false;
      setLoading(true);
      let promise!: Promise<TrackRecord[]>;
      promise = (async () => {
        try {
          const started = performance.now();
          const page = await control<Page>("library.tracks", {
            search: q.search,
            filters: q.filters,
            sort: q.sort,
            ...(q.playlist === null ? {} : { playlist: q.playlist }),
            offset,
            limit: PAGE_SIZE,
          });
          libraryPageTimings.record(performance.now() - started);
          if (epoch !== generation.current || q.key !== query.current.key)
            return items.current;
          // Stable IDs also protect against records inserted while a page is in flight.
          const knownIds = new Set(items.current.map((track) => track.id));
          const merged = reset
            ? page.tracks
            : [
                ...items.current,
                ...page.tracks.filter((track) => !knownIds.has(track.id)),
              ];
          items.current = merged;
          next.current =
            page.next_offset !== null && page.next_offset > offset
              ? page.next_offset
              : null;
          updateTracks(merged);
          setTotal(page.total);
          setDuration(page.duration_seconds);
          setHasMore(next.current !== null);
        } catch (error) {
          if (epoch === generation.current && q.key === query.current.key) {
            blocked.current = true;
            onError(error);
          }
        } finally {
          if (pending.current?.promise === promise) pending.current = null;
          if (epoch === generation.current) setLoading(false);
        }
        return items.current;
      })();
      pending.current = { key: q.key, epoch, promise };
      return promise;
    },
    [onError],
  );
  useEffect(() => {
    items.current = [];
    next.current = 0;
    updateTracks([]);
    setTotal(0);
    setHasMore(false);
    void fetchPage(true);
    return () => {
      generation.current++;
    };
  }, [queryKey, fetchPage]);
  const reload = useCallback(
    (metadataOnly = false) => {
      if (metadataOnly && !query.current.search && !query.current.filters.my_tags?.length && !query.current.filters.colors?.length && query.current.filters.rating_min === undefined && query.current.filters.rating_max === undefined) return;
      void fetchPage(true);
    },
    [fetchPage],
  );
  const loadMore = useCallback(() => fetchPage(false), [fetchPage]);
  const setTracks = useCallback(
    (update: (old: TrackRecord[]) => TrackRecord[]) => {
      updateTracks((old) => {
        const updated = update(old);
        items.current = updated;
        return updated;
      });
    },
    [],
  );
  return {
    tracks,
    setTracks,
    total,
    duration,
    hasMore,
    loading,
    queryKey,
    reload,
    loadMore,
  };
}
