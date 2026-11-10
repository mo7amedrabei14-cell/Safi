/**
 * SafiData custom scrollbar.
 *
 * The browser's default scrollbar is removed globally in `index.css`; this
 * component is the only scroll indicator. It mounts only after the cinematic
 * intro hands over to the website (`active` becomes true), follows the document
 * height, supports pointer dragging, and is keyboard operable.
 */
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  ReactNode
} from "react";
import { useCallback, useEffect, useRef, useState } from "react";

export interface SafiCustomScrollbarProps {
  readonly active?: boolean;
  readonly lang?: "ar" | "en";
}

export interface ScrollbarMetrics {
  readonly visible: boolean;
  readonly thumbHeight: number;
  readonly thumbTop: number;
  readonly progress: number;
  readonly trackTravel: number;
}

export const SCROLLBAR_MIN_THUMB_PX = 56;
export const SCROLLBAR_TRACK_INSET_PX = 8;

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
  if (maxScroll <= 0 || trackHeight <= 0) {
    return { visible: false, thumbHeight: 0, thumbTop: 0, progress: 0, trackTravel: 0 };
  }

  const ratio = Math.min(Math.max(viewportHeight / scrollHeight, 0), 1);
  const thumbHeight = Math.max(Math.round(trackHeight * ratio), SCROLLBAR_MIN_THUMB_PX);
  const travel = Math.max(trackHeight - thumbHeight, 0);
  const clampedScroll = Math.min(Math.max(scrollTop, 0), maxScroll);
  const progress = maxScroll === 0 ? 0 : clampedScroll / maxScroll;

  return {
    visible: true,
    thumbHeight,
    thumbTop: Math.round(travel * progress),
    progress,
    trackTravel: travel
  };
}

export function SafiCustomScrollbar({
  active = true,
  lang = "en"
}: SafiCustomScrollbarProps): ReactNode {
  const [metrics, setMetrics] = useState<ScrollbarMetrics>({
    visible: false,
    thumbHeight: 0,
    thumbTop: 0,
    progress: 0,
    trackTravel: 0
  });
  const [dragging, setDragging] = useState(false);
  const [engaged, setEngaged] = useState(false);
  const dragRef = useRef<{ readonly startY: number; readonly startScroll: number } | null>(null);

  const measure = useCallback((): void => {
    if (typeof window === "undefined" || typeof document === "undefined") return;
    const viewportHeight = window.innerHeight;
    const scrollHeight = Math.max(
      document.documentElement.scrollHeight,
      document.body?.scrollHeight ?? 0
    );
    const trackHeight = Math.max(viewportHeight - SCROLLBAR_TRACK_INSET_PX * 2, 0);
    setMetrics(computeScrollbarMetrics(window.scrollY, scrollHeight, viewportHeight, trackHeight));
  }, []);

  useEffect(() => {
    if (!active || typeof window === "undefined") {
      setMetrics({ visible: false, thumbHeight: 0, thumbTop: 0, progress: 0, trackTravel: 0 });
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
    window.addEventListener("resize", schedule);

    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver === "function" && document.body !== null) {
      observer = new ResizeObserver(schedule);
      observer.observe(document.body);
    }

    return () => {
      if (frame !== 0) window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      observer?.disconnect();
    };
  }, [active, measure]);

  const scrollToProgress = useCallback((nextProgress: number): void => {
    if (typeof window === "undefined") return;
    const maxScroll = Math.max(document.documentElement.scrollHeight - window.innerHeight, 0);
    window.scrollTo({ top: Math.round(maxScroll * Math.min(Math.max(nextProgress, 0), 1)) });
  }, []);

  const onThumbPointerDown = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (typeof window === "undefined") return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { startY: event.clientY, startScroll: window.scrollY };
    setDragging(true);
  };

  const onThumbPointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const drag = dragRef.current;
    if (drag === null || typeof window === "undefined") return;
    if (metrics.trackTravel <= 0) return;
    const maxScroll = Math.max(document.documentElement.scrollHeight - window.innerHeight, 0);
    const deltaProgress = (event.clientY - drag.startY) / metrics.trackTravel;
    scrollToProgress((drag.startScroll + deltaProgress * maxScroll) / Math.max(maxScroll, 1));
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>): void => {
    if (dragRef.current === null) return;
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
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

  return (
    <div
      className={`safi-scrollbar${dragging ? " is-dragging" : ""}${engaged ? " is-engaged" : ""}`}
      data-safi-custom-scrollbar="true"
      data-testid="safi-custom-scrollbar"
      onPointerEnter={() => setEngaged(true)}
      onPointerLeave={() => setEngaged(false)}
      aria-hidden="false"
    >
      <div className="safi-scrollbar-track">
        <div
          className="safi-scrollbar-thumb"
          role="scrollbar"
          aria-controls="safi-main"
          aria-orientation="vertical"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(metrics.progress * 100)}
          aria-valuetext={
            lang === "ar"
              ? `${Math.round(metrics.progress * 100)} بالمئة`
              : `${Math.round(metrics.progress * 100)} percent`
          }
          aria-label={lang === "ar" ? "شريط التمرير المخصص" : "Custom scrollbar"}
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
    </div>
  );
}
