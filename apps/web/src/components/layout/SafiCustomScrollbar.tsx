/**
 * SafiData custom scrollbar.
 *
 * The native root document scrollbar is hidden in `index.css` while internal
 * tables, panels, and modal containers preserve their own independent scrollbars.
 * This component mounts only after the intro completes, resolves the active
 * primary scroll container cleanly, provides functional Up/Down step arrows with
 * start/end disabled states, and coordinates with Lenis, pointer drag, touch,
 * wheel, and keyboard input.
 */
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  ReactNode
} from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { translateLang } from "@safi/i18n";
import { scrollDocumentSmoothAware } from "../motion/SmoothScrollProvider";

export interface SafiCustomScrollbarProps {
  readonly active?: boolean;
  readonly lang?: "ar" | "en";
  readonly target?: HTMLElement | null;
}

export interface ScrollbarMetrics {
  readonly visible: boolean;
  readonly thumbHeight: number;
  readonly thumbTop: number;
  readonly progress: number;
  readonly trackTravel: number;
  readonly atStart: boolean;
  readonly atEnd: boolean;
}

export type ResolvedScrollTarget =
  | { readonly kind: "document"; readonly id: string; readonly scrollEl: HTMLElement }
  | { readonly kind: "element"; readonly id: string; readonly scrollEl: HTMLElement }
  | { readonly kind: "blocked" };

export const SCROLLBAR_MIN_THUMB_PX = 56;
export const SCROLLBAR_TRACK_INSET_PX = 8;
export const SCROLLBAR_ARROW_FOOTPRINT_PX = 76;
export const SCROLLBAR_EDGE_TOLERANCE_PX = 1;
export const SCROLLBAR_MIN_STEP_PX = 96;

/**
 * Pure geometry for the scrollbar so the behaviour is unit-testable without a DOM.
 */
export function computeScrollbarMetrics(
  scrollTop: number,
  scrollHeight: number,
  viewportHeight: number,
  trackHeight: number
): ScrollbarMetrics {
  const maxScroll = Math.max(scrollHeight - viewportHeight, 0);
  if (maxScroll <= SCROLLBAR_EDGE_TOLERANCE_PX || trackHeight <= 0) {
    return {
      visible: false,
      thumbHeight: 0,
      thumbTop: 0,
      progress: 0,
      trackTravel: 0,
      atStart: true,
      atEnd: true
    };
  }

  const ratio = Math.min(Math.max(viewportHeight / scrollHeight, 0), 1);
  const thumbHeight = Math.min(
    trackHeight,
    Math.max(Math.round(trackHeight * ratio), SCROLLBAR_MIN_THUMB_PX)
  );
  const travel = Math.max(trackHeight - thumbHeight, 0);
  const clampedScroll = Math.min(Math.max(scrollTop, 0), maxScroll);
  const progress = maxScroll === 0 ? 0 : clampedScroll / maxScroll;

  return {
    visible: true,
    thumbHeight,
    thumbTop: Math.round(travel * progress),
    progress,
    trackTravel: travel,
    atStart: clampedScroll <= SCROLLBAR_EDGE_TOLERANCE_PX,
    atEnd: clampedScroll >= Math.max(maxScroll - SCROLLBAR_EDGE_TOLERANCE_PX, 0)
  };
}

export function computeScrollStepPx(viewportHeight: number): number {
  if (!Number.isFinite(viewportHeight) || viewportHeight <= 0) {
    return SCROLLBAR_MIN_STEP_PX;
  }
  return Math.max(Math.round(viewportHeight * 0.16), SCROLLBAR_MIN_STEP_PX);
}

export function computeNextArrowScrollTop(
  currentScrollTop: number,
  maxScroll: number,
  viewportHeight: number,
  direction: "up" | "down"
): number {
  const safeMax = Math.max(maxScroll, 0);
  const clampedCurrent = Math.min(Math.max(currentScrollTop, 0), safeMax);
  const step = computeScrollStepPx(viewportHeight);
  if (direction === "up") {
    return Math.max(0, clampedCurrent - step);
  }
  return Math.min(safeMax, clampedCurrent + step);
}

function isVerticalScrollable(el: HTMLElement): boolean {
  if (el.scrollHeight <= el.clientHeight + SCROLLBAR_EDGE_TOLERANCE_PX) {
    return false;
  }
  if (typeof window === "undefined" || typeof window.getComputedStyle !== "function") {
    return true;
  }
  const overflowY = window.getComputedStyle(el).overflowY;
  return overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay";
}

function findScrollableInModal(dialog: HTMLElement): HTMLElement | null {
  if (isVerticalScrollable(dialog)) {
    return dialog;
  }
  const marked = dialog.querySelector<HTMLElement>('[data-safi-primary-scroller="true"]');
  if (marked !== null && isVerticalScrollable(marked)) {
    return marked;
  }
  const candidates = dialog.querySelectorAll<HTMLElement>("div, section, main");
  for (const el of candidates) {
    if (isVerticalScrollable(el)) {
      return el;
    }
  }
  return null;
}

export function resolveActiveScrollTarget(
  explicitTarget?: HTMLElement | null
): ResolvedScrollTarget {
  if (explicitTarget != null && explicitTarget.isConnected) {
    return {
      kind: "element",
      id: explicitTarget.id || "safi-main",
      scrollEl: explicitTarget
    };
  }

  if (typeof document !== "undefined") {
    const dialogs = Array.from(
      document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]')
    );
    const topDialog = dialogs[dialogs.length - 1];
    if (topDialog !== undefined) {
      if (
        topDialog.hasAttribute("data-safi-own-scrollbar") ||
        topDialog.classList.contains("safi-auth-modal") ||
        topDialog.classList.contains("safi-account-modal")
      ) {
        return { kind: "blocked" };
      }
      const modalScroller = findScrollableInModal(topDialog);
      if (modalScroller !== null) {
        return {
          kind: "element",
          id: modalScroller.id || "safi-modal",
          scrollEl: modalScroller
        };
      }
      return { kind: "blocked" };
    }

    const primaryEl = document.querySelector<HTMLElement>('[data-safi-primary-scroller="true"]');
    if (primaryEl !== null && isVerticalScrollable(primaryEl)) {
      return {
        kind: "element",
        id: primaryEl.id || "safi-main",
        scrollEl: primaryEl
      };
    }

    const docScroller =
      (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
    return {
      kind: "document",
      id: "safi-main",
      scrollEl: docScroller
    };
  }

  return { kind: "blocked" };
}

function readTargetScrollSnapshot(resolved: ResolvedScrollTarget): {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly viewportHeight: number;
  readonly maxScroll: number;
} | null {
  if (resolved.kind === "blocked") {
    return null;
  }
  if (resolved.kind === "document") {
    if (typeof window === "undefined" || typeof document === "undefined") return null;
    const viewportHeight = window.innerHeight;
    const scrollHeight = Math.max(
      document.documentElement.scrollHeight,
      document.body?.scrollHeight ?? 0
    );
    const scrollTop = window.scrollY || resolved.scrollEl.scrollTop || 0;
    return {
      scrollTop,
      scrollHeight,
      viewportHeight,
      maxScroll: Math.max(scrollHeight - viewportHeight, 0)
    };
  }

  const el = resolved.scrollEl;
  const viewportHeight = el.clientHeight;
  const scrollHeight = el.scrollHeight;
  return {
    scrollTop: el.scrollTop,
    scrollHeight,
    viewportHeight,
    maxScroll: Math.max(scrollHeight - viewportHeight, 0)
  };
}

function writeTargetScrollTop(
  resolved: ResolvedScrollTarget,
  nextTop: number,
  options: { readonly immediate?: boolean } = {}
): void {
  const snapshot = readTargetScrollSnapshot(resolved);
  if (snapshot === null) return;
  const clampedTop = Math.min(Math.max(Math.round(nextTop), 0), snapshot.maxScroll);

  if (resolved.kind === "document") {
    scrollDocumentSmoothAware(clampedTop, options);
    return;
  }
  if (resolved.kind === "element") {
    const el = resolved.scrollEl;
    if (typeof el.scrollTo === "function") {
      try {
        el.scrollTo({ top: clampedTop, behavior: options.immediate ? "instant" : "smooth" });
        return;
      } catch {
        // Fallback below for jsdom or constrained environments.
      }
    }
    el.scrollTop = clampedTop;
  }
}

export function SafiCustomScrollbar({
  active = true,
  lang = "en",
  target = null
}: SafiCustomScrollbarProps): ReactNode {
  const [metrics, setMetrics] = useState<ScrollbarMetrics>({
    visible: false,
    thumbHeight: 0,
    thumbTop: 0,
    progress: 0,
    trackTravel: 0,
    atStart: true,
    atEnd: true
  });
  const [targetId, setTargetId] = useState("safi-main");
  const [dragging, setDragging] = useState(false);
  const [engaged, setEngaged] = useState(false);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{ readonly startY: number; readonly startScroll: number } | null>(null);
  const holdDelayTimerRef = useRef<number | null>(null);
  const holdIntervalTimerRef = useRef<number | null>(null);
  const didRepeatHoldRef = useRef(false);

  const stopArrowHold = useCallback((): void => {
    if (typeof window === "undefined") return;
    if (holdDelayTimerRef.current !== null) {
      window.clearTimeout(holdDelayTimerRef.current);
      holdDelayTimerRef.current = null;
    }
    if (holdIntervalTimerRef.current !== null) {
      window.clearInterval(holdIntervalTimerRef.current);
      holdIntervalTimerRef.current = null;
    }
  }, []);

  const measure = useCallback((): void => {
    if (typeof window === "undefined" || typeof document === "undefined") return;
    const resolved = resolveActiveScrollTarget(target);
    if (resolved.kind === "blocked") {
      setMetrics((prev) =>
        prev.visible
          ? {
              visible: false,
              thumbHeight: 0,
              thumbTop: 0,
              progress: 0,
              trackTravel: 0,
              atStart: true,
              atEnd: true
            }
          : prev
      );
      return;
    }
    const snapshot = readTargetScrollSnapshot(resolved);
    if (snapshot === null) return;

    const measuredTrack = trackRef.current?.clientHeight ?? 0;
    const trackHeight =
      measuredTrack > 0
        ? measuredTrack
        : Math.max(
            snapshot.viewportHeight - SCROLLBAR_TRACK_INSET_PX * 2 - SCROLLBAR_ARROW_FOOTPRINT_PX,
            0
          );

    setTargetId(resolved.id);
    setMetrics(
      computeScrollbarMetrics(
        snapshot.scrollTop,
        snapshot.scrollHeight,
        snapshot.viewportHeight,
        trackHeight
      )
    );
  }, [target]);

  useEffect(() => {
    if (!active || typeof window === "undefined" || typeof document === "undefined") {
      stopArrowHold();
      setMetrics({
        visible: false,
        thumbHeight: 0,
        thumbTop: 0,
        progress: 0,
        trackTravel: 0,
        atStart: true,
        atEnd: true
      });
      return;
    }

    let frame = 0;
    const schedule = (): void => {
      if (frame !== 0) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        measure();
      });
    };

    schedule();
    window.addEventListener("scroll", schedule, { passive: true });
    document.addEventListener("scroll", schedule, { passive: true, capture: true });
    window.addEventListener("resize", schedule);

    let resizeObserver: ResizeObserver | null = null;
    if (typeof ResizeObserver === "function" && document.body !== null) {
      resizeObserver = new ResizeObserver(schedule);
      resizeObserver.observe(document.documentElement);
      resizeObserver.observe(document.body);
      if (target !== null) {
        resizeObserver.observe(target);
      }
    }

    let mutationObserver: MutationObserver | null = null;
    if (typeof MutationObserver === "function" && document.body !== null) {
      mutationObserver = new MutationObserver(schedule);
      mutationObserver.observe(document.body, {
        childList: true,
        subtree: true
      });
    }

    return () => {
      stopArrowHold();
      if (frame !== 0) window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      document.removeEventListener("scroll", schedule, true);
      window.removeEventListener("resize", schedule);
      resizeObserver?.disconnect();
      mutationObserver?.disconnect();
    };
  }, [active, measure, stopArrowHold, target]);

  useEffect(() => {
    if (metrics.atStart || metrics.atEnd) {
      stopArrowHold();
    }
  }, [metrics.atStart, metrics.atEnd, stopArrowHold]);

  const scrollToProgress = useCallback(
    (nextProgress: number, options: { readonly immediate?: boolean } = {}): void => {
      const resolved = resolveActiveScrollTarget(target);
      const snapshot = readTargetScrollSnapshot(resolved);
      if (snapshot === null) return;
      const clampedProgress = Math.min(Math.max(nextProgress, 0), 1);
      writeTargetScrollTop(resolved, snapshot.maxScroll * clampedProgress, options);
      measure();
    },
    [measure, target]
  );

  const stepScroll = useCallback(
    (direction: "up" | "down", immediate = false): void => {
      const resolved = resolveActiveScrollTarget(target);
      const snapshot = readTargetScrollSnapshot(resolved);
      if (snapshot === null || snapshot.maxScroll <= SCROLLBAR_EDGE_TOLERANCE_PX) return;
      const nextTop = computeNextArrowScrollTop(
        snapshot.scrollTop,
        snapshot.maxScroll,
        snapshot.viewportHeight,
        direction
      );
      writeTargetScrollTop(resolved, nextTop, { immediate });
      window.requestAnimationFrame(() => measure());
    },
    [measure, target]
  );

  const startArrowHold = (
    event: ReactPointerEvent<HTMLButtonElement>,
    direction: "up" | "down"
  ): void => {
    if (event.button !== 0 && event.pointerType === "mouse") return;
    stopArrowHold();
    didRepeatHoldRef.current = false;
    holdDelayTimerRef.current = window.setTimeout(() => {
      didRepeatHoldRef.current = true;
      stepScroll(direction, true);
      holdIntervalTimerRef.current = window.setInterval(() => {
        stepScroll(direction, true);
      }, 85);
    }, 360);
  };

  const handleArrowClick = (direction: "up" | "down"): void => {
    if (didRepeatHoldRef.current) {
      didRepeatHoldRef.current = false;
      return;
    }
    stepScroll(direction, false);
  };

  const onThumbPointerDown = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const resolved = resolveActiveScrollTarget(target);
    const snapshot = readTargetScrollSnapshot(resolved);
    if (snapshot === null) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { startY: event.clientY, startScroll: snapshot.scrollTop };
    setDragging(true);
  };

  const onThumbPointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const drag = dragRef.current;
    if (drag === null) return;
    const resolved = resolveActiveScrollTarget(target);
    const snapshot = readTargetScrollSnapshot(resolved);
    if (snapshot === null || metrics.trackTravel <= 0) return;
    const deltaProgress = (event.clientY - drag.startY) / metrics.trackTravel;
    const nextProgress =
      (drag.startScroll + deltaProgress * snapshot.maxScroll) / Math.max(snapshot.maxScroll, 1);
    scrollToProgress(nextProgress, { immediate: true });
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (dragRef.current === null) return;
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const onTrackPointerDown = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (event.target !== trackRef.current || trackRef.current === null) return;
    const rect = trackRef.current.getBoundingClientRect();
    const clickY = event.clientY - rect.top - metrics.thumbHeight / 2;
    const nextProgress = metrics.trackTravel > 0 ? clickY / metrics.trackTravel : 0;
    scrollToProgress(nextProgress, { immediate: false });
  };

  const onThumbKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>): void => {
    const step = 0.08;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        scrollToProgress(metrics.progress + step);
        break;
      case "ArrowUp":
        event.preventDefault();
        scrollToProgress(metrics.progress - step);
        break;
      case "PageDown":
        event.preventDefault();
        scrollToProgress(metrics.progress + step * 3);
        break;
      case "PageUp":
        event.preventDefault();
        scrollToProgress(metrics.progress - step * 3);
        break;
      case "Home":
        event.preventDefault();
        scrollToProgress(0);
        break;
      case "End":
        event.preventDefault();
        scrollToProgress(1);
        break;
      default:
        break;
    }
  };

  if (!active || !metrics.visible) return null;

  const percent = Math.round(metrics.progress * 100);

  return (
    <div
      className={`safi-scrollbar${dragging ? " is-dragging" : ""}${engaged ? " is-engaged" : ""}`}
      data-safi-custom-scrollbar="true"
      data-testid="safi-custom-scrollbar"
      data-scroll-target={targetId}
      onPointerEnter={() => setEngaged(true)}
      onPointerLeave={() => {
        setEngaged(false);
        stopArrowHold();
      }}
      aria-hidden="false"
    >
      <button
        type="button"
        className="safi-scrollbar-arrow safi-scrollbar-arrow--up"
        data-scrollbar-arrow="up"
        aria-controls={targetId}
        aria-label={translateLang(lang, "scrollbar.scrollUp")}
        disabled={metrics.atStart}
        onClick={() => handleArrowClick("up")}
        onPointerDown={(event) => startArrowHold(event, "up")}
        onPointerUp={stopArrowHold}
        onPointerCancel={stopArrowHold}
      >
        <ChevronUp size={14} strokeWidth={2.5} aria-hidden="true" />
      </button>

      <div ref={trackRef} className="safi-scrollbar-track" onPointerDown={onTrackPointerDown}>
        <div
          className="safi-scrollbar-thumb"
          role="scrollbar"
          aria-controls={targetId}
          aria-orientation="vertical"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          aria-valuetext={translateLang(lang, "scrollbar.percent", { percent })}
          aria-label={translateLang(lang, "scrollbar.label")}
          tabIndex={0}
          style={{
            height: `${metrics.thumbHeight}px`,
            transform: `translateY(${metrics.thumbTop}px)`
          }}
          onPointerDown={onThumbPointerDown}
          onPointerMove={onThumbPointerMove}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onKeyDown={onThumbKeyDown}
        />
      </div>

      <button
        type="button"
        className="safi-scrollbar-arrow safi-scrollbar-arrow--down"
        data-scrollbar-arrow="down"
        aria-controls={targetId}
        aria-label={translateLang(lang, "scrollbar.scrollDown")}
        disabled={metrics.atEnd}
        onClick={() => handleArrowClick("down")}
        onPointerDown={(event) => startArrowHold(event, "down")}
        onPointerUp={stopArrowHold}
        onPointerCancel={stopArrowHold}
      >
        <ChevronDown size={14} strokeWidth={2.5} aria-hidden="true" />
      </button>
    </div>
  );
}
