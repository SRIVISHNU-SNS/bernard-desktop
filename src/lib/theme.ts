import { useEffect } from "react";

export type ThemeSetting = "system" | "light" | "dark";

const listeners = new WeakMap<Document, MediaQueryList>();
const cleanups = new WeakMap<Document, () => void>();

export function applyTheme(theme: ThemeSetting, doc: Document = document) {
  const dark =
    theme === "dark" ||
    (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  doc.documentElement.dataset.theme = dark ? "dark" : "light";
  doc.documentElement.style.colorScheme = dark ? "dark" : "light";
}

export function watchTheme(theme: ThemeSetting, doc: Document = document) {
  const previous = cleanups.get(doc);
  if (previous) previous();
  if (theme !== "system") {
    cleanups.set(doc, () => {});
    return;
  }
  let query = listeners.get(doc);
  if (!query) {
    query = window.matchMedia("(prefers-color-scheme: dark)");
    listeners.set(doc, query);
  }
  const update = () => applyTheme("system", doc);
  query.addEventListener("change", update);
  cleanups.set(doc, () => query?.removeEventListener("change", update));
}

export function useDocumentTheme(theme: ThemeSetting) {
  useEffect(() => {
    applyTheme(theme);
    watchTheme(theme);
  }, [theme]);
}
