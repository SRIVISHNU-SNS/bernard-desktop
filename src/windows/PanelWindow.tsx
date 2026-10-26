import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent as ReactDragEvent,
} from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, events, on } from "../lib/bridge";
import type {
  AppSettings,
  Attachment,
  ChatDelta,
  ChatDone,
  ChatFailed,
  Conversation,
  DownloadProgress,
  ModelFileInfo,
  NoticeTone,
  ServerStatus,
  SnipReady,
  StoredMessage,
} from "../lib/types";
import { applyTheme, watchTheme } from "../lib/theme";
import { fileToDataUrl, isImageFile } from "../lib/files";
import { formatDateGroup, messageOf } from "../lib/format";
import {
  AlertIcon,
  CameraIcon,
  CheckIcon,
  CloseIcon,
  HistoryIcon,
  LogoMark,
  PlusIcon,
  SettingsIcon,
  SparkIcon,
} from "../lib/icons";
import { ChatError, Message, StreamingMessage } from "../components/Message";
import { Composer } from "../components/Composer";
import { ConversationDrawer } from "../components/ConversationDrawer";
import { SettingsView } from "../components/SettingsView";
import "../styles/panel.css";
import "../styles/chat.css";
import "../styles/drawer.css";
import "../styles/settings.css";

const MAX_IMAGES = 4;
const SNIP_CONTEXT_WINDOW_MS = 2 * 60 * 1000;

type View = "chat" | "settings";
type Tone = "ready" | "busy" | "error";

interface StreamState {
  requestId: string;
  conversationId: string;
}

interface ChatProblem {
  message: string;
  kind: "send" | "stream";
}

interface ToastState {
  id: number;
  message: string;
  tone: NoticeTone;
}

interface BootMeta {
  appVersion: string;
  dataDir: string;
}

function sameDay(a: number, b: number): boolean {
  return new Date(a).toDateString() === new Date(b).toDateString();
}

function headerStatus(server: ServerStatus | null): { label: string; tone: Tone } {
  if (!server) return { label: "Starting", tone: "busy" };
  switch (server.phase) {
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
      return { label: "Stopped", tone: "busy" };
  }
}

function bannerFor(
  server: ServerStatus | null,
): { tone: "busy" | "warn" | "error"; title: string; detail: string } | null {
  if (!server) return null;
  switch (server.phase) {
    case "ready":
      return null;
    case "starting":
      return {
        tone: "busy",
        title: "Starting the local model",
        detail: "llama.cpp is loading Gemma on this machine. The first answer can take a moment.",
      };
    case "warming":
      return {
        tone: "busy",
        title: "Warming up Gemma",
        detail: server.message || "The model is getting ready for your first question.",
      };
    case "modelmissing":
      return {
        tone: "warn",
        title: "The model isn't installed yet",
        detail: "Bernard needs Gemma 4 on this PC before it can answer. Add it in Settings.",
      };
    case "error":
      return {
        tone: "error",
        title: "The local model needs attention",
        detail: server.lastError || server.message || "Restart the model from Settings.",
      };
    default:
      return {
        tone: "warn",
        title: "The local model is stopped",
        detail: "Open Settings to start it again.",
      };
  }
}

export function PanelWindow() {
  const [booted, setBooted] = useState(false);
  const [bootError, setBootError] = useState<string | null>(null);
  const [bootAttempt, setBootAttempt] = useState(0);
  const [meta, setMeta] = useState<BootMeta>({ appVersion: "", dataDir: "" });

  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [model, setModel] = useState<ModelFileInfo | null>(null);
  const [conversations, setConversations] = useState<Conversation[]>([]);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<StoredMessage[]>([]);
  const [attachments, setAttachments] = useState<Record<string, Attachment>>({});
  const [pending, setPending] = useState<Attachment[]>([]);
  const [draft, setDraft] = useState("");
  const [loadingMessages, setLoadingMessages] = useState(false);

  const [sending, setSending] = useState(false);
  const [streaming, setStreaming] = useState<StreamState | null>(null);
  const [streamText, setStreamText] = useState("");
  const [stopping, setStopping] = useState(false);
  const [chatProblem, setChatProblem] = useState<ChatProblem | null>(null);

  const [view, setView] = useState<View>("chat");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [lightbox, setLightbox] = useState<Attachment | null>(null);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [downloadingPreset, setDownloadingPreset] = useState<string | null>(null);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  const toastTimer = useRef<number | null>(null);
  const dragDepth = useRef(0);

  const activeIdRef = useRef<string | null>(null);
  const messagesRef = useRef<StoredMessage[]>([]);
  const pendingRef = useRef<Attachment[]>([]);
  const sendingRef = useRef(false);
  const streamingRef = useRef<StreamState | null>(null);
  const viewRef = useRef<View>("chat");
  const drawerRef = useRef(false);
  const lightboxRef = useRef<Attachment | null>(null);
  const loadTokenRef = useRef(0);
  const lastActivityRef = useRef(0);

  const notice = useCallback((message: string, tone: NoticeTone = "info") => {
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    const next = { id: Date.now() + Math.random(), message, tone };
    setToast(next);
    toastTimer.current = window.setTimeout(() => {
      setToast((current) => (current?.id === next.id ? null : current));
    }, 4400);
  }, []);

  useEffect(() => {
    return () => {
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
    };
  }, []);

  const setActiveConversation = useCallback((id: string | null) => {
    activeIdRef.current = id;
    setActiveId(id);
  }, []);

  const setViewMode = useCallback((next: View) => {
    viewRef.current = next;
    setView(next);
  }, []);

  const setDrawer = useCallback((next: boolean) => {
    drawerRef.current = next;
    setDrawerOpen(next);
  }, []);

  const setPreview = useCallback((next: Attachment | null) => {
    lightboxRef.current = next;
    setLightbox(next);
  }, []);

  const setPendingList = useCallback((next: Attachment[]) => {
    pendingRef.current = next;
    setPending(next);
  }, []);

  const focusComposer = useCallback(() => {
    window.setTimeout(() => {
      if (viewRef.current !== "chat") return;
      const node = textareaRef.current;
      if (!node) return;
      node.focus();
      try {
        node.setSelectionRange(node.value.length, node.value.length);
      } catch {
        // Some webviews refuse selection on a focused control; focus alone is enough.
      }
    }, 80);
  }, []);

  const refreshConversations = useCallback(async () => {
    try {
      setConversations(await api.listConversations(""));
    } catch {
      // The chat itself still works; the drawer will catch up on its next open.
    }
  }, []);

  const applyMessages = useCallback((list: StoredMessage[]) => {
    messagesRef.current = list;
    setMessages(list);
    const paths = Array.from(new Set(list.flatMap((message) => message.imagePaths)));
    if (paths.length === 0) return;
    void api
      .loadAttachments(paths)
      .then((loaded) => {
        if (loaded.length === 0) return;
        setAttachments((current) => {
          const next = { ...current };
          for (const attachment of loaded) next[attachment.path] = attachment;
          return next;
        });
      })
      .catch(() => {
        // Missing snips render as a small placeholder in the message.
      });
  }, []);

  const refreshMessages = useCallback(
    async (conversationId: string) => {
      if (activeIdRef.current !== conversationId) return;
      try {
        const list = await api.getMessages(conversationId);
        if (activeIdRef.current === conversationId) applyMessages(list);
      } catch {
        // Leave the current transcript in place if the reload fails.
      }
    },
    [applyMessages],
  );

  const loadConversation = useCallback(
    async (id: string) => {
      const token = ++loadTokenRef.current;
      setLoadingMessages(true);
      setActiveConversation(id);
      messagesRef.current = [];
      setMessages([]);
      setChatProblem(null);
      setStopping(false);
      try {
        const list = await api.getMessages(id);
        if (token !== loadTokenRef.current) return;
        applyMessages(list);
        lastActivityRef.current = Date.now();
      } catch (error) {
        if (token === loadTokenRef.current) {
          setChatProblem({ message: messageOf(error), kind: "stream" });
        }
      } finally {
        if (token === loadTokenRef.current) setLoadingMessages(false);
      }
    },
    [applyMessages, setActiveConversation],
  );

  const abandonStream = useCallback(() => {
    const current = streamingRef.current;
    streamingRef.current = null;
    sendingRef.current = false;
    setStreaming(null);
    setStreamText("");
    setStopping(false);
    setSending(false);
    if (current) {
      void api.stopStream(current.requestId).catch(() => {});
    }
  }, []);

  const startNewChat = useCallback(() => {
    if (sendingRef.current) {
      notice("Give Bernard a moment to start first.");
      return;
    }
    abandonStream();
    loadTokenRef.current += 1;
    setActiveConversation(null);
    messagesRef.current = [];
    setMessages([]);
    setChatProblem(null);
    setDraft("");
    setPendingList([]);
    setLoadingMessages(false);
    setStopping(false);
    setViewMode("chat");
    setDrawer(false);
    setPreview(null);
    lastActivityRef.current = 0;
    stickToBottom.current = true;
    focusComposer();
  }, [
    abandonStream,
    focusComposer,
    notice,
    setActiveConversation,
    setDrawer,
    setPendingList,
    setPreview,
    setViewMode,
  ]);

  const pushAttachment = useCallback(
    (attachment: Attachment) => {
      if (pendingRef.current.some((item) => item.path === attachment.path)) return;
      if (pendingRef.current.length >= MAX_IMAGES) {
        notice("Bernard can hold four images at once.");
        return;
      }
      const next = [...pendingRef.current, attachment];
      pendingRef.current = next;
      setPending(next);
      setAttachments((current) => ({ ...current, [attachment.path]: attachment }));
    },
    [notice],
  );

  const addFiles = useCallback(
    async (files: File[]) => {
      for (const file of files) {
        if (pendingRef.current.length >= MAX_IMAGES) {
          notice("Bernard can hold four images at once.");
          break;
        }
        if (!isImageFile(file)) {
          notice("Only images can be attached.");
          continue;
        }
        try {
          const dataUrl = await fileToDataUrl(file);
          const attachment = await api.savePastedImage(dataUrl);
          pushAttachment(attachment);
        } catch (error) {
          notice(messageOf(error), "error");
        }
      }
    },
    [notice, pushAttachment],
  );

  const addPaths = useCallback(
    async (paths: string[]) => {
      for (const path of paths) {
        if (pendingRef.current.length >= MAX_IMAGES) {
          notice("Bernard can hold four images at once.");
          break;
        }
        try {
          const attachment = await api.saveDroppedImage(path);
          pushAttachment(attachment);
        } catch (error) {
          notice(messageOf(error), "error");
        }
      }
    },
    [notice, pushAttachment],
  );

  const removeAttachment = useCallback(
    (path: string) => {
      setPendingList(pendingRef.current.filter((item) => item.path !== path));
    },
    [setPendingList],
  );

  const importClipboard = useCallback(async () => {
    try {
      pushAttachment(await api.importClipboard());
    } catch (error) {
      notice(messageOf(error), "error");
    }
  }, [notice, pushAttachment]);

  const startSnip = useCallback(async () => {
    try {
      await api.snipNow();
    } catch (error) {
      notice(messageOf(error), "error");
    }
  }, [notice]);

  const sendMessage = useCallback(async () => {
    const content = draft.trim();
    if (sendingRef.current || streamingRef.current) return;
    if (!content && pendingRef.current.length === 0) return;

    const targetId = activeIdRef.current;
    const imagePaths = pendingRef.current.map((item) => item.path);
    sendingRef.current = true;
    setSending(true);
    setChatProblem(null);
    setStopping(false);

    try {
      const result = await api.sendMessage({
        conversationId: targetId,
        content,
        imagePaths,
      });
      setActiveConversation(result.conversationId);
      const next =
        targetId === null
          ? [result.userMessage]
          : [...messagesRef.current, result.userMessage];
      messagesRef.current = next;
      setMessages(next);
      streamingRef.current = {
        requestId: result.requestId,
        conversationId: result.conversationId,
      };
      setStreaming(streamingRef.current);
      setStreamText("");
      setDraft("");
      setPendingList([]);
      lastActivityRef.current = Date.now();
      stickToBottom.current = true;
      void refreshConversations();
    } catch (error) {
      setChatProblem({ message: messageOf(error), kind: "send" });
    } finally {
      sendingRef.current = false;
      setSending(false);
    }
  }, [
    draft,
    refreshConversations,
    setActiveConversation,
    setPendingList,
  ]);

  const regenerate = useCallback(async () => {
    const id = activeIdRef.current;
    if (!id || sendingRef.current || streamingRef.current) return;
    sendingRef.current = true;
    setSending(true);
    setChatProblem(null);
    setStopping(false);
    try {
      const result = await api.regenerate(id);
      const next = messagesRef.current.slice();
      if (next.length > 0 && next[next.length - 1]?.role === "assistant") next.pop();
      messagesRef.current = next;
      setMessages(next);
      streamingRef.current = { requestId: result.requestId, conversationId: id };
      setStreaming(streamingRef.current);
      setStreamText("");
      lastActivityRef.current = Date.now();
      stickToBottom.current = true;
      void refreshConversations();
    } catch (error) {
      setChatProblem({ message: messageOf(error), kind: "stream" });
    } finally {
      sendingRef.current = false;
      setSending(false);
    }
  }, [refreshConversations]);

  const editMessage = useCallback(
    async (messageId: string, content: string): Promise<boolean> => {
      if (!activeIdRef.current || sendingRef.current || streamingRef.current) return false;
      sendingRef.current = true;
      setSending(true);
      setChatProblem(null);
      setStopping(false);
      try {
        const result = await api.editResend(messageId, content);
        const current = messagesRef.current;
        const index = current.findIndex((message) => message.id === messageId);
        const next =
          index >= 0
            ? current.slice(0, index + 1).map((message, position) =>
                position === index ? { ...message, content } : message,
              )
            : current;
        messagesRef.current = next;
        setMessages(next);
        streamingRef.current = {
          requestId: result.requestId,
          conversationId: result.conversationId,
        };
        setStreaming(streamingRef.current);
        setStreamText("");
        lastActivityRef.current = Date.now();
        stickToBottom.current = true;
        void refreshConversations();
        return true;
      } catch (error) {
        setChatProblem({ message: messageOf(error), kind: "stream" });
        return false;
      } finally {
        sendingRef.current = false;
        setSending(false);
      }
    },
    [refreshConversations],
  );

  const deleteFromMessage = useCallback(
    async (message: StoredMessage) => {
      if (sendingRef.current || streamingRef.current) {
        notice("Stop the current reply before removing messages.");
        return;
      }
      try {
        await api.deleteFromMessage(message.id);
        await refreshMessages(message.conversationId);
        void refreshConversations();
        notice("The conversation was trimmed there.");
      } catch (error) {
        notice(messageOf(error), "error");
      }
    },
    [notice, refreshConversations, refreshMessages],
  );

  const selectConversation = useCallback(
    (id: string) => {
      if (id === activeIdRef.current) {
        setDrawer(false);
        return;
      }
      if (sendingRef.current) {
        notice("Give Bernard a moment to start first.");
        return;
      }
      abandonStream();
      setViewMode("chat");
      setDrawer(false);
      setPreview(null);
      void loadConversation(id);
      focusComposer();
    },
    [
      abandonStream,
      focusComposer,
      loadConversation,
      notice,
      setDrawer,
      setPreview,
      setViewMode,
    ],
  );

  const renameConversation = useCallback(
    async (id: string, title: string): Promise<boolean> => {
      try {
        await api.renameConversation(id, title);
        setConversations((current) =>
          current.map((conversation) =>
            conversation.id === id ? { ...conversation, title } : conversation,
          ),
        );
        return true;
      } catch (error) {
        notice(messageOf(error), "error");
        return false;
      }
    },
    [notice],
  );

  const removeConversation = useCallback(
    async (id: string) => {
      if (sendingRef.current || streamingRef.current) {
        notice("Stop the current reply before deleting a conversation.");
        return;
      }
      try {
        await api.deleteConversation(id);
        if (activeIdRef.current === id) {
          loadTokenRef.current += 1;
          setActiveConversation(null);
          messagesRef.current = [];
          setMessages([]);
          setChatProblem(null);
          setPendingList([]);
          setDraft("");
          lastActivityRef.current = 0;
        }
        await refreshConversations();
        notice("Conversation deleted.");
      } catch (error) {
        notice(messageOf(error), "error");
      }
    },
    [
      notice,
      refreshConversations,
      setActiveConversation,
      setPendingList,
    ],
  );

  const stopStream = useCallback(() => {
    const current = streamingRef.current;
    if (!current) return;
    setStopping(true);
    void api
      .stopStream(current.requestId)
      .then((stopped) => {
        if (!stopped && streamingRef.current?.requestId === current.requestId) {
          setStopping(false);
        }
      })
      .catch((error) => {
        setStopping(false);
        notice(messageOf(error), "error");
      });
  }, [notice]);

  const openSettings = useCallback(() => {
    setViewMode("settings");
    setDrawer(false);
    setPreview(null);
  }, [setDrawer, setPreview, setViewMode]);

  const closeSettings = useCallback(() => {
    setViewMode("chat");
    focusComposer();
  }, [focusComposer, setViewMode]);

  const hidePanel = useCallback(() => {
    setDrawer(false);
    setPreview(null);
    void api.hidePanel().catch(() => {});
  }, [setDrawer, setPreview]);

  const openExternal = useCallback(
    async (href: string) => {
      if (!/^(https?:|mailto:)/i.test(href)) {
        notice("Bernard only opens web and email links.");
        return;
      }
      try {
        await api.openExternal(href);
      } catch (error) {
        notice(messageOf(error), "error");
      }
    },
    [notice],
  );

  const syncModel = useCallback(async () => {
    try {
      const [info, current] = await Promise.all([api.modelInfo(), api.getSettings()]);
      setModel(info);
      setSettings(current);
      applyTheme(current.theme);
      watchTheme(current.theme);
      if (activeIdRef.current) void refreshMessages(activeIdRef.current);
    } catch {
      // The server status event will explain any remaining problem.
    }
  }, [refreshMessages]);

  const startDownload = useCallback(
    async (presetId: string) => {
      setDownload(null);
      setDownloadingPreset(presetId);
      try {
        await api.downloadModel(presetId);
      } catch (error) {
        setDownloadingPreset(null);
        notice(messageOf(error), "error");
      }
    },
    [notice],
  );

  const cancelDownload = useCallback(() => {
    void api.cancelDownload().catch(() => {});
  }, []);

  const handleDataCleared = useCallback(() => {
    loadTokenRef.current += 1;
    setActiveConversation(null);
    messagesRef.current = [];
    setMessages([]);
    setConversations([]);
    setAttachments({});
    setPendingList([]);
    setDraft("");
    setChatProblem(null);
    setLoadingMessages(false);
    setDrawer(false);
    setPreview(null);
    lastActivityRef.current = 0;
  }, [setActiveConversation, setDrawer, setPendingList, setPreview]);

  useEffect(() => {
    let cancelled = false;
    setBooted(false);
    setBootError(null);
    void (async () => {
      try {
        const boot = await api.bootstrap();
        if (cancelled) return;
        setSettings(boot.settings);
        setServer(boot.server);
        setModel(boot.model);
        setConversations(boot.conversations);
        setMeta({ appVersion: boot.appVersion, dataDir: boot.dataDir });
        applyTheme(boot.settings.theme);
        watchTheme(boot.settings.theme);
        setBooted(true);
      } catch (error) {
        if (cancelled) return;
        setBootError(messageOf(error));
        setBooted(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bootAttempt]);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const add = (promise: Promise<() => void>) => {
      void promise.then((stop) => {
        if (cancelled) stop();
        else unlisteners.push(stop);
      });
    };

    add(
      on<ServerStatus>(events.serverStatus, (event) => {
        setServer(event.payload);
        if (event.payload.phase === "ready") void syncModel();
      }),
    );

    add(
      on<AppSettings>(events.settingsChanged, (event) => {
        setSettings(event.payload);
        applyTheme(event.payload.theme);
        watchTheme(event.payload.theme);
      }),
    );

    add(
      on(events.conversationsChanged, () => {
        void refreshConversations();
      }),
    );

    add(
      on<ChatDelta>(events.chatDelta, (event) => {
        const current = streamingRef.current;
        if (!current || current.requestId !== event.payload.requestId) return;
        if (activeIdRef.current !== event.payload.conversationId) return;
        setStreamText((text) => text + event.payload.delta);
      }),
    );

    add(
      on<ChatDone>(events.chatDone, (event) => {
        const current = streamingRef.current;
        if (!current || current.requestId !== event.payload.requestId) return;
        streamingRef.current = null;
        setStreaming(null);
        setStreamText("");
        setStopping(false);
        if (activeIdRef.current === event.payload.conversationId) {
          const next = messagesRef.current.some(
            (message) => message.id === event.payload.message.id,
          )
            ? messagesRef.current
            : [...messagesRef.current, event.payload.message];
          messagesRef.current = next;
          setMessages(next);
        }
        lastActivityRef.current = Date.now();
        if (event.payload.contextTrimmed) {
          notice("Older messages were left out to fit the model's context.");
        }
        void refreshConversations();
      }),
    );

    add(
      on<ChatFailed>(events.chatFailed, (event) => {
        const current = streamingRef.current;
        if (!current || current.requestId !== event.payload.requestId) return;
        streamingRef.current = null;
        setStreaming(null);
        setStreamText("");
        setStopping(false);
        setSending(false);
        sendingRef.current = false;
        void refreshMessages(event.payload.conversationId);
        void refreshConversations();
        if (event.payload.message === "Stopped.") {
          notice("Stopped.");
        } else {
          setChatProblem({ message: event.payload.message, kind: "stream" });
        }
      }),
    );

    add(
      on<SnipReady>(events.snipReady, (event) => {
        const payload = event.payload;
        const now = Date.now();
        const recent = now - lastActivityRef.current < SNIP_CONTEXT_WINDOW_MS;
        if (!payload.panelWasVisible && !streamingRef.current && !recent) {
          loadTokenRef.current += 1;
          setActiveConversation(null);
          messagesRef.current = [];
          setMessages([]);
          setChatProblem(null);
          setDraft("");
          setLoadingMessages(false);
          lastActivityRef.current = 0;
        }
        pushAttachment(payload.attachment);
        lastActivityRef.current = now;
        setViewMode("chat");
        setDrawer(false);
        setPreview(null);
        notice("Snip attached. Tell Bernard what to look for.");
        focusComposer();
      }),
    );

    add(
      on<{ message?: string }>(events.snipFailed, (event) => {
        notice(event.payload?.message || "The snip could not be captured.", "error");
      }),
    );

    add(
      on(events.panelOpened, () => {
        void refreshConversations();
        if (viewRef.current === "chat") focusComposer();
      }),
    );

    add(
      on(events.panelClosed, () => {
        setDrawer(false);
        setPreview(null);
        setToast(null);
      }),
    );

    add(
      on(events.panelFocusComposer, () => {
        focusComposer();
      }),
    );

    add(
      on<DownloadProgress>(events.modelProgress, (event) => {
        setDownload(event.payload);
        if (event.payload.done) return;
        setDownloadingPreset(event.payload.presetId);
        if (event.payload.error) {
          setDownloadingPreset(null);
          notice(event.payload.error, "error");
        }
      }),
    );

    add(
      on<string>(events.modelDone, () => {
        setDownload(null);
        setDownloadingPreset(null);
        notice("The model is ready. Bernard is waking it up.", "success");
        window.setTimeout(() => void syncModel(), 500);
        void refreshConversations();
      }),
    );

    add(
      on<{ presetId: string; message: string }>(events.modelFailed, (event) => {
        setDownload(null);
        setDownloadingPreset(null);
        const cancelled = /cancelled/i.test(event.payload.message);
        notice(event.payload.message, cancelled ? "info" : "error");
      }),
    );

    add(
      on(events.dataCleared, () => {
        handleDataCleared();
        notice("Conversations and snips were cleared.");
      }),
    );

    add(
      on(events.menuNewChat, () => {
        startNewChat();
      }),
    );

    add(
      on(events.menuOpenSettings, () => {
        openSettings();
      }),
    );

    return () => {
      cancelled = true;
      for (const stop of unlisteners) stop();
    };
  }, [
    focusComposer,
    handleDataCleared,
    notice,
    openSettings,
    pushAttachment,
    refreshConversations,
    refreshMessages,
    setActiveConversation,
    setDrawer,
    setPreview,
    setViewMode,
    startNewChat,
    syncModel,
  ]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragOver(true);
        } else if (event.payload.type === "drop") {
          setDragOver(false);
          dragDepth.current = 0;
          void addPaths(event.payload.paths);
        } else {
          setDragOver(false);
          dragDepth.current = 0;
        }
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        // Native drag events can be unavailable in a stripped webview; pasting still works.
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [addPaths]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const meta = event.ctrlKey || event.metaKey;
      if (meta && !event.altKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        startNewChat();
        return;
      }
      if (meta && !event.altKey && event.key === ",") {
        event.preventDefault();
        if (viewRef.current === "chat") openSettings();
        return;
      }
      if (event.key === "Escape") {
        if (lightboxRef.current) {
          event.preventDefault();
          setPreview(null);
          return;
        }
        if (drawerRef.current) return;
        if (viewRef.current === "settings") return;
        event.preventDefault();
        hidePanel();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hidePanel, openSettings, setPreview, startNewChat]);

  useLayoutEffect(() => {
    if (view !== "chat") return;
    if (!stickToBottom.current) return;
    const node = scrollRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
  }, [messages, streamText, chatProblem, view, loadingMessages]);

  useEffect(() => {
    if (view !== "chat") return;
    stickToBottom.current = true;
    const timer = window.setTimeout(() => {
      const node = scrollRef.current;
      if (node) node.scrollTop = node.scrollHeight;
    }, 30);
    return () => window.clearTimeout(timer);
  }, [view]);

  const handleScroll = useCallback(() => {
    const node = scrollRef.current;
    if (!node) return;
    stickToBottom.current =
      node.scrollHeight - node.scrollTop - node.clientHeight < 84;
  }, []);

  const handleDragEnter = (event: ReactDragEvent<HTMLDivElement>) => {
    event.preventDefault();
    dragDepth.current += 1;
    if (event.dataTransfer.types.includes("Files")) setDragOver(true);
  };

  const handleDragLeave = (event: ReactDragEvent<HTMLDivElement>) => {
    event.preventDefault();
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) setDragOver(false);
  };

  const handleDrop = (event: ReactDragEvent<HTMLDivElement>) => {
    event.preventDefault();
    dragDepth.current = 0;
    setDragOver(false);
    const files = Array.from(event.dataTransfer.files);
    if (files.length > 0) {
      void addFiles(files);
      return;
    }
    const paths = Array.from(event.dataTransfer.items)
      .map((item) => (item.kind === "file" ? item.getAsFile() : null))
      .filter((file): file is File => Boolean(file));
    if (paths.length > 0) void addFiles(paths);
  };

  const banner = bannerFor(server);
  const status = headerStatus(server);
  const busy = sending || Boolean(streaming);
  const lastAssistantIndex = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index]?.role === "assistant") return index;
    }
    return -1;
  }, [messages]);

  const retryChat = useCallback(() => {
    const kind = chatProblem?.kind;
    setChatProblem(null);
    if (kind === "send") void sendMessage();
    else void regenerate();
  }, [chatProblem?.kind, regenerate, sendMessage]);

  const drawer = (
    <ConversationDrawer
      open={drawerOpen}
      conversations={conversations}
      activeId={activeId}
      onClose={() => setDrawer(false)}
      onSelect={selectConversation}
      onNew={startNewChat}
      onRename={renameConversation}
      onDelete={removeConversation}
      onSettings={openSettings}
      onOpenData={() => {
        void api.openDataFolder().catch((error) => notice(messageOf(error), "error"));
      }}
    />
  );

  const toastView = toast ? (
    <div className="toast-layer" aria-live="polite">
      <div className={`toast toast--${toast.tone}`} key={toast.id}>
        <span className="toast-mark" aria-hidden>
          {toast.tone === "success" ? (
            <CheckIcon size={14} />
          ) : toast.tone === "error" ? (
            <AlertIcon size={14} />
          ) : (
            <SparkIcon size={14} />
          )}
        </span>
        <span className="toast-text">{toast.message}</span>
        <button
          type="button"
          className="toast-close"
          onClick={() => setToast(null)}
          aria-label="Dismiss notice"
        >
          <CloseIcon size={12} />
        </button>
      </div>
    </div>
  ) : null;

  const previewView = lightbox ? (
    <div
      className="preview-layer"
      role="dialog"
      aria-modal="true"
      aria-label="Snip preview"
      onClick={() => setPreview(null)}
    >
      <button
        type="button"
        className="preview-close"
        onClick={() => setPreview(null)}
        aria-label="Close preview"
      >
        <CloseIcon size={18} />
      </button>
      <figure className="preview-figure" onClick={(event) => event.stopPropagation()}>
        <img src={lightbox.dataUrl} alt="Enlarged snip" />
        <figcaption>
          {lightbox.width} × {lightbox.height}
        </figcaption>
      </figure>
    </div>
  ) : null;

  return (
    <div
      className="panel-frame"
      onDragEnter={handleDragEnter}
      onDragOver={(event) => {
        event.preventDefault();
        if (event.dataTransfer.types.includes("Files")) setDragOver(true);
      }}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className="panel-shell">
        {!booted ? (
          <div className="panel-boot">
            <span className="boot-mark">
              <LogoMark size={44} />
            </span>
            <span className="button-spinner" aria-hidden />
            <p>Waking Bernard…</p>
          </div>
        ) : bootError || !settings ? (
          <div className="panel-boot panel-boot--error">
            <span className="boot-mark">
              <LogoMark size={44} />
            </span>
            <h1>Bernard could not open</h1>
            <p>{bootError || "The settings store could not be read."}</p>
            <button
              type="button"
              className="primary-button"
              onClick={() => setBootAttempt((value) => value + 1)}
            >
              Try again
            </button>
          </div>
        ) : view === "settings" ? (
          <>
            <SettingsView
              settings={settings}
              model={model}
              server={server}
              dataDir={meta.dataDir}
              appVersion={meta.appVersion}
              download={download}
              downloadingPreset={downloadingPreset}
              onBack={closeSettings}
              onSettings={(next) => {
                setSettings(next);
                applyTheme(next.theme);
                watchTheme(next.theme);
              }}
              onModel={setModel}
              onDownload={startDownload}
              onCancelDownload={cancelDownload}
              onNotice={notice}
              onDataCleared={handleDataCleared}
            />
            {drawer}
          </>
        ) : (
          <>
            <header className="panel-head">
              <div className="brand">
                <span className="brand-mark">
                  <LogoMark size={27} />
                </span>
                <div className="brand-text">
                  <span className="brand-name">Bernard</span>
                  <span className={`brand-status brand-status--${status.tone}`} title={server?.message}>
                    <i aria-hidden />
                    {status.label}
                  </span>
                </div>
              </div>
              <div className="panel-head-actions">
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => {
                    setDrawer(true);
                    void refreshConversations();
                  }}
                  title="Conversations"
                  aria-label="Open conversations"
                >
                  <HistoryIcon size={16} />
                </button>
                <button
                  type="button"
                  className="icon-button"
                  onClick={startNewChat}
                  title="New chat (Ctrl+N)"
                  aria-label="New chat"
                >
                  <PlusIcon size={17} />
                </button>
                <button
                  type="button"
                  className="icon-button"
                  onClick={openSettings}
                  title="Settings"
                  aria-label="Open settings"
                >
                  <SettingsIcon size={16} />
                </button>
                <button
                  type="button"
                  className="icon-button icon-button--quiet"
                  onClick={hidePanel}
                  title="Hide panel (Esc)"
                  aria-label="Hide panel"
                >
                  <CloseIcon size={16} />
                </button>
              </div>
            </header>

            <div className="chat-scroll" ref={scrollRef} onScroll={handleScroll}>
              <div className="chat-inner">
                {banner ? (
                  <div className={`status-banner status-banner--${banner.tone}`} aria-live="polite">
                    <span className="status-banner-mark" aria-hidden>
                      {banner.tone === "busy" ? (
                        <span className="button-spinner" />
                      ) : (
                        <AlertIcon size={15} />
                      )}
                    </span>
                    <div className="status-banner-copy">
                      <strong>{banner.title}</strong>
                      <span>{banner.detail}</span>
                    </div>
                    {banner.tone !== "busy" ? (
                      <button type="button" className="text-button text-button--accent" onClick={openSettings}>
                        Open settings
                      </button>
                    ) : null}
                  </div>
                ) : null}

                {messages.length === 0 && !streaming && !loadingMessages && !chatProblem ? (
                  <div className="chat-empty">
                    <span className="chat-empty-mark">
                      <LogoMark size={46} />
                    </span>
                    <h1>What are we looking at?</h1>
                    <p>
                      Snip a window, drop an image, or just ask. Bernard runs entirely on
                      this PC.
                    </p>
                    <div className="chat-empty-actions">
                      <button type="button" className="chat-suggestion" onClick={() => void startSnip()}>
                        <CameraIcon size={16} />
                        <span>Explain what's on my screen</span>
                      </button>
                      <button
                        type="button"
                        className="chat-suggestion"
                        onClick={() => {
                          setDraft("Help me understand this error. Here's the message:\n\n");
                          focusComposer();
                        }}
                      >
                        <AlertIcon size={16} />
                        <span>Untangle an error</span>
                      </button>
                      <button
                        type="button"
                        className="chat-suggestion"
                        onClick={() => {
                          setDraft("Help me draft a short, clear email about ");
                          focusComposer();
                        }}
                      >
                        <SparkIcon size={16} />
                        <span>Draft something</span>
                      </button>
                    </div>
                  </div>
                ) : null}

                {loadingMessages ? (
                  <div className="chat-loading" aria-live="polite">
                    <span className="button-spinner" aria-hidden />
                    <span>Opening the conversation…</span>
                  </div>
                ) : null}

                {messages.map((message, index) => {
                  const previous = messages[index - 1];
                  const showDay = !previous || !sameDay(previous.createdAt, message.createdAt);
                  return (
                    <Fragment key={message.id}>
                      {showDay ? (
                        <div className="chat-day">
                          <span>{formatDateGroup(message.createdAt)}</span>
                        </div>
                      ) : null}
                      <Message
                        message={message}
                        attachments={attachments}
                        lastAssistant={index === lastAssistantIndex}
                        busy={busy}
                        showPerformance={settings.showPerformance}
                        onEdit={editMessage}
                        onRegenerate={() => void regenerate()}
                        onDeleteFrom={(target) => void deleteFromMessage(target)}
                        onOpenImage={setPreview}
                        onOpenLink={openExternal}
                      />
                    </Fragment>
                  );
                })}

                {streaming ? (
                  <StreamingMessage text={streamText} stopping={stopping} />
                ) : null}

                {chatProblem ? (
                  <ChatError
                    message={chatProblem.message}
                    onRetry={retryChat}
                    onDismiss={() => setChatProblem(null)}
                    busy={busy}
                  />
                ) : null}
              </div>
            </div>

            <div className="chat-foot">
              <Composer
                value={draft}
                onChange={setDraft}
                onSend={() => void sendMessage()}
                onStop={stopStream}
                onSnip={() => void startSnip()}
                onFiles={(files) => void addFiles(files)}
                onPasteImage={(file) => void addFiles([file])}
                onImportClipboard={() => void importClipboard()}
                onRemoveAttachment={removeAttachment}
                attachments={pending}
                sending={sending}
                streaming={Boolean(streaming)}
                textareaRef={textareaRef}
              />
            </div>

            {drawer}
          </>
        )}

        {dragOver ? (
          <div className="drop-veil" aria-hidden>
            <div className="drop-veil-card">
              <CameraIcon size={24} />
              <strong>Drop it on Bernard</strong>
              <span>PNG, JPEG, WebP, GIF, or BMP · up to four</span>
            </div>
          </div>
        ) : null}

        {previewView}
        {toastView}
      </div>
    </div>
  );
}
