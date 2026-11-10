// 智子账户明细 (Java ZhiziAccountDetailsDialog): a non-modal window with paged, read-only usage and
// funds tables, 20 records a page, each table with 上一页 / page / 下一页 / 刷新. Closing it drops
// the pages still loading.
import type { CloudFailure, CreditPage, UsagePage } from "../contract/generated";
import { format, t } from "../i18n";
import { account, closeOwner, duration, failureText, localTime, money, requestOwner } from "./service";
import { button, el, openCloudWindow } from "./window";

const PAGE_SIZE = 20;

interface Table {
  panel: HTMLElement;
  show(rows: string[][], page: number, total: number): void;
  busy(loading: boolean): void;
}

let focusOpen: ((tab: 0 | 1) => void) | null = null;

/** `lost` reports a failure that ended the sign-in, so the remote compute window can follow. */
export function openAccountDetails(initialTab: 0 | 1, lost: (failure: CloudFailure) => void): void {
  if (focusOpen) { focusOpen(initialTab); return; }
  const owner = requestOwner();
  const title = t("RemoteCompute.account.detailsTitle");
  const win = openCloudWindow(title, "rc-details", "remote-details", () => {
    focusOpen = null;
    closeOwner(owner);
  });
  const root = el("div", "rc-details-root");
  root.append(el("h2", "rc-details-title", title));
  const tabs = el("div", "rc-details-tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", title);
  tabs.title = t("RemoteCompute.account.detailsDescription");
  const status = el("div", "rc-details-status", "\u00a0");
  status.setAttribute("role", "status");
  status.setAttribute("aria-label", t("RemoteCompute.connectionStatus"));
  const report = (text: string, error: boolean) => { status.textContent = text; status.classList.toggle("rc-error", error); };

  function table(key: string, columns: string[], load: (page: number) => void): Table {
    const panel = el("div", "rc-details-panel");
    const scroll = el("div", "rc-details-scroll");
    const grid = el("table", "rc-details-table");
    grid.setAttribute("aria-label", title);
    const head = el("tr");
    const body = el("tbody");
    let rows: string[][] = [], sortColumn = -1, ascending = true, page = 0, total = 0;
    const render = () => {
      const sorted = sortColumn < 0 ? rows : [...rows].sort((a, b) => (ascending ? 1 : -1) * a[sortColumn].localeCompare(b[sortColumn]));
      body.replaceChildren(...sorted.map((row) => { const tr = el("tr"); tr.append(...row.map((cell) => el("td", "", cell))); return tr; }));
      [...head.children].forEach((th, i) => th.setAttribute("aria-sort", i !== sortColumn ? "none" : ascending ? "ascending" : "descending"));
    };
    columns.forEach((name, i) => {
      const th = el("th", "", name);
      th.tabIndex = 0;
      const sort = () => { ascending = sortColumn === i ? !ascending : true; sortColumn = i; render(); };
      th.onclick = sort;
      th.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); sort(); } };
      head.append(th);
    });
    const thead = el("thead");
    thead.append(head);
    grid.append(thead, body);
    scroll.append(grid);
    const previous = button(t("RemoteCompute.account.previous"), "rc-details-button", `${key}.previous`, () => load(page - 1));
    const label = el("span", "rc-details-page");
    const next = button(t("RemoteCompute.account.next"), "rc-details-button", `${key}.next`, () => load(page + 1));
    const refresh = button(t("RemoteCompute.account.refresh"), "rc-details-button", `${key}.refresh`, () => load(page));
    const controls = el("div", "rc-details-controls");
    controls.append(previous, label, next, refresh);
    panel.append(scroll, controls);
    const busy = (loading: boolean) => {
      previous.disabled = loading || page <= 0;
      next.disabled = loading || (page + 1) * PAGE_SIZE >= total;
      refresh.disabled = loading;
    };
    return {
      panel,
      show(items, at, count) {
        rows = items; page = at; total = count;
        label.textContent = format(t("RemoteCompute.account.page"), page + 1, total);
        render();
      },
      busy,
    };
  }

  const failed = (f: CloudFailure) => {
    if (f.cancelled) return;
    if (f.authenticationInvalidated) lost(f);
    report(failureText(f, ""), true);
  };

  let usageLoading = false, creditLoading = false;
  const usage = table("remote.usage", [t("RemoteCompute.account.time"), t("RemoteCompute.account.plan"), t("RemoteCompute.account.duration"), t("RemoteCompute.account.cost"), t("RemoteCompute.account.state")], (page) => void loadUsage(page));
  const credit = table("remote.credit", [t("RemoteCompute.account.time"), t("RemoteCompute.account.creditType"), t("RemoteCompute.account.amount"), t("RemoteCompute.account.source")], (page) => void loadCredit(page));

  async function loadUsage(requested: number) {
    if (usageLoading) return;
    usageLoading = true;
    usage.busy(true);
    report(t("RemoteCompute.account.loadingUsage"), false);
    const outcome = await account({ type: "Usages", page: Math.max(0, requested), page_size: PAGE_SIZE, finished: null }, owner);
    if (!outcome) return;
    usageLoading = false;
    if (outcome.reply?.result.type === "Usages") {
      const page: UsagePage = outcome.reply.result.page;
      usage.show(page.items.map((item) => {
        const kind = item.gpuType.trim() ? item.gpuType : item.engineType;
        return [
          localTime(item.startedAt, "dateTime"),
          item.vip ? t("RemoteCompute.account.vip") + (kind.trim() ? ` · ${kind}` : "") : kind,
          duration(item.durationSeconds),
          money(item.totalCostYuan),
          item.finished ? t("RemoteCompute.account.finished") : item.ready ? t("RemoteCompute.account.running") : t("RemoteCompute.account.preparing"),
        ];
      }), page.page, page.total);
      report(t("RemoteCompute.account.updated"), false);
    } else if (outcome.failure) failed(outcome.failure);
    usage.busy(false);
  }

  async function loadCredit(requested: number) {
    if (creditLoading) return;
    creditLoading = true;
    credit.busy(true);
    report(t("RemoteCompute.account.loadingCredits"), false);
    const outcome = await account({ type: "Credits", page: Math.max(0, requested), page_size: PAGE_SIZE, credit_type: null }, owner);
    if (!outcome) return;
    creditLoading = false;
    if (outcome.reply?.result.type === "Credits") {
      const page: CreditPage = outcome.reply.result.page;
      credit.show(page.items.map((item) => {
        let type = item.creditType;
        if (type === "CASH") type = t("RemoteCompute.account.creditCash");
        else if (type === "COUPON") type = t("RemoteCompute.account.creditCoupon");
        else if (type === "PURCHASE_PRODUCT") type = t("RemoteCompute.account.creditProduct");
        return [localTime(item.createdAt, "dateTime"), type, money(item.amountYuan), item.productName.trim() ? item.productName : item.source];
      }), page.page, page.total);
      report(t("RemoteCompute.account.updated"), false);
    } else if (outcome.failure) failed(outcome.failure);
    credit.busy(false);
  }

  const panels = [usage.panel, credit.panel];
  const tabButtons = [t("RemoteCompute.account.usageTab"), t("RemoteCompute.account.creditTab")].map((name, i) => {
    const tab = button(name, "rc-details-tab", `remote.details.tab${i}`, () => select(i as 0 | 1));
    tab.setAttribute("role", "tab");
    return tab;
  });
  const select = (tab: 0 | 1) => {
    panels.forEach((panel, i) => (panel.hidden = i !== tab));
    tabButtons.forEach((b, i) => { b.classList.toggle("selected", i === tab); b.setAttribute("aria-selected", String(i === tab)); });
  };
  tabs.append(...tabButtons);
  root.append(tabs, ...panels, status);
  win.body.append(root);
  select(initialTab);
  usage.show([], 0, 0);
  credit.show([], 0, 0);
  focusOpen = (tab) => { select(tab); win.frame.focus(); };
  void loadUsage(0);
  void loadCredit(0);
}
