// GtpConsolePane.java:85-177, 232-243, 293-319: non-modal, bounded colored protocol text.
import { run, state } from "../app";
import { engineConsole, onConsole } from "../contract/client";
import type { ConsoleLineView } from "../contract/generated";
import { closeBox, message } from "../dialogs";
import { t } from "../i18n";
import { actions, button, input } from "./controls";

let pane: HTMLElement | null = null;
let output: HTMLElement;
let sequence = 0;
let chars = 0;
let listening: Promise<unknown> | null = null;
let pending: ConsoleLineView[] | null = null;

export function consoleVisible(): boolean { return pane !== null && !pane.hidden; }

function append(lines: ConsoleLineView[]): void {
  for (const line of lines) {
    if (line.seq <= sequence) continue;
    sequence = line.seq;
    const text = (line.kind === "Sent" ? `${line.engine}> ${line.text}` : line.kind === "Output" ? ` ${line.text}` : line.text) + "\n";
    const span = document.createElement("span"); span.className = `gtp-line-${line.kind.toLowerCase()}`; span.textContent = text;
    output.append(span); chars += text.length;
  }
  if (chars > 30000) {
    let remove = chars - 15000;
    while (remove > 0 && output.firstChild) {
      const node = output.firstChild;
      const length = node.textContent?.length ?? 0;
      if (length <= remove) { node.remove(); remove -= length; chars -= length; }
      else { node.textContent = node.textContent!.slice(remove); chars -= remove; remove = 0; }
    }
  }
  output.scrollTop = output.scrollHeight;
}

/** Shows or hides the console; the core records what a hidden one misses by `always-gtp`. Hiding
 *  never stops the engine or clears the text. */
function show(visible: boolean): void {
  if (!pane) return;
  if (!visible && pane.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
  pane.hidden = !visible;
  void run({ type: "SetConsoleVisible", visible });
}

export async function toggleConsole(): Promise<void> {
  if (pane) { show(pane.hidden); if (!pane.hidden) pane.querySelector<HTMLInputElement>("input")?.focus(); return; }
  void run({ type: "SetConsoleVisible", visible: true });
  pane = document.createElement("section"); pane.className = "gtp-console"; pane.setAttribute("aria-label", t("GtpConsolePane.title"));
  // A window of its own in Java: usable while WaitForAnalysis disables the main window.
  pane.dataset.freeWindow = "";
  const header = document.createElement("div"); header.className = "gtp-header";
  const x = closeBox(() => show(false)); x.dataset.key = "gtp.close";
  header.append(t("GtpConsolePane.title"), x);
  // Escape hides it from any of its controls, the command field included (DESIGN.md Dialogs).
  pane.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault(); event.stopPropagation();
    show(false);
  }, true);
  // The protocol text is a dark console surface in either mode: its tokens are the dark set's.
  output = document.createElement("pre"); output.className = "gtp-output"; output.tabIndex = 0; output.dataset.theme = "dark";
  const command = input(""); command.setAttribute("aria-label", "GTP>");
  const send = async () => { const text = command.value.trim(); if (!text) return; command.value = ""; await run({ type: "SendGtpCommand", command: text }); command.focus(); };
  command.onkeydown = (event) => { event.stopPropagation(); if (event.key === "Enter") { event.preventDefault(); void send(); } };
  const commands = document.createElement("select"); commands.setAttribute("aria-label", t("GtpConsolePane.commands"));
  const fillCommands = () => {
    commands.replaceChildren(new Option(t("GtpConsolePane.commands"), ""));
    for (const text of state.view?.engine.foreground?.commands ?? []) commands.add(new Option(text, text));
  };
  commands.onfocus = fillCommands;
  commands.onchange = () => { command.value = commands.value; command.focus(); };
  fillCommands();
  pane.append(header, output);
  const prompt = document.createElement("label"); prompt.textContent = "GTP>"; prompt.append(command);
  actions(pane, prompt, button(t("GtpConsolePane.send"), send), button(t("GtpConsolePane.clear"), () => { output.replaceChildren(); chars = 0; }), commands);
  document.body.append(pane);
  // Subscribe before loading history; merge by sequence so no line is lost or duplicated.
  pending = [];
  try {
    listening ??= onConsole((lines) => { if (pending) pending.push(...lines); else append(lines); });
    await listening;
    const history = await engineConsole(0);
    const buffered = pending ?? []; pending = null;
    append([...history, ...buffered].sort((a, b) => a.seq - b.seq));
  } catch (error) {
    pending = null;
    await message(t("GtpConsolePane.title"), String(error), "error");
  }
  command.focus();
}
