import type { KeyboardEvent as ReactKeyboardEvent } from "react";

const NAMED_KEYS: Record<string, string> = {
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Minus: "Minus",
  Equal: "Equal",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Backslash: "Backslash",
  Semicolon: "Semicolon",
  Quote: "Quote",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
  Backquote: "Backquote",
};

const RESERVED = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

export function hotkeyFromEvent(event: ReactKeyboardEvent | KeyboardEvent): string | null {
  if (RESERVED.has(event.code)) return null;
  if (event.key === "Escape" || event.key === "Tab" || event.key === "Enter") return null;

  let key: string | null = null;
  if (/^Key[A-Z]$/.test(event.code)) {
    key = event.code.slice(3);
  } else if (/^Digit[0-9]$/.test(event.code)) {
    key = event.code.slice(5);
  } else if (/^Numpad[0-9]$/.test(event.code)) {
    key = `Num${event.code.slice(6)}`;
  } else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.code)) {
    key = event.code;
  } else {
    key = NAMED_KEYS[event.code] ?? null;
  }
  if (!key) return null;

  const modifiers: string[] = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Super");

  if (modifiers.length === 0 && !key.startsWith("F")) return null;
  return [...modifiers, key].join("+");
}

export function prettyHotkey(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => {
      if (part === "ArrowUp") return "↑";
      if (part === "ArrowDown") return "↓";
      if (part === "ArrowLeft") return "←";
      if (part === "ArrowRight") return "→";
      return part;
    })
    .join(" + ");
}
