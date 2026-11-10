// Web 旁观 (Menu.java:4263-4325, BottomToolbar.java:1237-1289): starting and stopping the
// viewer page, copying its address and 强制结束试下. Rust runs the share and the 试下.
import { run, state } from "./app";
import { COMMANDS, type CommandEntry } from "./commands";
import { t } from "./i18n";

const url = () => state.view?.web_board.url ?? null;

Object.assign(COMMANDS, {
  // The item's text follows the share when the menu opens (menuSelected / popupMenuWillBecomeVisible).
  "menu.webBoardToggle": {
    run: () => void run({ type: "WebBoard", op: { type: url() ? "Stop" : "Start" } }),
    label: () => (url() ? t("Menu.webBoardStop") : t("Menu.webBoardStart")),
  },
  // Copies the address while the share runs; otherwise does nothing, like Java.
  "menu.webBoardCopyUrl": {
    run: () => {
      const address = url();
      if (address) void navigator.clipboard.writeText(address).catch(() => {});
    },
  },
  // Enabled only while a viewer's 试下 runs (BottomToolbar.java:1283-1288).
  "sync.exit-web-trial": {
    run: () => void run({ type: "WebBoard", op: { type: "ForceExitTrial" } }),
    enabled: () => !!url() && !!state.view?.web_board.trial_active,
  },
} satisfies Record<string, CommandEntry>);
