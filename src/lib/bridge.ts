import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn, type EventCallback } from "@tauri-apps/api/event";
import type {
  AppSettings,
  Attachment,
  Bootstrap,
  ChatDelta,
  ChatDone,
  ChatFailed,
  Conversation,
  Diagnostics,
  DownloadProgress,
  ModelFileInfo,
  SendMessageRequest,
  SendMessageResult,
  ServerMetrics,
  ServerStatus,
  SnipReady,
  SnipSelection,
  StoredMessage,
  Theme,
  OverlayPayload,
} from "./types";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const message =
      typeof error === "string"
        ? error
        : error instanceof Error
          ? error.message
          : "Bernard could not complete that request.";
    throw new Error(message);
  }
}

export const api = {
  bootstrap: () => call<Bootstrap>("bootstrap"),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  listConversations: (search: string) => call<Conversation[]>("list_conversations", { search }),
  createConversation: () => call<Conversation>("create_conversation"),
  renameConversation: (id: string, title: string) =>
    call<void>("rename_conversation", { id, title }),
  deleteConversation: (id: string) => call<void>("delete_conversation", { id }),
  getMessages: (conversationId: string) =>
    call<StoredMessage[]>("get_messages", { conversationId }),
  loadAttachments: (paths: string[]) => call<Attachment[]>("load_attachments", { paths }),
  sendMessage: (request: SendMessageRequest) =>
    call<SendMessageResult>("send_message", { request }),
  stopStream: (requestId: string) => call<boolean>("stop_stream", { requestId }),
  regenerate: (conversationId: string) =>
    call<SendMessageResult>("regenerate", { conversationId }),
  editResend: (messageId: string, content: string) =>
    call<SendMessageResult>("edit_resend", { messageId, content }),
  deleteFromMessage: (messageId: string) =>
    call<void>("delete_from_message", { messageId }),
  copyText: (text: string) => call<void>("copy_text", { text }),
  openExternal: (url: string) => call<void>("open_external", { url }),
  importClipboard: () => call<Attachment>("import_clipboard"),
  saveDroppedImage: (path: string) => call<Attachment>("save_dropped_image", { path }),
  savePastedImage: (dataUrl: string) =>
    call<Attachment>("save_pasted_image", { dataUrl }),
  snipNow: () => call<void>("snip_now"),
  cancelSnip: () => call<void>("cancel_snip"),
  snipCommit: (selection: SnipSelection) => call<void>("snip_commit", { selection }),
  overlayPayload: (label: string) => call<OverlayPayload>("overlay_payload", { label }),
  togglePanel: () => call<void>("toggle_panel"),
  showPanel: () => call<void>("show_panel"),
  hidePanel: () => call<void>("hide_panel"),
  showAvatarMenu: () => call<void>("show_avatar_menu"),
  hideAvatarMenu: () => call<void>("hide_avatar_menu"),
  hideAvatar: () => call<void>("hide_avatar"),
  showAvatar: () => call<void>("show_avatar"),
  quit: () => call<void>("quit_app"),
  startDragging: () => call<void>("start_dragging"),
  persistAvatarPosition: () => call<void>("persist_avatar_position"),
  serverStatus: () => call<ServerStatus>("server_status"),
  restartServer: () => call<void>("restart_server"),
  diagnostics: () => call<Diagnostics>("diagnostics"),
  modelInfo: () => call<ModelFileInfo>("model_info"),
  downloadModel: (presetId: string) => call<void>("download_model", { presetId }),
  cancelDownload: () => call<void>("cancel_download"),
  pickModelFile: (kind: "model" | "mmproj") =>
    call<string | null>("pick_model_file", { kind }),
  resetSettings: () => call<AppSettings>("reset_settings"),
  clearAllData: () => call<void>("clear_all_data"),
  openDataFolder: () => call<void>("open_data_folder"),
  setTheme: (theme: Theme) => call<AppSettings>("set_theme", { theme }),
  notifyAvatarState: (stateName: string) =>
    call<void>("notify_avatar_state", { stateName }),
};

export function on<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return listen<T>(event, handler);
}

export const events = {
  serverStatus: "server://status",
  serverMetrics: "server://metrics",
  avatarState: "avatar://state",
  panelOpened: "panel://opened",
  panelClosed: "panel://closed",
  panelFocusComposer: "panel://focus-composer",
  menuNewChat: "menu://new-chat",
  menuOpenSettings: "menu://open-settings",
  chatDelta: "chat://delta",
  chatDone: "chat://done",
  chatFailed: "chat://failed",
  conversationsChanged: "conversations://changed",
  settingsChanged: "settings://changed",
  snipShow: "snip://show",
  snipReady: "snip://ready",
  snipCancelled: "snip://cancelled",
  snipFailed: "snip://failed",
  modelProgress: "model://progress",
  modelDone: "model://done",
  modelFailed: "model://failed",
  performanceSample: "performance://sample",
  hotkeyRegistered: "hotkey://registered",
  dataCleared: "data://cleared",
} as const;

export type {
  ChatDelta,
  ChatDone,
  ChatFailed,
  DownloadProgress,
  ServerMetrics,
  SnipReady,
};
