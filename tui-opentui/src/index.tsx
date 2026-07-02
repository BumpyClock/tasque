import { createCliRenderer } from "@opentui/core";
import { createRoot } from "@opentui/react";
import { App } from "./tui-app";
import { WatchApp } from "./watch-app";

const mode = process.env.TSQ_TUI_MODE?.trim();
const renderer = await createCliRenderer();
createRoot(renderer).render(mode === "watch" ? <WatchApp /> : <App />);
