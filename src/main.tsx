import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles/tokens.css";
import { windowKind } from "./lib/format";
import { AvatarWindow } from "./windows/AvatarWindow";
import { MenuWindow } from "./windows/MenuWindow";
import { OverlayWindow } from "./windows/OverlayWindow";
import { PanelWindow } from "./windows/PanelWindow";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Bernard's root element is missing.");
}

const kind = windowKind();
document.documentElement.dataset.window = kind;

createRoot(root).render(
  kind === "avatar" ? (
    <AvatarWindow />
  ) : kind === "menu" ? (
    <MenuWindow />
  ) : kind === "overlay" ? (
    <OverlayWindow />
  ) : (
    <PanelWindow />
  ),
);
