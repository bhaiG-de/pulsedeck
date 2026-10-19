import {
  cloneElement,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type CSSProperties,
  type RefObject,
} from "react";
import { libraryRenderTimings, libraryFrameTimings } from "./performance";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";

/** One continuous table, with measured row heights and a bounded mounted window.
 * A focused tag editor stays mounted while its row scrolls out of view. */
function VirtualTrackRows({
  count,
  getKey,
  scrollRef,
  selectedIndex,
  focusSelection,
  queryKey,
  hasMore = false,
  loading = false,
  onMore,
  children,
}: {
  count: number;
  getKey: (index: number) => string | number;
  scrollRef: RefObject<HTMLDivElement | null>;
  selectedIndex: number;
  focusSelection: boolean;
  queryKey: string;
  hasMore?: boolean;
  loading?: boolean;
  onMore?: () => void;
  children: (
    index: number,
    measure: (node: HTMLTableRowElement | null) => void,
  ) => ReactElement<{ style?: CSSProperties; role?: string }>;
}) {
  const renderStarted = performance.now();
  useLayoutEffect(() => {
    libraryRenderTimings.record(performance.now() - renderStarted);
  });
  useEffect(() => {
    const pane = scrollRef.current;
    if (!pane) return;
    let frame = 0,
      lastScroll = 0,
      previous: number | null = null;
    const tick = (timestamp: number) => {
      if (previous !== null && timestamp > previous)
        libraryFrameTimings.record(timestamp - previous);
      previous = timestamp;
      if (performance.now() - lastScroll < 180)
        frame = requestAnimationFrame(tick);
      else {
        frame = 0;
        previous = null;
      }
    };
    const onScroll = () => {
      lastScroll = performance.now();
      if (!frame) frame = requestAnimationFrame(tick);
    };
    pane.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      pane.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    };
  }, [scrollRef]);
  const body = useRef<HTMLTableSectionElement>(null);
  const [margin, setMargin] = useState(60);
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  const pendingFocus = useRef(false);
  const rangeExtractor = useCallback(
    (range: Parameters<typeof defaultRangeExtractor>[0]) => {
      const indexes = defaultRangeExtractor(range);
      if (
        focusedIndex !== null &&
        focusedIndex < count &&
        !indexes.includes(focusedIndex)
      )
        indexes.push(focusedIndex);
      return indexes.sort((a, b) => a - b);
    },
    [focusedIndex, count],
  );
  // Stable keys preserve the virtualizer's measurements across scroll updates.
  const itemKey = useCallback(
    (index: number) => (index < count ? getKey(index) : "loading-next-page"),
    [count, getKey],
  );
  const virtual = useVirtualizer({
    count: count + Number(hasMore || loading),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 44,
    getItemKey: itemKey,
    overscan: 8,
    scrollMargin: margin,
    scrollPaddingStart: 62,
    useAnimationFrameWithResizeObserver: true,
    directDomUpdates: true,
    useFlushSync: false,
    rangeExtractor,
  });
  const setBody = useCallback(
    (node: HTMLTableSectionElement | null) => {
      body.current = node;
      virtual.containerRef(node);
    },
    [virtual],
  );
  useLayoutEffect(() => {
    const pane = scrollRef.current;
    const tbody = body.current;
    if (!pane || !tbody) return;
    const measure = () =>
      setMargin(
        tbody.getBoundingClientRect().top -
          pane.getBoundingClientRect().top +
          pane.scrollTop,
      );
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(pane);
    if (tbody.previousElementSibling)
      observer.observe(tbody.previousElementSibling);
    return () => observer.disconnect();
  }, [scrollRef, queryKey]);
  useLayoutEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
    setFocusedIndex(null);
  }, [queryKey, scrollRef]);
  useLayoutEffect(() => {
    if (selectedIndex < 0 || selectedIndex >= count || !focusSelection) return;
    pendingFocus.current = true;
    virtual.scrollToIndex(selectedIndex, { align: "auto" });
  }, [selectedIndex, focusSelection, queryKey, virtual]);
  const items = virtual.getVirtualItems();
  useLayoutEffect(() => {
    if (!pendingFocus.current) return;
    const row = body.current?.querySelector<HTMLElement>(
      `tr[data-index="${selectedIndex}"]`,
    );
    if (row) {
      row.focus({ preventScroll: true });
      pendingFocus.current = false;
    }
  });
  const lastVisible = virtual.range?.endIndex ?? 0;
  useEffect(() => {
    if (hasMore && !loading && lastVisible >= count - 12) onMore?.();
  }, [hasMore, loading, count, lastVisible, onMore]);
  const rows = items.map((item) => {
    const style: CSSProperties = {
      position: "absolute",
      width: "100%",
      top: 0,
      left: 0,
    };
    if (item.index < count) {
      const row = children(item.index, virtual.measureElement);
      return cloneElement(row, {
        key: item.key,
        role: "row",
        style: { ...row.props.style, ...style },
      });
    }
    return (
      <tr
        key={item.key}
        role="row"
        style={style}
        data-index={item.index}
        ref={virtual.measureElement}
        className="virtual-loading"
      >
        <td colSpan={7} role="status" style={{ gridColumn: "1 / -1" }}>
          Loading tracks…
        </td>
      </tr>
    );
  });
  return (
    <tbody
      ref={setBody}
      className="virtual-track-body"
      onFocusCapture={(event) => {
        // Pin actual inputs/buttons, rather than the controller's row focus.
        if (!(event.target instanceof HTMLInputElement)) return;
        const row = event.target.closest<HTMLTableRowElement>("tr[data-index]");
        if (row) setFocusedIndex(Number(row.dataset.index));
      }}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null))
          setFocusedIndex(null);
      }}
    >
      {rows}
    </tbody>
  );
}
export default memo(VirtualTrackRows);
