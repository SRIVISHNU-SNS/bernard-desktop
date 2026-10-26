import type { PointerEvent as ReactPointerEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, events, on } from "../lib/bridge";
import type { OverlayPayload, SnipSelection } from "../lib/types";
import { clamp, overlayLabel } from "../lib/format";
import "../styles/overlay.css";

interface Point {
  x: number;
  y: number;
}

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const MIN_SIDE = 8;

function normalize(start: Point, end: Point, width: number, height: number): Rect {
  const x = clamp(Math.min(start.x, end.x), 0, width);
  const y = clamp(Math.min(start.y, end.y), 0, height);
  const right = clamp(Math.max(start.x, end.x), 0, width);
  const bottom = clamp(Math.max(start.y, end.y), 0, height);
  return { x, y, width: right - x, height: bottom - y };
}

export function OverlayWindow() {
  const label = useRef(overlayLabel()).current;
  const [payload, setPayload] = useState<OverlayPayload | null>(null);
  const [start, setStart] = useState<Point | null>(null);
  const [cursor, setCursor] = useState<Point | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [committing, setCommitting] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const committingRef = useRef(false);

  const reset = useCallback(() => {
    setStart(null);
    setRect(null);
    setCursor(null);
    setCommitting(false);
    committingRef.current = false;
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];

    void (async () => {
      try {
        const current = await api.overlayPayload(label);
        if (!cancelled && current.label === label) setPayload(current);
      } catch {
        // No snip is active; the next snip event will provide the image.
      }
      unlisten.push(
        await on<OverlayPayload>(events.snipShow, (event) => {
          if (cancelled || event.payload.label !== label) return;
          setPayload(event.payload);
          reset();
        }),
      );
      unlisten.push(
        await on(events.snipCancelled, () => {
          if (cancelled) return;
          setPayload(null);
          reset();
        }),
      );
      unlisten.push(
        await on<{ message: string }>(events.snipFailed, () => {
          if (cancelled) return;
          setPayload(null);
          reset();
        }),
      );
    })();

    return () => {
      cancelled = true;
      for (const stop of unlisten) stop();
    };
  }, [label, reset]);

  const commit = useCallback(
    (selection: Rect) => {
      if (!payload || committingRef.current) return;
      if (selection.width < MIN_SIDE || selection.height < MIN_SIDE) {
        reset();
        return;
      }
      committingRef.current = true;
      setCommitting(true);
      const request: SnipSelection = {
        label,
        x: selection.x,
        y: selection.y,
        width: selection.width,
        height: selection.height,
      };
      void api.snipCommit(request).catch(() => {
        committingRef.current = false;
        setCommitting(false);
      });
    },
    [label, payload, reset],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!payload) return;
      if (event.key === "Escape") {
        event.preventDefault();
        void api.cancelSnip();
      }
      if (event.key === "Enter" && rect) {
        event.preventDefault();
        commit(rect);
      }
      if (event.key === "Backspace" && !committing) {
        event.preventDefault();
        reset();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [commit, committing, payload, rect, reset]);

  if (!payload) {
    return <div className="overlay-blank" aria-hidden />;
  }

  const viewportWidth = payload.viewportWidth;
  const viewportHeight = payload.viewportHeight;
  const selection = rect ?? (start && cursor ? normalize(start, cursor, viewportWidth, viewportHeight) : null);
  const liveLabel = selection && selection.width >= 1 && selection.height >= 1
    ? `${Math.round(selection.width)} × ${Math.round(selection.height)}`
    : null;

  const point = (event: ReactPointerEvent<HTMLDivElement>): Point => {
    const bounds = rootRef.current?.getBoundingClientRect();
    const left = bounds?.left ?? 0;
    const top = bounds?.top ?? 0;
    return {
      x: clamp(event.clientX - left, 0, viewportWidth),
      y: clamp(event.clientY - top, 0, viewportHeight),
    };
  };

  return (
    <div
      ref={rootRef}
      className={`overlay-root${committing ? " overlay-root--committing" : ""}`}
      onPointerDown={(event) => {
        if (event.button !== 0 || committing) return;
        event.preventDefault();
        const next = point(event);
        setStart(next);
        setCursor(next);
        setRect(null);
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        const next = point(event);
        setCursor(next);
        if (start) {
          setRect(normalize(start, next, viewportWidth, viewportHeight));
        }
      }}
      onPointerUp={(event) => {
        if (!start || committing) return;
        const end = point(event);
        const finalRect = normalize(start, end, viewportWidth, viewportHeight);
        setStart(null);
        setCursor(end);
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
        if (finalRect.width >= MIN_SIDE && finalRect.height >= MIN_SIDE) {
          setRect(finalRect);
          commit(finalRect);
        } else {
          setRect(null);
        }
      }}
      onPointerCancel={() => {
        setStart(null);
        setRect(null);
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        void api.cancelSnip();
      }}
    >
      <img
        className="overlay-shot"
        src={payload.imageDataUrl}
        alt=""
        draggable={false}
      />

      {selection ? (
        <>
          <div className="overlay-shade" aria-hidden />
          <div
            className="overlay-selection"
            style={{
              left: selection.x,
              top: selection.y,
              width: selection.width,
              height: selection.height,
            }}
          >
            <span className="overlay-corner overlay-corner--tl" />
            <span className="overlay-corner overlay-corner--tr" />
            <span className="overlay-corner overlay-corner--bl" />
            <span className="overlay-corner overlay-corner--br" />
            {liveLabel ? <span className="overlay-size">{liveLabel}</span> : null}
          </div>
        </>
      ) : (
        <div className="overlay-scrim" aria-hidden />
      )}

      {!start && !selection && cursor ? (
        <div className="overlay-crosshair" aria-hidden>
          <span className="overlay-crosshair-v" style={{ left: cursor.x }} />
          <span className="overlay-crosshair-h" style={{ top: cursor.y }} />
        </div>
      ) : null}

      <div className="overlay-hint">
        <div className="overlay-hint-main">
          {committing ? "Saving your snip…" : selection ? "Release to capture" : "Drag to snip"}
        </div>
        <div className="overlay-hint-meta">
          <span>{payload.name}</span>
          <span className="overlay-dot" aria-hidden />
          <span>Esc to cancel</span>
          <span className="overlay-dot" aria-hidden />
          <span>Right-click to leave</span>
        </div>
      </div>
    </div>
  );
}
