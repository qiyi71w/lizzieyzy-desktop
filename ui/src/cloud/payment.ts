// 智子充值与 VIP (Java ZhiziVipPurchaseDialog): a modal WeChat Native Pay flow. A choice is
// confirmed in its own dialog before exactly one order is created; a failed or uncertain creation is
// never retried automatically. A pending order is polled every 2 s (backing off to 10 s while the
// network fails), and a paid order is checked against the account up to six times, 2 s apart.
// Closing the dialog stops polling and verification and drops every late reply. The choice is a
// segmented 余额充值 / VIP 会员 over its amount or plan row; the order shows its QR code beside the
// details. The footer: 创建订单 and 取消 while choosing, 重新下单 (when offered) and 关闭 once an
// order exists.
import type { CloudCommand, MembershipProduct, PaymentOrder, PaymentPurpose, Purchase } from "../contract/generated";
import { ask, formPanel } from "../dialogs";
import { footer, row, segmented } from "../engine/controls";
import { format, product, t } from "../i18n";
import { qrSvg } from "./qr";
import { account, closeOwner, fen, months, requestOwner, TOP_UP_PRESETS_FEN, type AccountOutcome } from "./service";
import { button, el } from "./window";

const SETTLEMENT_ATTEMPTS = 6;
const SETTLEMENT_RETRY_MS = 2000;
const CANCELLED = { key: "cancelled", status: 0, retryAfterSeconds: 0, retryable: false, uncertain: false, cancelled: true, authenticationInvalidated: false, hostKey: null };

interface Choice {
  purpose: PaymentPurpose;
  amountFen: number;
  productName: string;
  displayName: string;
  purchase: Purchase;
}

type Flow = "selecting" | "creating" | "pending" | "success" | "failed" | "closed";

/** Resolves when the dialog is closed; `completed` runs once after a confirmed payment. */
export function openPayment(completed: () => void): Promise<void> {
  let flow: Flow = "selecting";
  let mode: "topUp" | "vip" = "topUp";
  let products: MembershipProduct[] = [];
  let choice: Choice | null = null;
  let order: PaymentOrder | null = null;
  const owner = requestOwner();
  let pollFailures = 0, notified = false;
  let pollTimer: number | undefined;

  /** A closed dialog's reply is dropped: every caller checks `flow` before using it. */
  const call = async (op: CloudCommand): Promise<AccountOutcome> =>
    (await account(op, owner)) ?? { failure: { ...CANCELLED } };
  const notify = () => { if (!notified) { notified = true; completed(); } };

  const root = el("div", "rc-payment-root");
  // The dialog's title is the frame's; the page opens with its subtitle.
  const heading = el("p", "rc-payment-subtitle", t("RemoteCompute.payment.subtitle"));

  // ---- choice -------------------------------------------------------------------------------
  const choiceCard = el("div", "rc-payment-choice");
  const modes = segmented("rc-payment-mode", [["topUp", t("RemoteCompute.payment.topUp")], ["vip", t("RemoteCompute.payment.vip")]], "topUp");
  modes.el.setAttribute("aria-label", t("RemoteCompute.payment.title"));
  const [topUpMode, vipMode] = modes.inputs;
  topUpMode.parentElement!.dataset.key = "payment.mode.topUp";
  vipMode.parentElement!.dataset.key = "payment.mode.vip";
  topUpMode.onchange = vipMode.onchange = () => selectMode(vipMode.checked ? "vip" : "topUp");
  const modeRow = el("div", "ds-row rc-payment-modes");
  modeRow.append(modes.el);

  const topUpPanel = el("div", "rc-payment-selection");
  const topUpSelect = el("select");
  topUpSelect.id = "rc-payment-topup";
  topUpSelect.dataset.key = "payment.topUp";
  topUpSelect.title = t("RemoteCompute.payment.topUpAmountDescription");
  TOP_UP_PRESETS_FEN.forEach((amount) => topUpSelect.append(new Option(fen(amount), String(amount))));
  row(topUpPanel, t("RemoteCompute.payment.topUpAmount"), "", topUpSelect);
  const topUpSummary = el("div", "rc-payment-summary");
  const topUpStatus = el("div", "rc-payment-status", t("RemoteCompute.payment.topUpNotice"));
  topUpPanel.append(topUpSummary, topUpStatus);

  const vipPanel = el("div", "rc-payment-selection");
  const productSelect = el("select");
  productSelect.id = "rc-payment-product";
  productSelect.dataset.key = "payment.product";
  productSelect.title = t("RemoteCompute.payment.planDescription");
  row(vipPanel, t("RemoteCompute.payment.plan"), "", productSelect);
  const productSummary = el("div", "rc-payment-summary");
  const productStatus = el("div", "rc-payment-status", t("RemoteCompute.payment.loadingProducts"));
  vipPanel.append(productSummary, productStatus);

  const cancelButton = button(t("RemoteCompute.payment.cancel"), "", "payment.cancel", () => void panel.close());
  const createButton = button(t("RemoteCompute.payment.createOrder"), "primary", "payment.create", () => void confirmAndCreate());
  createButton.title = t("RemoteCompute.payment.createOrderDescription");
  choiceCard.append(modeRow, topUpPanel, vipPanel);

  // ---- payment: the QR code (black on white with its quiet zone) beside the order ------------
  const paymentCard = el("div", "rc-payment-pay");
  const qr = el("div", "rc-payment-qr");
  qr.title = t("RemoteCompute.payment.qrDescription");
  const details = el("div", "rc-payment-details");
  const paymentProduct = el("div", "rc-payment-product");
  const paymentAmount = el("div", "rc-payment-amount");
  const paymentStatus = el("div", "rc-payment-status rc-payment-progress");
  paymentStatus.setAttribute("role", "status");
  paymentStatus.setAttribute("aria-live", "polite");
  let reauthenticate = false;
  const retryButton = button(t("RemoteCompute.payment.newOrder"), "", "payment.retry", () => {
    if (reauthenticate) void panel.close(); else resetForNewOrder();
  });
  retryButton.hidden = true;
  const closeButton = button(t("RemoteCompute.payment.close"), "", "payment.close", () => void panel.close());
  details.append(el("div", "rc-payment-scan", t("RemoteCompute.payment.scan")), paymentProduct, paymentAmount, paymentStatus,
    el("p", "rc-payment-provider", t("RemoteCompute.payment.providerNotice")));
  paymentCard.append(qr, details);
  paymentCard.hidden = true;
  root.append(heading, choiceCard, paymentCard);
  footer(root, { primary: createButton, others: [retryButton, closeButton], cancel: cancelButton });

  const setStatus = (node: HTMLElement, text: string, tone: "muted" | "good" | "error" = "muted") => {
    node.textContent = text;
    node.classList.toggle("rc-good", tone === "good");
    node.classList.toggle("rc-error", tone === "error");
  };

  function selectedChoice(): Choice | null {
    if (mode === "topUp") {
      const amount = Number(topUpSelect.value);
      if (!amount) return null;
      return { purpose: "BalanceTopUp", amountFen: amount, productName: "", displayName: t("RemoteCompute.payment.balanceTopUp"), purchase: { type: "TopUp", amount_fen: amount } };
    }
    const selected = products.find((p) => p.name === productSelect.value);
    if (!selected) return null;
    return { purpose: "VipMembership", amountFen: selected.priceFen, productName: selected.name, displayName: months(selected.durationMonths),
      purchase: { type: "Membership", product_name: selected.name, auto_renew: false } };
  }

  function update() {
    const selecting = flow === "selecting";
    // 创建订单 / 取消 while choosing; 关闭 (and 重新下单 when offered) once an order is shown.
    const ordering = !paymentCard.hidden;
    createButton.hidden = cancelButton.hidden = ordering;
    closeButton.hidden = !ordering;
    topUpPanel.hidden = mode !== "topUp";
    vipPanel.hidden = mode !== "vip";
    topUpMode.disabled = vipMode.disabled = !selecting;
    topUpSelect.disabled = !selecting;
    productSelect.disabled = !selecting || products.length === 0;
    createButton.disabled = !selecting || !selectedChoice();
    topUpSummary.textContent = format(t("RemoteCompute.payment.topUpSummary"), fen(Number(topUpSelect.value)));
    const selected = products.find((p) => p.name === productSelect.value);
    productSummary.textContent = selected ? format(t("RemoteCompute.payment.summary"), months(selected.durationMonths), fen(selected.priceFen)) : "\u00a0";
  }

  function selectMode(next: "topUp" | "vip") {
    if (flow !== "selecting") return;
    mode = next;
    update();
  }
  topUpSelect.onchange = productSelect.onchange = () => update();

  async function loadProducts() {
    const outcome = await call({ type: "Products" });
    if (flow === "closed") return;
    if (outcome.reply?.result.type === "Products") {
      products = outcome.reply.result.products;
      productSelect.replaceChildren(...products.map((p) => new Option(`${months(p.durationMonths)} · ${fen(p.priceFen)}`, p.name)));
      setStatus(productStatus, products.length ? t("RemoteCompute.payment.livePrice") : t("RemoteCompute.payment.noProducts"));
    } else {
      setStatus(productStatus, t("RemoteCompute.payment.productsFailed"), "error");
    }
    update();
  }

  async function confirmAndCreate() {
    const selected = selectedChoice();
    if (!selected || flow !== "selecting") return;
    const body = el("div", "rc-payment-confirm");
    body.append(
      el("strong", "", t("RemoteCompute.payment.confirmTitle")),
      el("p", "", selected.purpose === "BalanceTopUp" ? t("RemoteCompute.payment.confirmTopUp") : format(t("RemoteCompute.payment.confirmProduct"), selected.displayName)),
      el("p", "", format(t("RemoteCompute.payment.confirmAmount"), fen(selected.amountFen))),
      el("p", "", t("RemoteCompute.payment.confirmMethod")),
      el("p", "", t("RemoteCompute.payment.confirmProvider")),
    );
    if (!(await ask(t("RemoteCompute.payment.confirmTitle"), body, product("ok"), product("cancel")))) return;
    if (flow !== "selecting") return;
    choice = selected;
    order = null;
    flow = "creating";
    update();
    const status = selected.purpose === "BalanceTopUp" ? topUpStatus : productStatus;
    setStatus(status, t("RemoteCompute.payment.creating"));
    const outcome = await call({ type: "CreateOrder", purchase: selected.purchase, confirmed: true });
    if (flow !== "creating") return;
    const created = outcome.reply?.result.type === "Order" ? outcome.reply.result.order : null;
    if (!created || !matches(selected, created)) {
      flow = "selecting";
      choice = null;
      setStatus(status, t("RemoteCompute.payment.createFailed"), "error");
      update();
      return;
    }
    showOrder(created, selected);
  }

  function matches(expected: Choice, candidate: PaymentOrder): boolean {
    return candidate.amountFen === expected.amountFen && candidate.purpose === expected.purpose && candidate.productName === expected.productName;
  }

  function showOrder(created: PaymentOrder, selected: Choice) {
    order = created;
    flow = created.status === "Success" ? "success" : created.status === "Fail" ? "failed" : "pending";
    qr.replaceChildren();
    if (created.status === "Pending") qr.append(qrSvg(created.codeUrl, 276, t("RemoteCompute.payment.qrName")));
    paymentProduct.textContent = selected.displayName;
    paymentAmount.textContent = fen(created.amountFen);
    choiceCard.hidden = true;
    paymentCard.hidden = false;
    update();
    closeButton.focus();
    if (flow === "success") void finishSuccess();
    else if (flow === "failed") finishFailure();
    else {
      setStatus(paymentStatus, t("RemoteCompute.payment.waiting"));
      pollFailures = 0;
      schedulePoll(2000);
    }
  }

  function schedulePoll(delay: number) {
    clearTimeout(pollTimer);
    pollTimer = window.setTimeout(() => void poll(), delay);
  }

  async function poll() {
    const current = order;
    if (flow !== "pending" || !current) return;
    const outcome = await call({ type: "ReadOrder", id: current.id });
    if (flow !== "pending" || order?.id !== current.id) return;
    const updated = outcome.reply?.result.type === "Order" ? outcome.reply.result.order : null;
    if (updated) {
      if (updated.id.toLowerCase() !== current.id.toLowerCase() || !choice || !matches(choice, updated)) { stopForMismatch(); return; }
      order = updated;
      pollFailures = 0;
      if (updated.status === "Success") { flow = "success"; void finishSuccess(); }
      else if (updated.status === "Fail") { flow = "failed"; finishFailure(); }
      else { setStatus(paymentStatus, t("RemoteCompute.payment.waiting")); schedulePoll(2000); }
      return;
    }
    const failure = outcome.failure;
    if (!failure || failure.key === "invalid_response") { stopForMismatch(); return; }
    if (failure.cancelled) return;
    pollFailures++;
    setStatus(paymentStatus, t("RemoteCompute.payment.pollRetry"), "error");
    if (failure.authenticationInvalidated) {
      clearTimeout(pollTimer);
      reauthenticate = true;
      retryButton.textContent = t("RemoteCompute.payment.closeAndLogin");
      retryButton.hidden = false;
      return;
    }
    schedulePoll(Math.min(10000, 2000 * (pollFailures + 1)));
  }

  function stopForMismatch() {
    clearTimeout(pollTimer);
    setStatus(paymentStatus, t("RemoteCompute.payment.orderMismatch"), "error");
  }

  async function finishSuccess() {
    clearTimeout(pollTimer);
    setStatus(paymentStatus, t("RemoteCompute.payment.confirmedVerifying"), "good");
    retryButton.hidden = true;
    const paid = order;
    if (!paid) return;
    let settled = false;
    for (let attempt = 0; attempt < SETTLEMENT_ATTEMPTS; attempt++) {
      const outcome = await call({ type: "VerifyPayment", id: paid.id });
      if (flow === "closed") return;
      if (outcome.reply?.result.type === "PaymentVerification") {
        if (outcome.reply.result.verification.settled) { settled = true; break; }
      } else if (outcome.failure?.authenticationInvalidated || outcome.failure?.cancelled) break;
      if (attempt + 1 < SETTLEMENT_ATTEMPTS) {
        const pause = Promise.withResolvers<boolean>();
        setTimeout(() => pause.resolve(flow === "closed"), SETTLEMENT_RETRY_MS);
        if (await pause.promise) return;
      }
    }
    if (settled) {
      setStatus(paymentStatus, paid.purpose === "BalanceTopUp" ? t("RemoteCompute.payment.topUpVerified") : t("RemoteCompute.payment.vipVerified"), "good");
    } else {
      setStatus(paymentStatus, t("RemoteCompute.payment.settlementPending"), "error");
    }
    notify();
  }

  function finishFailure() {
    clearTimeout(pollTimer);
    setStatus(paymentStatus, t("RemoteCompute.payment.failed"), "error");
    reauthenticate = false;
    retryButton.textContent = t("RemoteCompute.payment.newOrder");
    retryButton.hidden = false;
  }

  function resetForNewOrder() {
    clearTimeout(pollTimer);
    reauthenticate = false;
    flow = "selecting";
    choice = null;
    order = null;
    qr.replaceChildren();
    retryButton.hidden = true;
    setStatus(topUpStatus, t("RemoteCompute.payment.topUpNotice"));
    setStatus(productStatus, t("RemoteCompute.payment.livePrice"));
    paymentCard.hidden = true;
    choiceCard.hidden = false;
    update();
  }

  const panel = formPanel(t("RemoteCompute.payment.title"), root, async () => {
    clearTimeout(pollTimer);
    if (flow === "success") notify();
    flow = "closed";
    choice = null;
    order = null;
    closeOwner(owner);
    return true;
  });
  root.closest(".modal-box")?.classList.add("rc-payment-dialog");
  update();
  void loadProducts();
  return panel.closed;
}
