import { useEffect, useMemo, useRef, useState } from "react";
import type { Conversation } from "../lib/types";
import { formatDateGroup, formatRelative } from "../lib/format";
import {
  CheckIcon,
  CloseIcon,
  FolderIcon,
  HistoryIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  TrashIcon,
} from "../lib/icons";

interface ConversationDrawerProps {
  open: boolean;
  conversations: Conversation[];
  activeId: string | null;
  onClose: () => void;
  onSelect: (id: string) => void;
  onNew: () => void;
  onRename: (id: string, title: string) => Promise<boolean>;
  onDelete: (id: string) => Promise<void>;
  onSettings: () => void;
  onOpenData: () => void;
}

export function ConversationDrawer({
  open,
  conversations,
  activeId,
  onClose,
  onSelect,
  onNew,
  onRename,
  onDelete,
  onSettings,
  onOpenData,
}: ConversationDrawerProps) {
  const [query, setQuery] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  const [savingRename, setSavingRename] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const skipBlur = useRef(false);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setRenamingId(null);
      setConfirmingId(null);
      return;
    }
    const timer = window.setTimeout(() => searchRef.current?.focus(), 60);
    return () => window.clearTimeout(timer);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (renamingId) {
          setRenamingId(null);
        } else if (confirmingId) {
          setConfirmingId(null);
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [confirmingId, onClose, open, renamingId]);

  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = conversations
      .filter((conversation) =>
        needle ? conversation.title.toLowerCase().includes(needle) : true,
      )
      .slice()
      .sort((a, b) => b.updatedAt - a.updatedAt);
    const map = new Map<string, Conversation[]>();
    for (const conversation of filtered) {
      const label = formatDateGroup(conversation.updatedAt);
      const bucket = map.get(label);
      if (bucket) bucket.push(conversation);
      else map.set(label, [conversation]);
    }
    return Array.from(map.entries());
  }, [conversations, query]);

  if (!open) return null;

  const beginRename = (conversation: Conversation) => {
    setConfirmingId(null);
    setRenamingId(conversation.id);
    setRenameDraft(conversation.title);
    skipBlur.current = false;
  };

  const commitRename = async () => {
    if (!renamingId || savingRename) return;
    const title = renameDraft.trim();
    if (!title) {
      setRenamingId(null);
      return;
    }
    setSavingRename(true);
    const ok = await onRename(renamingId, title);
    setSavingRename(false);
    if (ok) setRenamingId(null);
  };

  const total = conversations.length;

  return (
    <div className="drawer-layer">
      <button
        type="button"
        className="drawer-backdrop"
        aria-label="Close conversations"
        onClick={onClose}
        tabIndex={-1}
      />
      <aside className="drawer" role="dialog" aria-modal="true" aria-label="Conversations">
        <header className="drawer-head">
          <div className="drawer-heading">
            <HistoryIcon size={17} />
            <span>Conversations</span>
            {total > 0 ? <span className="drawer-count">{total}</span> : null}
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label="Close conversations"
            title="Close"
          >
            <CloseIcon size={15} />
          </button>
        </header>

        <button type="button" className="drawer-new" onClick={onNew}>
          <PlusIcon size={15} />
          <span>New chat</span>
          <kbd>Ctrl N</kbd>
        </button>

        <div className="drawer-search">
          <SearchIcon size={14} />
          <input
            ref={searchRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search conversations"
            aria-label="Search conversations"
          />
          {query ? (
            <button
              type="button"
              className="drawer-search-clear"
              onClick={() => {
                setQuery("");
                searchRef.current?.focus();
              }}
              aria-label="Clear search"
            >
              <CloseIcon size={12} />
            </button>
          ) : null}
        </div>

        <div className="drawer-list">
          {groups.length === 0 ? (
            <div className="drawer-empty">
              <span>{query ? "No conversations match that search." : "No conversations yet."}</span>
              {!query ? (
                <button type="button" className="text-button text-button--accent" onClick={onNew}>
                  Start a chat
                </button>
              ) : null}
            </div>
          ) : (
            groups.map(([label, items]) => (
              <section key={label} className="drawer-group" aria-label={label}>
                <h2 className="drawer-group-label">{label}</h2>
                {items.map((conversation) => {
                  const active = conversation.id === activeId;
                  const renaming = conversation.id === renamingId;
                  const confirming = conversation.id === confirmingId;
                  return (
                    <div
                      key={conversation.id}
                      className={`drawer-row${active ? " drawer-row--active" : ""}`}
                    >
                      {renaming ? (
                        <div className="drawer-rename">
                          <input
                            value={renameDraft}
                            onChange={(event) => setRenameDraft(event.target.value)}
                            onKeyDown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                void commitRename();
                              }
                              if (event.key === "Escape") {
                                event.preventDefault();
                                skipBlur.current = true;
                                setRenamingId(null);
                              }
                            }}
                            onBlur={() => {
                              if (skipBlur.current) {
                                skipBlur.current = false;
                                return;
                              }
                              void commitRename();
                            }}
                            autoFocus
                            maxLength={120}
                            aria-label="Conversation name"
                          />
                        </div>
                      ) : (
                        <button
                          type="button"
                          className="drawer-main"
                          onClick={() => onSelect(conversation.id)}
                          title={conversation.title}
                        >
                          <span className="drawer-title">{conversation.title}</span>
                          <span className="drawer-meta">
                            {conversation.messageCount} {conversation.messageCount === 1 ? "message" : "messages"}
                            <span aria-hidden> · </span>
                            {formatRelative(conversation.updatedAt)}
                          </span>
                        </button>
                      )}

                      {confirming ? (
                        <div className="drawer-confirm">
                          <button
                            type="button"
                            className="drawer-action drawer-action--danger"
                            onClick={() => {
                              setConfirmingId(null);
                              void onDelete(conversation.id);
                            }}
                            title="Delete conversation"
                            aria-label={`Delete ${conversation.title}`}
                          >
                            <CheckIcon size={13} />
                          </button>
                          <button
                            type="button"
                            className="drawer-action"
                            onClick={() => setConfirmingId(null)}
                            title="Keep conversation"
                            aria-label="Keep conversation"
                          >
                            <CloseIcon size={13} />
                          </button>
                        </div>
                      ) : (
                        <div className="drawer-row-actions">
                          <button
                            type="button"
                            className="drawer-action"
                            onClick={() => beginRename(conversation)}
                            aria-label={`Rename ${conversation.title}`}
                            title="Rename"
                          >
                            <PencilIcon size={13} />
                          </button>
                          <button
                            type="button"
                            className="drawer-action drawer-action--danger"
                            onClick={() => setConfirmingId(conversation.id)}
                            aria-label={`Delete ${conversation.title}`}
                            title="Delete"
                          >
                            <TrashIcon size={13} />
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </section>
            ))
          )}
        </div>

        <footer className="drawer-foot">
          <button type="button" className="drawer-foot-button" onClick={onSettings}>
            <SettingsIcon size={15} />
            Settings
          </button>
          <button type="button" className="drawer-foot-button" onClick={onOpenData}>
            <FolderIcon size={15} />
            Data folder
          </button>
        </footer>
      </aside>
    </div>
  );
}
