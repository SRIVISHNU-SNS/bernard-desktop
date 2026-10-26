import { useEffect, useRef, useState } from "react";
import { api, events, on } from "../lib/bridge";
import type { AppSettings, AvatarState, ServerStatus } from "../lib/types";
import { Avatar } from "../components/Avatar";
import { applyTheme, watchTheme } from "../lib/theme";
import "../styles/avatar.css";

function stateFromServer(status: ServerStatus): AvatarState {
  switch (status.phase) {
    case "starting":
    case "warming":
      return "warming";
    case "error":
      return "error";
    case "ready":
      return "idle";
    default:
      return "idle";
  }
}

export function AvatarWindow() {
  const [state, setState] = useState<AvatarState>("idle");
  const pointer = useRef({ x: 0, y: 0, down: false, moved: false });
  const dragStarted = useRef(false);

  useEffect(() => {
    let theme = "system" as "system" | "light" | "dark";
    const unlisten: Array<() => void> = [];
    let cancelled = false;

    void (async () => {
      try {
        const settings = await api.getSettings();
        theme = settings.theme;
        applyTheme(settings.theme, document);
        watchTheme(settings.theme, document);
      } catch {
        applyTheme("system", document);
      }
      try {
        const status = await api.serverStatus();
        if (!cancelled) setState(stateFromServer(status));
      } catch {
        // The backend may still be booting; the first status event will arrive.
      }
      unlisten.push(
        await on<AvatarState>(events.avatarState, (event) => {
          if (!cancelled) setState(event.payload);
        }),
      );
      unlisten.push(
        await on<ServerStatus>(events.serverStatus, (event) => {
          if (cancelled) return;
          if (event.payload.phase === "ready") {
            setState((current) => (current === "error" ? "idle" : current));
          } else {
            setState(stateFromServer(event.payload));
          }
        }),
      );
      unlisten.push(
        await on<AppSettings>(events.settingsChanged, (event) => {
          theme = event.payload.theme;
          applyTheme(theme, document);
          watchTheme(theme, document);
        }),
      );
      unlisten.push(
        await on(events.panelOpened, () => {
          setState((current) => (current === "thinking" || current === "answering" ? current : "listening"));
        }),
      );
      unlisten.push(
        await on(events.panelClosed, () => {
          setState((current) => (current === "thinking" || current === "answering" ? current : "idle"));
        }),
      );
    })();

    return () => {
      cancelled = true;
      for (const stop of unlisten) stop();
    };
  }, []);

  const openPanel = () => {
    if (dragStarted.current) return;
    void api.togglePanel();
  };

  const openMenu = () => {
    if (dragStarted.current) return;
    void api.showAvatarMenu();
  };

  return (
    <div className="avatar-stage">
      <button
        type="button"
        className={`avatar-button avatar-button--${state}`}
        aria-label="Bernard. Click to open chat, right-click for menu, drag to move."
        title={
          state === "thinking"
            ? "Bernard is thinking"
            : state === "answering"
              ? "Bernard is answering"
              : state === "warming"
                ? "Bernard is warming up the model"
                : state === "error"
                  ? "Bernard needs attention"
                  : "Bernard"
        }
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          dragStarted.current = false;
          pointer.current = {
            x: event.clientX,
            y: event.clientY,
            down: true,
            moved: false,
          };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          if (!pointer.current.down || dragStarted.current) return;
          const distance = Math.hypot(
            event.clientX - pointer.current.x,
            event.clientY - pointer.current.y,
          );
          if (distance > 5) {
            pointer.current.moved = true;
            dragStarted.current = true;
            void api.startDragging().catch(() => {
              dragStarted.current = false;
            });
          }
        }}
        onPointerUp={(event) => {
          const wasDown = pointer.current.down;
          const moved = pointer.current.moved;
          pointer.current.down = false;
          if (event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.releasePointerCapture(event.pointerId);
          }
          if (wasDown && !moved && !dragStarted.current) {
            openPanel();
          }
          window.setTimeout(() => {
            dragStarted.current = false;
          }, 80);
        }}
        onPointerCancel={() => {
          pointer.current.down = false;
          dragStarted.current = false;
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          openMenu();
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            openPanel();
          }
          if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
            event.preventDefault();
            openMenu();
          }
        }}
      >
        <span className="avatar-halo" aria-hidden />
        <Avatar state={state} size={64} className="avatar-face" />
      </button>
    </div>
  );
}
