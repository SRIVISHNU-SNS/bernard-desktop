import { useEffect, useRef, useState } from "react";
import { api } from "../lib/bridge";
import { fileName } from "../lib/files";
import { formatClock, formatDuration } from "../lib/format";
import type { Attachment, StoredMessage } from "../lib/types";
import { Markdown } from "./Markdown";
import {
  AlertIcon,
  CheckIcon,
  CopyIcon,
  PencilIcon,
  RefreshIcon,
  TrashIcon,
} from "../lib/icons";

interface MessageProps {
  message: StoredMessage;
  attachments: Record<string, Attachment>;
  lastAssistant: boolean;
  busy: boolean;
  showPerformance: boolean;
  onEdit: (messageId: string, content: string) => Promise<boolean>;
  onRegenerate: () => void;
  onDeleteFrom: (message: StoredMessage) => void;
  onOpenImage: (attachment: Attachment) => void;
  onOpenLink: (href: string) => void;
}

export function Message({
  message,
  attachments,
  lastAssistant,
  busy,
  showPerformance,
  onEdit,
  onRegenerate,
  onDeleteFrom,
  onOpenImage,
  onOpenLink,
}: MessageProps) {
  const mine = message.role === "user";
  const [copied, setCopied] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(message.content);
  const [savingEdit, setSavingEdit] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const deleteTimer = useRef<number | null>(null);

  useEffect(() => {
    setDraft(message.content);
  }, [message.content]);

  useEffect(() => {
    if (!editing) return;
    const node = editRef.current;
    if (!node) return;
    node.style.height = "auto";
    node.style.height = `${Math.min(node.scrollHeight, 220)}px`;
    node.focus();
    node.setSelectionRange(node.value.length, node.value.length);
  }, [editing]);

  useEffect(() => {
    return () => {
      if (deleteTimer.current) window.clearTimeout(deleteTimer.current);
    };
  }, []);

  const copy = () => {
    void api
      .copyText(message.content)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1600);
      })
      .catch(() => setCopied(false));
  };

  const askDelete = () => {
    setConfirmingDelete(true);
    if (deleteTimer.current) window.clearTimeout(deleteTimer.current);
    deleteTimer.current = window.setTimeout(() => setConfirmingDelete(false), 5000);
  };

  const submitEdit = async () => {
    const next = draft.trim();
    if (!next) {
      setEditError("A message cannot be empty.");
      return;
    }
    setSavingEdit(true);
    setEditError(null);
    const edited = await onEdit(message.id, next);
    setSavingEdit(false);
    if (edited) setEditing(false);
  };

  const images = message.imagePaths
    .map((path) => attachments[path])
    .filter((attachment): attachment is Attachment => Boolean(attachment));

  const metrics = !mine && showPerformance ? message : null;

  return (
    <article className={`message message--${mine ? "user" : "assistant"}`}>
      {!mine ? (
        <header className="message-head">
          <span className="message-face" aria-hidden>
            b
          </span>
          <span className="message-author">Bernard</span>
          <span className="message-time">{formatClock(message.createdAt)}</span>
        </header>
      ) : null}

      <div className={`message-body${mine ? " message-body--user" : ""}`}>
        {message.imagePaths.length > 0 ? (
          <div className={`message-images message-images--${Math.min(images.length, 2)}`}>
            {message.imagePaths.map((path) => {
              const attachment = attachments[path];
              return attachment ? (
                <button
                  key={path}
                  type="button"
                  className="message-image"
                  onClick={() => onOpenImage(attachment)}
                  title={`Open ${fileName(path)}`}
                >
                  <img src={attachment.dataUrl} alt={`Attached snip ${fileName(path)}`} />
                </button>
              ) : (
                <span key={path} className="message-image message-image--missing" aria-hidden>
                  <AlertIcon size={15} />
                </span>
              );
            })}
          </div>
        ) : null}

        {editing ? (
          <div className="message-edit">
            <textarea
              ref={editRef}
              value={draft}
              onChange={(event) => {
                setDraft(event.target.value);
                setEditError(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void submitEdit();
                }
                if (event.key === "Escape") {
                  event.preventDefault();
                  setEditing(false);
                  setDraft(message.content);
                  setEditError(null);
                }
              }}
              disabled={savingEdit}
              aria-label="Edit your message"
            />
            {editError ? <span className="message-edit-error">{editError}</span> : null}
            <div className="message-edit-actions">
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  setEditing(false);
                  setDraft(message.content);
                  setEditError(null);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                className="text-button text-button--accent"
                onClick={() => void submitEdit()}
                disabled={savingEdit}
              >
                {savingEdit ? "Sending…" : "Save & resend"}
              </button>
            </div>
          </div>
        ) : mine ? (
          message.content ? (
            <div className="message-user-text">{message.content}</div>
          ) : (
            <div className="message-user-text message-user-text--muted">Sent a snip</div>
          )
        ) : (
          <div className="message-assistant-text">
            <Markdown source={message.content} onLink={onOpenLink} />
          </div>
        )}

        {!mine && metrics && (metrics.tokensPerSecond || metrics.ttftMs) ? (
          <div className="message-metrics" aria-label="Performance for this reply">
            {metrics.ttftMs ? (
              <span>
                <strong>{formatDuration(metrics.ttftMs)}</strong> to first token
              </span>
            ) : null}
            {metrics.tokensPerSecond ? (
              <span>
                <strong>{metrics.tokensPerSecond.toFixed(1)}</strong> tok/s
              </span>
            ) : null}
            {metrics.promptTokens ? <span>{metrics.promptTokens} prompt</span> : null}
            {metrics.cacheTokens ? <span>{metrics.cacheTokens} cached</span> : null}
          </div>
        ) : null}
      </div>

      {!editing ? (
        <footer className={`message-actions${mine ? " message-actions--user" : ""}`}>
          {confirmingDelete ? (
            <div className="message-confirm" role="group" aria-label="Confirm deletion">
              <span>Delete from here?</span>
              <button
                type="button"
                className="msg-action msg-action--danger"
                onClick={() => {
                  if (deleteTimer.current) window.clearTimeout(deleteTimer.current);
                  setConfirmingDelete(false);
                  onDeleteFrom(message);
                }}
                disabled={busy}
              >
                <CheckIcon size={12} />
                Delete
              </button>
              <button
                type="button"
                className="msg-action"
                onClick={() => setConfirmingDelete(false)}
              >
                Cancel
              </button>
            </div>
          ) : (
            <>
              <button type="button" className="msg-action" onClick={copy}>
                {copied ? <CheckIcon size={12} /> : <CopyIcon size={12} />}
                {copied ? "Copied" : "Copy"}
              </button>
              {mine ? (
                <button
                  type="button"
                  className="msg-action"
                  onClick={() => {
                    setDraft(message.content);
                    setEditing(true);
                  }}
                  disabled={busy}
                >
                  <PencilIcon size={12} />
                  Edit
                </button>
              ) : null}
              {!mine && lastAssistant ? (
                <button
                  type="button"
                  className="msg-action"
                  onClick={onRegenerate}
                  disabled={busy}
                >
                  <RefreshIcon size={12} />
                  Regenerate
                </button>
              ) : null}
              <button
                type="button"
                className="msg-action msg-action--quiet"
                onClick={askDelete}
                disabled={busy}
              >
                <TrashIcon size={12} />
                Delete from here
              </button>
            </>
          )}
        </footer>
      ) : null}

      {mine ? <div className="message-time message-time--user">{formatClock(message.createdAt)}</div> : null}
    </article>
  );
}

export function StreamingMessage({ text, stopping }: { text: string; stopping: boolean }) {
  return (
    <article className="message message--assistant message--streaming">
      <header className="message-head">
        <span className="message-face message-face--live" aria-hidden>
          b
        </span>
        <span className="message-author">Bernard</span>
        <span className="message-time">{stopping ? "Stopping…" : "Writing…"}</span>
      </header>
      <div className="message-body">
        <div className="message-assistant-text">
          {text ? (
            <Markdown source={text} />
          ) : (
            <span className="thinking-line">
              <i aria-hidden />
              <i aria-hidden />
              <i aria-hidden />
              <span>Thinking</span>
            </span>
          )}
          {text ? <span className="stream-caret" aria-hidden /> : null}
        </div>
      </div>
    </article>
  );
}

export function ChatError({
  message,
  onRetry,
  onDismiss,
  busy,
}: {
  message: string;
  onRetry: () => void;
  onDismiss: () => void;
  busy: boolean;
}) {  return (
    <div className="chat-error" role="alert">
      <AlertIcon size={16} />
      <div className="chat-error-copy">
        <strong>Bernard hit a snag</strong>
        <span>{message}</span>
      </div>
      <div className="chat-error-actions">
        <button type="button" className="text-button text-button--accent" onClick={onRetry} disabled={busy}>
          Try again
        </button>
        <button type="button" className="text-button" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}
