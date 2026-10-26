import { useLayoutEffect, useRef, type RefObject } from "react";
import type { Attachment } from "../lib/types";
import { fileName } from "../lib/files";
import {
  CameraIcon,
  ClipboardIcon,
  CloseIcon,
  PaperclipIcon,
  StopIcon,
  ArrowUpIcon,
} from "../lib/icons";

interface ComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onStop: () => void;
  onSnip: () => void;
  onFiles: (files: File[]) => void;
  onPasteImage: (file: File) => void;
  onImportClipboard: () => void;
  onRemoveAttachment: (path: string) => void;
  attachments: Attachment[];
  sending: boolean;
  streaming: boolean;
  autoFocus?: boolean;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}

export function Composer({
  value,
  onChange,
  onSend,
  onStop,
  onSnip,
  onFiles,
  onPasteImage,
  onImportClipboard,
  onRemoveAttachment,
  attachments,
  sending,
  streaming,
  textareaRef,
}: ComposerProps) {
  const fileRef = useRef<HTMLInputElement>(null);
  const busy = sending || streaming;
  const canSend = !busy && (value.trim().length > 0 || attachments.length > 0);

  useLayoutEffect(() => {
    const node = textareaRef.current;
    if (!node) return;
    node.style.height = "auto";
    const next = Math.min(Math.max(node.scrollHeight, 24), 168);
    node.style.height = `${next}px`;
  }, [value, attachments.length, textareaRef]);

  return (
    <div className={`composer${busy ? " composer--busy" : ""}`}>
      {attachments.length > 0 ? (
        <div className="composer-attachments" aria-label="Attached images">
          {attachments.map((attachment) => (
            <div key={attachment.path} className="composer-thumb">
              <img src={attachment.dataUrl} alt={`Attached ${fileName(attachment.path)}`} />
              <button
                type="button"
                className="composer-thumb-remove"
                onClick={() => onRemoveAttachment(attachment.path)}
                aria-label={`Remove ${fileName(attachment.path)}`}
                title="Remove snip"
              >
                <CloseIcon size={11} />
              </button>
            </div>
          ))}
          <span className="composer-attachments-hint">
            {attachments.length} of 4
          </span>
        </div>
      ) : null}

      <div className="composer-box">
        <textarea
          ref={textareaRef}
          className="composer-input"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              if (canSend) onSend();
            }
            if (event.key === "Escape" && streaming) {
              event.preventDefault();
              onStop();
            }
          }}
          onPaste={(event) => {
            const files: File[] = [];
            for (const item of Array.from(event.clipboardData.items)) {
              if (item.kind !== "file" || !item.type.startsWith("image/")) continue;
              const file = item.getAsFile();
              if (file) files.push(file);
            }
            if (files.length > 0) {
              event.preventDefault();
              for (const file of files) onPasteImage(file);
            }
          }}
          placeholder={busy ? "Bernard is working…" : "Ask Bernard anything…"}
          aria-label="Message Bernard"
          rows={1}
          spellCheck
        />
        <div className="composer-bar">
          <div className="composer-tools">
            <button
              type="button"
              className="composer-tool"
              onClick={onSnip}
              title="Snip part of your screen"
              aria-label="Snip part of your screen"
              disabled={busy}
            >
              <CameraIcon size={16} />
            </button>
            <button
              type="button"
              className="composer-tool"
              onClick={() => fileRef.current?.click()}
              title="Attach an image"
              aria-label="Attach an image"
              disabled={busy}
            >
              <PaperclipIcon size={16} />
            </button>
            <button
              type="button"
              className="composer-tool"
              onClick={onImportClipboard}
              title="Attach the image on your clipboard"
              aria-label="Attach the image on your clipboard"
              disabled={busy}
            >
              <ClipboardIcon size={16} />
            </button>
            <input
              ref={fileRef}
              type="file"
              accept="image/png,image/jpeg,image/webp,image/gif,image/bmp"
              multiple
              hidden
              onChange={(event) => {
                const files = Array.from(event.target.files ?? []);
                event.target.value = "";
                if (files.length > 0) onFiles(files);
              }}
            />
          </div>

          <div className="composer-send-wrap">
            {streaming ? (
              <button
                type="button"
                className="composer-send composer-send--stop"
                onClick={onStop}
                title="Stop Bernard"
                aria-label="Stop Bernard"
              >
                <StopIcon size={16} />
              </button>
            ) : (
              <button
                type="button"
                className="composer-send"
                onClick={onSend}
                disabled={!canSend}
                title="Send message"
                aria-label="Send message"
              >
                {sending ? <span className="button-spinner" aria-hidden /> : <ArrowUpIcon size={17} />}
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="composer-note">
        <span>Enter to send · Shift+Enter for a new line</span>
        <span className="composer-local">Runs on this PC</span>
      </div>
    </div>
  );
}
