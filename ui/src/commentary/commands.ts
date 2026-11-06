// AI 解说 entries (分析 > AI 解说 and the menu strip button, action id analysis.commentary;
// TeacherDialog.show), the relay to its window and the comment panel's AI section
// (CommentDisplayRenderer). The core owns the dialog state; the window shows it.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { state, subscribe } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { WorkbenchView } from "../contract/generated";
import { language, t } from "../i18n";
import { markdownToHtml, plainTextToHtml } from "./markdown";
import {
  COMMENTARY_DATA_EVENT,
  COMMENTARY_FOCUS_EVENT,
  COMMENTARY_INPUT_EVENT,
  COMMENTARY_LABEL,
  type CommentaryInput,
  type CommentaryPayload,
  type CommentaryReport,
} from "./model";
import "./panel.css";

/** The window exists (created by `open_commentary`); `seenOpen` once the core has opened the
 *  dialog, so its closing (the adapter sends Close when the window goes) ends the relay. */
let shown = false;
let seenOpen = false;
let opening = false;
let lastSent = "";
let report: CommentaryReport | null = null;

/** TeacherDialog.show: one window; using the entry again raises it and focuses 开始解说. */
export async function openCommentary(): Promise<void> {
  if (opening) return;
  opening = true;
  const again = shown;
  try {
    await invoke("open_commentary", { title: t("Teacher.title") });
    shown = true;
    if (again) void emitTo(COMMENTARY_LABEL, COMMENTARY_FOCUS_EVENT);
    else {
      lastSent = "";
      if (state.view) relayCommentary(state.view);
    }
  } finally {
    opening = false;
  }
}

/** Sends the window its part of the view; called with every main-window render. */
export function relayCommentary(v: WorkbenchView): void {
  if (!shown) return;
  if (v.commentary.open) seenOpen = true;
  else if (seenOpen) {
    shown = false;
    seenOpen = false;
    report = null;
    return;
  }
  const payload: CommentaryPayload = { language: language(), document: v.document.id, view: v.commentary };
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(COMMENTARY_LABEL, COMMENTARY_DATA_EVENT, payload);
}

export async function startCommentary(): Promise<void> {
  await listen<CommentaryInput>(COMMENTARY_INPUT_EVENT, (e) => {
    if (e.payload.type === "ready") {
      shown = true;
      lastSent = "";
      if (state.view) relayCommentary(state.view);
    } else report = e.payload.report;
  });
}

/** What the acceptance probe reports: whether the window is open and what it last showed. */
export function commentaryReport() {
  return { open: shown, window: shown ? report : null };
}

/** The menu strip's AI 解说 button (Menu.aiCommentaryButton): secondary style, the short label
 *  when the strip is narrower than 900 px (refreshAiFeatureLabels). */
export function buildCommentaryButton(): HTMLElement {
  const pill = document.createElement("span");
  pill.className = "pill commentary";
  pill.dataset.key = "toolbar.aiCommentary";
  pill.dataset.function = "analysis.commentary";
  pill.title = t("Menu.aiCommentary");
  pill.setAttribute("role", "button");
  pill.setAttribute("aria-label", t("Menu.aiCommentary"));
  pill.setAttribute("aria-description", t("Teacher.subtitle"));
  pill.addEventListener("mousedown", (e) => e.preventDefault());
  pill.onclick = () => void openCommentary();
  const strip = document.getElementById("menubar");
  const update = () => {
    const width = strip?.clientWidth ?? 0;
    const text = width > 0 && width < 900 ? t("Menu.aiCommentary.short") : t("Menu.aiCommentary");
    if (pill.textContent !== text) pill.textContent = text;
  };
  subscribe(update);
  if (strip) new ResizeObserver(update).observe(strip);
  update();
  return pill;
}

/** CommentDisplayRenderer.render: the SGF comment as escaped plain text, then the stored AI
 *  commentary under the Teacher.title heading with the safe Markdown subset. */
export function renderCommentPanel(host: HTMLElement, comment: string, ai: string | null): void {
  let html = "";
  if (comment) html += `<div class="sgf-comment">${plainTextToHtml(comment)}</div>`;
  if (ai !== null) {
    if (comment) html += `<div class="comment-spacer">&nbsp;</div>`;
    const title = document.createElement("strong");
    title.textContent = t("Teacher.title");
    html += `<div class="ai-commentary"><div class="ai-commentary-title">${title.outerHTML}</div>${markdownToHtml(ai)}</div>`;
  }
  host.innerHTML = html;
}

Object.assign(COMMANDS, {
  // 分析 > AI 解说 and the menu strip button share the Java action id (Menu.java:6740, 6874).
  "analysis.commentary": { run: () => void openCommentary() },
} satisfies Record<string, CommandEntry>);
