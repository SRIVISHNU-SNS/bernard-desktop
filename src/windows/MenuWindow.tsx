import { useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { api, events, on } from "../lib/bridge";
import type { AppSettings, ServerStatus } from "../lib/types";
import { applyTheme, watchTheme } from "../lib/theme";
import {
  CameraIcon,
  ChevronRightIcon,
  CloseIcon,
  GripIcon,
  LogoMark,
  PencilIcon,
  SettingsIcon,
  SparkIcon,
} from "../lib/icons";
import "../styles/menu.css";

interface StatusCopy {
  label: string;
  tone: "ready" | "busy" | "error";
}

function statusCopy(status: ServerStatus | null): StatusCopy {
  if (!status) return { label: "Starting", tone: "busy" };
  switch (status.phase) {
    case "ready":
      return { label: "Local · ready", tone: "ready" };
    case "warming":
      return { label: "Warming up", tone: "busy" };
    case "starting":
      return { label: "Starting", tone: "busy" };
    case "modelmissing":
      return { label: "Model missing", tone: "error" };
    case "error":
      return { label: "Needs attention", tone: "error" };
    default:
      return { label: "Offline", tone: "error" };
  }
}

export function MenuWindow() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [hotkey, setHotkey] = useState("Alt+B");
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlisten: Array<() => void> = [];
    let cancelled = false;
    void (async () => {
      try {
        const settings = await api.getSettings();
        setHotkey(settings.hotkey);
        applyTheme(settings.theme);
        watchTheme(settings.theme);
      } catch {
        applyTheme("system");
      }
      try {
        setStatus(await api.serverStatus());
      } catch {
        // The first status event will fill this in.
      }
      unlisten.push(
        await on<ServerStatus>(events.serverStatus, (event) => {
          if (!cancelled) setStatus(event.payload);
        }),
      );
      unlisten.push(
        await on<AppSettings>(events.settingsChanged, (event) => {
          if (cancelled) return;
          setHotkey(event.payload.hotkey);
          applyTheme(event.payload.theme);
          watchTheme(event.payload.theme);
        }),
      );
      unlisten.push(
        await on<string>(events.hotkeyRegistered, (event) => {
          if (!cancelled) setHotkey(event.payload);
        }),
      );
      window.setTimeout(() => {
        rootRef.current?.querySelector<HTMLButtonElement>("button.menu-item")?.focus();
      }, 30);
    })();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void api.hideAvatarMenu();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      cancelled = true;
      window.removeEventListener("keydown", onKey);
      for (const stop of unlisten) stop();
    };
  }, []);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      const items = Array.from(root.querySelectorAll<HTMLButtonElement>("button.menu-item"));
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      if (index < 0) return;
      event.preventDefault();
      const next = event.key === "ArrowDown" ? index + 1 : index - 1;
      items[(next + items.length) % items.length]?.focus();
    };
    root.addEventListener("keydown", onKeyDown);
    return () => root.removeEventListener("keydown", onKeyDown);
  }, []);

  const close = () => {
    void api.hideAvatarMenu();
  };

  const run = (action: () => Promise<unknown> | void) => {
    close();
    void Promise.resolve(action()).catch(() => {});
  };

  const copy = statusCopy(status);

  return (
    <div className="menu-shell" ref={rootRef}>
      <header className="menu-head">
        <LogoMark size={30} />
        <div className="menu-head-text">
          <span className="menu-name">Bernard</span>
          <span className={`menu-status menu-status--${copy.tone}`}>
            <i aria-hidden />
            {copy.label}
          </span>
        </div>
        <button
          type="button"
          className="menu-dismiss"
          onClick={close}
          aria-label="Close menu"
        >
          <CloseIcon size={14} />
        </button>
      </header>

      <div className="menu-list" role="menu" aria-label="Bernard actions">
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          onClick={() => run(() => api.togglePanel())}
        >
          <SparkIcon size={16} />
          <span>Open chat</span>
          <kbd>Click</kbd>
        </button>
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          onClick={() => run(() => api.snipNow())}
        >
          <CameraIcon size={16} />
          <span>Snip screen</span>
          <kbd>{hotkey}</kbd>
        </button>
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          onClick={() => {
            close();
            void emit("menu://new-chat");
            void api.showPanel();
          }}
        >
          <PencilIcon size={16} />
          <span>New chat</span>
          <ChevronRightIcon size={13} className="menu-chevron" />
        </button>
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          onClick={() => {
            close();
            void emit("menu://open-settings");
            void api.showPanel();
          }}
        >
          <SettingsIcon size={16} />
          <span>Settings</span>
          <ChevronRightIcon size={13} className="menu-chevron" />
        </button>
      </div>

      <div className="menu-divider" />

      <div className="menu-list menu-list--footer" role="menu" aria-label="Window actions">
        <button
          type="button"
          role="menuitem"
          className="menu-item"
          onClick={() => run(() => api.hideAvatar())}
        >
          <GripIcon size={16} />
          <span>Hide avatar</span>
        </button>
        <button
          type="button"
          role="menuitem"
          className="menu-item menu-item--danger"
          onClick={() => run(() => api.quit())}
        >
          <CloseIcon size={16} />
          <span>Quit Bernard</span>
        </button>
      </div>
    </div>
  );
}
