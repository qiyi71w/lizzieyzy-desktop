// 综合设置's panel rows (ConfigDialog2.java:6745-6767): 显示胜率曲线 / 显示分支面板 (引擎与分析),
// 显示评论面板 (对局与操作) and the 棋盘与显示 rows edit one draft of the panel settings that
// 保存设置 commits once; 取消 discards it. One draft per opened dialog.
import { run, state } from "../app";
import type { PanelSettings } from "../contract/generated";
import { checkbox } from "../engine/controls";

type PanelFlag = { [K in keyof PanelSettings]: PanelSettings[K] extends boolean ? K : never }[keyof PanelSettings];

export class PanelDraft {
  private readonly rows = new Map<PanelFlag, HTMLInputElement>();

  /** A check box row tagged with its setting target; `disabled` greys it out like Java. */
  row(host: ParentNode, field: PanelFlag, target: string, title: string, sub: string, disabled = false): HTMLElement {
    const row = document.createElement("div");
    row.className = "settings-row";
    row.dataset.settingTarget = target;
    const box = checkbox(title, state.view?.settings.panels[field] ?? false);
    box.input.dataset.key = target;
    box.input.disabled = disabled;
    box.label.title = sub;
    const hint = document.createElement("p");
    hint.textContent = sub;
    row.append(box.label, hint);
    host.append(row);
    this.rows.set(field, box.input);
    return row;
  }

  /** Commits the edited rows over the current panel settings; true when nothing was refused. */
  async save(): Promise<boolean> {
    const now = state.view?.settings.panels;
    if (!now) return false;
    const next: PanelSettings = { ...now };
    for (const [field, input] of this.rows) next[field] = input.checked;
    if ([...this.rows.keys()].every((field) => next[field] === now[field])) return true;
    const result = await run({ type: "SetPanelSettings", settings: next });
    return !!result && result.type !== "Refused";
  }
}

let current = new PanelDraft();

/** Starts the draft of a newly opened 综合设置 dialog. */
export function beginPanelDraft(): PanelDraft {
  current = new PanelDraft();
  return current;
}

/** The open dialog's draft (section pages add their rows to it while being built). */
export function panelDraft(): PanelDraft {
  return current;
}
