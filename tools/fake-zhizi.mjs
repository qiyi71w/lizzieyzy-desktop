#!/usr/bin/env node
// Local fake of the 智子 (Zhizi) cloud-compute service, for acceptance runs only. Nothing here talks
// to the real service and nothing is charged: accounts, orders and payments live in memory.
//
//   node tools/fake-zhizi.mjs [--port 18080] [--host 127.0.0.1] [--namespace /zhizi-fixture]
//     [--scenario ok] [--account vip] [--ready-delay-ms 1000] [--engine "<cmd>"] [--pay-after N]
//     [--ssh-port 1] [--latency-ms 0] [--stdin-log <file>]
//
// Point an acceptance build at it with LIZZIEYZY_TAURI_ZHIZI_BASE=http://127.0.0.1:18080 (plain
// http is accepted only for 127.0.0.1, ::1 and localhost). Sign in with any phone/email and the
// password `fixture-password`, or code login with `123456`.
//
// Scenarios (--scenario), for the cloud engine start:
//   ok           namespace CONNECT accepted, "Loading model" stderr, then `ready`; stdin goes to
//                --engine (a GTP child process per socket) or to a built-in minimal KataGo stand-in
//   not-ready    namespace CONNECT accepted, `ready` is never sent
//   refused      namespace CONNECT answered with CONNECT_ERROR "该账号未开通 VIP 包月"
//   disconnect   the WebSocket is closed right after the namespace CONNECT
//   token-error  fetch-socketio-token fails (500, key failed_to_insert_socket_io_token)
// Accounts (--account): vip, expired, none, rich (very long numbers), empty (no usages/credits).
// Orders (POST /api/pay/orders) return a fake weixin:// QR payload and stay PENDING; with
// --pay-after N an order turns SUCCESS on its Nth status read and the account is credited.
//
// Limitation: the model catalog refresh ends in an SSH `query-server` call. This fake serves the
// HTTP part (connectAccount, /world.json, /users/<name>.ssh.json) but runs no SSH server; the
// endpoint points at 127.0.0.1:--ssh-port (default 1, where nothing listens), so 刷新模型 fails
// fast with catalog_unavailable instead of reaching the real service. The build must also read
// world.json from this base.
//
// For protocol measurements, --latency-ms delays the engine's output to the client by that many
// milliseconds (in order), like a network between them; when the --engine process exits, the
// close follows its last output. --stdin-log appends every engine
// input line, with the time it arrived, to a JSON-lines file ({"t": epoch ms, "line": "..."}).
//
// Only Node built-ins are used; runs on Linux and Windows with Node 20+.

import http from "node:http";
import crypto from "node:crypto";
import fs from "node:fs";
import { spawn } from "node:child_process";
import { URL } from "node:url";

const SCENARIOS = ["ok", "not-ready", "refused", "disconnect", "token-error"];
const ACCOUNTS = ["vip", "expired", "none", "rich", "empty"];
const FIXTURE_PASSWORD = "fixture-password";
const FIXTURE_CODE = "123456";
const PING_INTERVAL = 25000;
const PING_TIMEOUT = 20000;
const MAX_MESSAGE = 16 * 1024 * 1024;
const MAX_BODY = 1024 * 1024;
const DAY = 86400000;
const PRODUCTS = [
  { type: "MEMBERSHIP", name: "MEMBERSHIP_1_MONTH", price: 3000, months: 1 },
  { type: "MEMBERSHIP", name: "MEMBERSHIP_3_MONTH", price: 8000, months: 3 },
  { type: "MEMBERSHIP", name: "MEMBERSHIP_6_MONTH", price: 15000, months: 6 },
  { type: "MEMBERSHIP", name: "MEMBERSHIP_12_MONTH", price: 28000, months: 12 },
];
const CATALOG_CONNECT_USER = "fixture-connect";
const CATALOG_CONNECT_PASSWORD = crypto.randomBytes(12).toString("hex");

// ---------------------------------------------------------------------------------------------
// Command line

function usage(message) {
  if (message) console.error(`fake-zhizi: ${message}`);
  console.error("usage: node tools/fake-zhizi.mjs [--port 18080] [--host 127.0.0.1] [--namespace /zhizi-fixture]\n"
    + `  [--scenario ${SCENARIOS.join("|")}] [--account ${ACCOUNTS.join("|")}] [--ready-delay-ms 1000]\n`
    + "  [--engine \"<command line>\"] [--pay-after N] [--ssh-port 1] [--latency-ms 0] [--stdin-log <file>]");
  process.exit(2);
}

function parseArgs(argv) {
  const opts = { port: 18080, host: "127.0.0.1", namespace: "/zhizi-fixture", scenario: "ok", account: "vip",
    readyDelayMs: 1000, engine: null, payAfter: null, sshPort: 1, latencyMs: 0, stdinLog: null };
  const integer = (name, value, min) => {
    if (!/^\d+$/.test(value ?? "") || Number(value) < min) usage(`${name} needs an integer >= ${min}`);
    return Number(value);
  };
  for (let i = 0; i < argv.length; i++) {
    let [name, value] = argv[i].split(/=(.*)/s, 2);
    if (name === "--help" || name === "-h") usage();
    if (value === undefined) value = argv[++i];
    if (value === undefined) usage(`${name} needs a value`);
    switch (name) {
      case "--port": opts.port = integer(name, value, 0); break;
      case "--host": opts.host = value; break;
      case "--namespace": opts.namespace = value.startsWith("/") ? value : `/${value}`; break;
      case "--scenario": if (!SCENARIOS.includes(value)) usage(`unknown scenario ${value}`); opts.scenario = value; break;
      case "--account": if (!ACCOUNTS.includes(value)) usage(`unknown account ${value}`); opts.account = value; break;
      case "--ready-delay-ms": opts.readyDelayMs = integer(name, value, 0); break;
      case "--engine": opts.engine = splitCommandLine(value); if (!opts.engine.length) usage("--engine is empty"); break;
      case "--pay-after": opts.payAfter = integer(name, value, 0); break;
      case "--ssh-port": opts.sshPort = integer(name, value, 1); break;
      case "--latency-ms": opts.latencyMs = integer(name, value, 0); break;
      case "--stdin-log": opts.stdinLog = value; break;
      default: usage(`unknown option ${name}`);
    }
  }
  if (opts.port > 65535 || opts.sshPort > 65535) usage("port out of range");
  if (opts.namespace.includes(",")) usage("--namespace must not contain ','");
  return opts;
}

// Quotes ("..." or '...') group words; backslashes stay literal so Windows paths work unquoted.
function splitCommandLine(line) {
  const words = [];
  let word = "", quoted = false, quote = null;
  for (const ch of line) {
    if (quote) { if (ch === quote) quote = null; else word += ch; }
    else if (ch === '"' || ch === "'") { quote = ch; quoted = true; }
    else if (/\s/.test(ch)) { if (word || quoted) words.push(word); word = ""; quoted = false; }
    else word += ch;
  }
  if (quote) usage("--engine has an unterminated quote");
  if (word || quoted) words.push(word);
  return words;
}

const opts = parseArgs(process.argv.slice(2));
const log = (line) => console.log(line);

// ---------------------------------------------------------------------------------------------
// Account data

const iso = (ms) => new Date(ms).toISOString();
// Exact decimal text for an integer count of 10^-decimals units.
function decimal(units, decimals) {
  const sign = units < 0 ? "-" : "";
  const text = String(Math.abs(units)).padStart(decimals + 1, "0");
  return `${sign}${text.slice(0, text.length - decimals)}.${text.slice(text.length - decimals)}`;
}
const yuan = (fen) => decimal(fen, 2);
function addMonths(ms, months) {
  const date = new Date(ms);
  date.setUTCMonth(date.getUTCMonth() + months);
  return date.getTime();
}

function seedAccount(identifier) {
  const now = Date.now();
  const variant = opts.account;
  const rich = variant === "rich";
  const profile = {
    vip: { isMembership: true, membershipExpiresAt: now + 38 * DAY, membershipAutoRenew: false },
    expired: { isMembership: false, membershipExpiresAt: now - 20 * DAY, membershipAutoRenew: false },
    none: { isMembership: false, membershipExpiresAt: null, membershipAutoRenew: null },
    rich: { isMembership: true, membershipExpiresAt: now + 3650 * DAY, membershipAutoRenew: true },
    empty: { isMembership: false, membershipExpiresAt: null, membershipAutoRenew: null },
  }[variant];
  const balance = {
    vip: { remaining: 4250, totalCash: 10000, yesterday: 125, consumption: 5750, duration: 3 * 3600 + 1234, share24: 1800, vip24: 5400, nodes: 8 },
    expired: { remaining: 380, totalCash: 3000, yesterday: 0, consumption: 2620, duration: 41 * 3600 + 17, share24: 0, vip24: 0, nodes: 8 },
    none: { remaining: 1000, totalCash: 1000, yesterday: 48, consumption: 0, duration: 1800, share24: 1800, vip24: 0, nodes: 8 },
    rich: { remaining: 12345678999, totalCash: 99999999999, yesterday: 8765432, consumption: 98765432101, duration: 987654321, share24: 86399, vip24: 86400, nodes: 1234567 },
    empty: { remaining: 0, totalCash: 0, yesterday: 0, consumption: 0, duration: 0, share24: 0, vip24: 0, nodes: 8 },
  }[variant];
  const usages = [];
  const credits = [];
  if (variant !== "empty") {
    let started = now - 2 * 3600 * 1000;
    for (let i = 0; i < 45; i++) {
      const vipRow = (variant === "vip" || rich) && i % 3 !== 2;
      const duration = rich ? 12345678 + i * 100003 : 600 + ((i * 977) % 7200);
      // 1.60 yuan per hour for shared rows, in 1/10000 yuan.
      const cost = rich ? 987654321234 + i * 1111111 : vipRow ? 0 : Math.round(duration * 16000 / 3600);
      started -= rich ? (duration + 3600) * 1000 : (i * 7 + 5) * 3600 * 1000;
      usages.push({ startedAt: iso(started), endedAt: iso(started + duration * 1000), finished: true, ready: true,
        duration, totalCost: decimal(cost, 4), gpuType: vipRow ? "vip-share" : "share", engineType: "go", vip: vipRow, share: true });
    }
    credits.push(
      { creditType: "CASH", amount: rich ? "99999999.99" : "30.00", source: "PAYMENT", productName: "", createdAt: iso(now - 2 * DAY) },
      { creditType: "CASH", amount: rich ? "88888888.88" : "10.00", source: "PAYMENT", productName: "", createdAt: iso(now - 15 * DAY) },
      { creditType: "COUPON", amount: rich ? "7777777.77" : "5.00", source: "COUPON", productName: "", createdAt: iso(now - 20 * DAY) },
    );
    if (variant !== "none") {
      credits.push({ creditType: "PURCHASE_PRODUCT", amount: rich ? "28000.00" : "30.00", source: "PAYMENT",
        productName: rich ? "MEMBERSHIP_12_MONTH" : "MEMBERSHIP_1_MONTH", createdAt: iso(now - 45 * DAY) });
    }
  }
  return { identifier, password: FIXTURE_PASSWORD, profile, balance, usages, credits, sockets: 0 };
}

const accounts = new Map(); // lower-cased identifier -> account
const accountTokens = new Map(); // bearer token -> account
const socketTokens = new Map(); // zz-socketio-token -> { account, args }
const orders = new Map(); // order id -> order

function account(identifier) {
  const key = identifier.trim().toLowerCase();
  if (!accounts.has(key)) accounts.set(key, seedAccount(identifier.trim()));
  return accounts.get(key);
}

function profileJson(a) {
  const body = { id: crypto.createHash("sha1").update(a.identifier.toLowerCase()).digest("hex").slice(0, 24) };
  body[a.identifier.includes("@") ? "email" : "phone"] = a.identifier;
  body.isMembership = a.profile.isMembership;
  if (a.profile.membershipExpiresAt !== null) body.membershipExpiresAt = iso(a.profile.membershipExpiresAt);
  if (a.profile.membershipAutoRenew !== null) body.membershipAutoRenew = a.profile.membershipAutoRenew;
  return body;
}

function balanceJson(a) {
  const b = a.balance;
  return { remainingBalance: yuan(b.remaining), totalCashAmount: yuan(b.totalCash), yesterdayConsumption: yuan(b.yesterday),
    totalConsumption: yuan(b.consumption), totalDuration: b.duration, last24HrsShareDuration: b.share24,
    last24HrsVIPShareDuration: b.vip24, currentNumOfMyConnections: a.sockets, currentNumOfNodes: b.nodes };
}

function page(items, query) {
  const pageNumber = Math.max(0, Number.parseInt(query.get("page") ?? "0", 10) || 0);
  const pageSize = Math.min(100, Math.max(1, Number.parseInt(query.get("pageSize") ?? "20", 10) || 20));
  return { total: items.length, page: pageNumber, pageSize, items: items.slice(pageNumber * pageSize, (pageNumber + 1) * pageSize) };
}

function orderJson(o) {
  const body = { id: o.id, userId: o.userId, amount: o.amount, paidStatus: o.status, createdAt: iso(o.createdAt), updatedAt: iso(o.updatedAt),
    nativePayRequest: { codeURL: o.codeURL } };
  if (o.product) { body.orderType = "PURCHASE_PRODUCT"; body.productName = o.product.name; }
  if (o.paidAt) body.paidAt = iso(o.paidAt);
  return body;
}

function settle(o) {
  const a = o.account;
  const now = Date.now();
  o.status = "SUCCESS"; o.paidAt = now; o.updatedAt = now;
  if (o.product) {
    const current = a.profile.isMembership && a.profile.membershipExpiresAt > now ? a.profile.membershipExpiresAt : now;
    a.profile.isMembership = true;
    a.profile.membershipExpiresAt = addMonths(current, o.product.months);
    a.profile.membershipAutoRenew = o.autoRenew;
    a.credits.unshift({ creditType: "PURCHASE_PRODUCT", amount: yuan(o.amount), source: "PAYMENT", productName: o.product.name, createdAt: iso(now) });
  } else {
    a.balance.remaining += o.amount;
    a.balance.totalCash += o.amount;
    a.credits.unshift({ creditType: "CASH", amount: yuan(o.amount), source: "PAYMENT", productName: "", createdAt: iso(now) });
  }
  log(`ORDER ${o.id} paid`);
}

// ---------------------------------------------------------------------------------------------
// HTTP

class Failure {
  constructor(status, key) { this.status = status; this.key = key; }
}
// The service's error body: {"statusCode": <status>, "key": "<key>"}.
const fail = (status, key) => { throw new Failure(status, key); };

function bearer(req) {
  const match = /^Bearer\s+(\S+)$/i.exec(req.headers.authorization ?? "");
  const a = match && accountTokens.get(match[1]);
  if (!a) fail(401, "unauthorized");
  return a;
}

function identifierOf(body) {
  const value = typeof body.email === "string" ? body.email : typeof body.phone === "string" ? body.phone : "";
  if (!value.trim()) fail(400, typeof body.email === "string" ? "invalid_email" : "invalid_phone");
  return value;
}

function issueAccountToken(a) {
  const token = crypto.randomBytes(24).toString("hex");
  accountTokens.set(token, a);
  return { token };
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY) { reject(new Failure(413, "invalid_request")); req.destroy(); return; }
      chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function json(text) {
  if (!text.trim()) return {};
  try {
    const value = JSON.parse(text);
    if (value && typeof value === "object" && !Array.isArray(value)) return value;
  } catch { /* falls through */ }
  return fail(400, "invalid_request");
}

function requestHost(req) {
  return req.headers.host ?? `${opts.host.includes(":") ? `[${opts.host}]` : opts.host}:${server.address().port}`;
}

async function route(req, url) {
  const path = url.pathname;
  const method = req.method;
  const query = url.searchParams;
  const post = async () => json(await readBody(req));

  if (method === "POST" && path === "/api/cluster/account/login") {
    const body = await post();
    const a = account(identifierOf(body));
    if (body.password !== a.password) fail(401, "invalid_credentials");
    return issueAccountToken(a);
  }
  if (method === "POST" && path === "/api/cluster/account/fast-login") {
    const body = await post();
    const a = account(identifierOf(body));
    if (String(body.verificationCode ?? "").trim() !== FIXTURE_CODE) fail(401, "invalid_verification_code");
    return issueAccountToken(a);
  }
  if (method === "POST" && path === "/api/cluster/account/send-code") {
    const body = await post();
    identifierOf(body);
    if (!["fast_login", "reset_password"].includes(body.type)) fail(400, "send_code_error");
    return {};
  }
  if (method === "POST" && path === "/api/cluster/account/reset-password") {
    const body = await post();
    const a = account(identifierOf(body));
    if (String(body.verificationCode ?? "").trim() !== FIXTURE_CODE) fail(401, "invalid_verification_code");
    if (typeof body.password !== "string" || body.password.length < 8) fail(400, "password_too_short");
    a.password = body.password;
    return issueAccountToken(a);
  }
  if (method === "POST" && path === "/api/cluster/account/fetch-socketio-token") {
    const a = bearer(req);
    const body = await post();
    if (typeof body.args !== "string" || !body.args.trim()) fail(400, "invalid_request");
    if (opts.scenario === "token-error") fail(500, "failed_to_insert_socket_io_token");
    const token = crypto.randomBytes(24).toString("hex");
    socketTokens.set(token, { account: a, args: body.args });
    return { token, socketIOURL: `http://${requestHost(req)}${opts.namespace}` };
  }
  if (method === "GET" && path === "/api/cluster/account/connectAccount/fetch") {
    bearer(req);
    return { connectUsername: CATALOG_CONNECT_USER, connectPassword: CATALOG_CONNECT_PASSWORD };
  }
  if (method === "GET" && path === "/api/cluster/account/me") return profileJson(bearer(req));
  if (method === "GET" && path === "/api/cluster/balance") return balanceJson(bearer(req));
  if (method === "GET" && path === "/api/cluster/usage/my-usages") {
    const a = bearer(req);
    const finished = query.get("finished");
    return page(finished === null ? a.usages : a.usages.filter((u) => String(u.finished) === finished), query);
  }
  if (method === "GET" && path === "/api/cluster/credit/my-credits") {
    const a = bearer(req);
    const type = query.get("creditType")?.trim();
    return page(type ? a.credits.filter((c) => c.creditType === type) : a.credits, query);
  }
  if (method === "GET" && path === "/api/cluster/product") {
    const type = query.get("type");
    return PRODUCTS.filter((p) => !type || p.type === type).map(({ type: t, name, price }) => ({ type: t, name, price }));
  }
  if (method === "POST" && path === "/api/pay/orders") {
    const a = bearer(req);
    const body = await post();
    if (body.payType !== "WECHAT" || body.tradeType !== "NATIVE") fail(400, "invalid_pay_type");
    if (!Number.isSafeInteger(body.amount) || body.amount <= 0) fail(400, "invalid_amount");
    let product = null;
    if (body.orderType === "PURCHASE_PRODUCT") {
      if (typeof body.productName !== "string" || !body.productName) fail(400, "missing_product_name");
      product = PRODUCTS.find((p) => p.name === body.productName) ?? fail(400, "invalid_product_name");
      if (body.amount !== product.price) fail(400, "invalid_amount");
    } else if (body.orderType !== undefined) fail(400, "invalid_product");
    const now = Date.now();
    const id = crypto.randomBytes(12).toString("hex");
    const o = { id, userId: profileJson(a).id, account: a, amount: body.amount, product, autoRenew: body.extraInfo?.autoRenew === true,
      status: "PENDING", polls: 0, createdAt: now, updatedAt: now, paidAt: null,
      codeURL: `weixin://wxpay/bizpayurl?pr=fixture${crypto.randomBytes(8).toString("hex")}` };
    orders.set(id, o);
    log(`ORDER ${id} created ${product ? product.name : "top-up"} ${yuan(o.amount)} CNY (fake, never charged)`);
    return orderJson(o);
  }
  const orderPath = /^\/api\/pay\/orders\/([^/]+)$/.exec(path);
  if (method === "GET" && orderPath) {
    const a = bearer(req);
    const o = orders.get(orderPath[1].toLowerCase());
    if (!o || o.account !== a) fail(404, "order_not_found");
    o.polls++;
    if (o.status === "PENDING" && opts.payAfter !== null && o.polls >= opts.payAfter) settle(o);
    return orderJson(o);
  }
  if (method === "GET" && path === "/world.json") {
    return { platforms: [{ name: "all", http: { getUrl: `http://${requestHost(req)}` } }] };
  }
  const sshPath = /^\/users\/(.+)\.ssh\.json$/.exec(path);
  if (method === "GET" && sshPath) {
    let name;
    try { name = decodeURIComponent(sshPath[1]); } catch { name = ""; }
    if (name !== CATALOG_CONNECT_USER) fail(404, "not_found");
    return { host: "127.0.0.1", port: opts.sshPort, user: CATALOG_CONNECT_USER };
  }
  return fail(404, "not_found");
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", "http://fixture.invalid");
  let status = 200, body;
  try {
    body = await route(req, url);
  } catch (error) {
    if (!(error instanceof Failure)) { console.error(error); error = new Failure(500, "internal_error"); }
    status = error.status;
    body = { statusCode: error.status, key: error.key };
  }
  const text = JSON.stringify(body);
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Content-Length": Buffer.byteLength(text) });
  res.end(text);
  log(`${req.method} ${url.pathname}${url.search} -> ${status}`);
});
server.on("clientError", (_error, socket) => { if (socket.writable) socket.end("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n"); });

// ---------------------------------------------------------------------------------------------
// WebSocket (RFC 6455) + Engine.IO v4 + Socket.IO v5

function rejectUpgrade(socket, status, reason, key) {
  const text = JSON.stringify({ statusCode: status, key });
  socket.end(`HTTP/1.1 ${status} ${reason}\r\nContent-Type: application/json\r\nContent-Length: ${Buffer.byteLength(text)}\r\nConnection: close\r\n\r\n${text}`);
  log(`GET /socket.io.v4/ (websocket) -> ${status}`);
}

server.on("upgrade", (req, socket, head) => {
  const url = new URL(req.url ?? "/", "http://fixture.invalid");
  const key = req.headers["sec-websocket-key"];
  if (req.method !== "GET" || url.pathname !== "/socket.io.v4/" || url.searchParams.get("EIO") !== "4"
    || url.searchParams.get("transport") !== "websocket" || (req.headers.upgrade ?? "").toLowerCase() !== "websocket"
    || req.headers["sec-websocket-version"] !== "13" || !key) {
    rejectUpgrade(socket, 400, "Bad Request", "invalid_request");
    return;
  }
  const grant = socketTokens.get(url.searchParams.get("zz-socketio-token") ?? "");
  if (!grant) { rejectUpgrade(socket, 403, "Forbidden", "unauthorized"); return; }
  const accept = crypto.createHash("sha1").update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest("base64");
  // No Sec-WebSocket-Protocol / Sec-WebSocket-Extensions: the client rejects either.
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  log("GET /socket.io.v4/ (websocket) -> 101");
  new EngineSocket(socket, head, grant);
});

let nextSocketId = 1;
const children = new Set(); // running --engine processes, killed when the fake exits

class EngineSocket {
  constructor(socket, head, grant) {
    this.id = nextSocketId++;
    this.socket = socket;
    this.grant = grant;
    this.buffer = Buffer.alloc(0);
    this.fragments = [];
    this.fragmentOpcode = null;
    this.fragmentSize = 0;
    this.closed = false;
    this.closeSent = false;
    this.namespace = null; // the configured namespace once connected
    this.engine = null;
    this.pending = [];
    this.timers = new Set();
    this.usage = null;
    grant.account.sockets++;
    socket.setNoDelay(true);
    socket.on("data", (chunk) => this.receive(chunk));
    socket.on("close", () => this.finish());
    // The HTTP server keeps upgraded sockets half-open: a client that closes its side ends the
    // session at once, as the service does, rather than at the next heartbeat timeout.
    socket.on("end", () => { log(`WS ${this.id} closed by the client`); socket.destroy(); });
    socket.on("error", () => socket.destroy());
    log(`WS ${this.id} connect`);
    this.text(`0${JSON.stringify({ sid: randomId(), upgrades: [], pingInterval: PING_INTERVAL, pingTimeout: PING_TIMEOUT })}`);
    this.pingTimer = setInterval(() => {
      this.text("2");
      clearTimeout(this.pongTimer);
      this.pongTimer = setTimeout(() => { log(`WS ${this.id} ping timeout`); this.close(1000); }, PING_TIMEOUT);
    }, PING_INTERVAL);
    if (head.length) this.receive(head);
  }

  later(ms, action) {
    if (this.closed) return;
    const timer = setTimeout(() => { this.timers.delete(timer); if (!this.closed) action(); }, ms);
    this.timers.add(timer);
  }

  frame(opcode, payload) {
    if (this.closed || this.closeSent || !this.socket.writable) return;
    const length = payload.length;
    let header;
    if (length < 126) header = Buffer.from([0x80 | opcode, length]);
    else if (length < 65536) header = Buffer.from([0x80 | opcode, 126, length >> 8, length & 0xff]);
    else {
      header = Buffer.alloc(10);
      header[0] = 0x80 | opcode;
      header[1] = 127;
      header.writeBigUInt64BE(BigInt(length), 2);
    }
    this.socket.write(Buffer.concat([header, payload]));
  }

  text(message) { this.frame(0x1, Buffer.from(message, "utf8")); }

  close(code, reason = "") {
    if (this.closeSent || this.closed) return;
    const payload = Buffer.alloc(2 + Buffer.byteLength(reason));
    payload.writeUInt16BE(code, 0);
    payload.write(reason, 2);
    this.frame(0x8, payload);
    this.closeSent = true;
    this.socket.end();
    setTimeout(() => this.socket.destroy(), 2000).unref();
  }

  receive(chunk) {
    this.buffer = this.buffer.length ? Buffer.concat([this.buffer, chunk]) : chunk;
    while (!this.closed && !this.closeSent) {
      const buffer = this.buffer;
      if (buffer.length < 2) return;
      const fin = (buffer[0] & 0x80) !== 0;
      const opcode = buffer[0] & 0x0f;
      if (buffer[0] & 0x70) return this.close(1002);
      if (!(buffer[1] & 0x80)) return this.close(1002); // client frames must be masked
      let length = buffer[1] & 0x7f;
      let offset = 2;
      if (length === 126) {
        if (buffer.length < 4) return;
        length = buffer.readUInt16BE(2); offset = 4;
      } else if (length === 127) {
        if (buffer.length < 10) return;
        const big = buffer.readBigUInt64BE(2);
        if (big > BigInt(MAX_MESSAGE)) return this.close(1009);
        length = Number(big); offset = 10;
      }
      if (length > MAX_MESSAGE) return this.close(1009);
      if (buffer.length < offset + 4 + length) return;
      const mask = buffer.subarray(offset, offset + 4);
      const payload = Buffer.from(buffer.subarray(offset + 4, offset + 4 + length));
      for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i & 3];
      this.buffer = buffer.subarray(offset + 4 + length);
      if (opcode >= 0x8) {
        if (!fin || length > 125) return this.close(1002);
        if (opcode === 0x8) {
          log(`WS ${this.id} close frame received`);
          const code = payload.length >= 2 ? payload.readUInt16BE(0) : 1000;
          return this.close(code === 1005 || code === 1006 || code === 1015 ? 1000 : code);
        }
        if (opcode === 0x9) this.frame(0xa, payload);
        else if (opcode !== 0xa) return this.close(1002);
        continue;
      }
      if (opcode === 0x0) {
        if (this.fragmentOpcode === null) return this.close(1002);
      } else if (opcode === 0x1 || opcode === 0x2) {
        if (this.fragmentOpcode !== null) return this.close(1002);
        this.fragmentOpcode = opcode;
        this.fragments = [];
        this.fragmentSize = 0;
      } else return this.close(1002);
      this.fragments.push(payload);
      this.fragmentSize += payload.length;
      if (this.fragmentSize > MAX_MESSAGE) return this.close(1009);
      if (!fin) continue;
      const message = Buffer.concat(this.fragments);
      const kind = this.fragmentOpcode;
      this.fragmentOpcode = null;
      this.fragments = [];
      if (kind === 0x1) this.engineIo(message.toString("utf8"));
      else log(`WS ${this.id} ignored a binary message`);
    }
  }

  engineIo(message) {
    switch (message[0]) {
      case "2": this.text(`3${message.slice(1)}`); break;
      case "3": clearTimeout(this.pongTimer); break;
      case "1": log(`WS ${this.id} engine.io close`); this.close(1000); break;
      case "4": this.socketIo(message.slice(1)); break;
      default: break;
    }
  }

  prefix(namespace) { return namespace === "/" ? "" : `${namespace},`; }

  emit(namespace, ...args) { this.text(`42${this.prefix(namespace)}${JSON.stringify(args)}`); }

  socketIo(packet) {
    const type = packet[0];
    let rest = packet.slice(1);
    let namespace = "/";
    if (rest.startsWith("/")) {
      const comma = rest.indexOf(",");
      namespace = comma < 0 ? rest : rest.slice(0, comma);
      rest = comma < 0 ? "" : rest.slice(comma + 1);
    }
    if (type === "0") this.connect(namespace);
    else if (type === "1") {
      log(`SIO ${this.id} disconnect ${namespace}`);
      if (namespace === this.namespace) this.close(1000);
    } else if (type === "2" && namespace === this.namespace) {
      let event;
      try { event = JSON.parse(rest.replace(/^\d+/, "")); } catch { return; }
      if (Array.isArray(event) && event[0] === "stdin" && typeof event[1] === "string") this.stdin(event[1]);
    }
  }

  connect(namespace) {
    const sid = JSON.stringify({ sid: randomId() });
    if (namespace !== opts.namespace) {
      if (namespace === "/") {
        // socket.io servers always admit the main namespace; this one never becomes ready.
        this.text(`40${sid}`);
        log(`SIO ${this.id} namespace / accepted (not the service namespace; silent)`);
      } else {
        this.text(`44${namespace},${JSON.stringify({ message: "Invalid namespace" })}`);
        log(`SIO ${this.id} namespace ${namespace} rejected (Invalid namespace)`);
      }
      return;
    }
    if (this.namespace) return;
    if (opts.scenario === "refused") {
      this.text(`44${this.prefix(namespace)}${JSON.stringify({ message: "该账号未开通 VIP 包月" })}`);
      log(`SIO ${this.id} namespace ${namespace} refused (scenario refused)`);
      return;
    }
    this.text(`40${this.prefix(namespace)}${sid}`);
    this.namespace = namespace;
    log(`SIO ${this.id} namespace ${namespace} connected`);
    if (opts.scenario === "disconnect") {
      log(`WS ${this.id} closing (scenario disconnect)`);
      this.close(1000);
      return;
    }
    const args = this.grant.args.split(/\s+/);
    const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 && args[i + 1] ? args[i + 1] : fallback; };
    const gpuType = option("--gpu-type", "vip-share");
    this.usage = { startedAt: iso(Date.now()), finished: false, ready: false, duration: 0, totalCost: "0.0000", gpuType,
      engineType: option("--engine-type", "go"), vip: gpuType.includes("vip"), share: gpuType.includes("share") };
    this.grant.account.usages.unshift(this.usage);
    const weight = option("--kata-weight", "10b512t");
    this.emit(namespace, "stderr", `Loading model ${weight}…\n`);
    this.later(Math.floor(opts.readyDelayMs / 2), () => this.emit(namespace, "stderr", "Initializing neural net backend…\n"));
    if (opts.scenario === "not-ready") return;
    this.later(opts.readyDelayMs, () => {
      this.emit(namespace, "ready");
      this.usage.ready = true;
      log(`SIO ${this.id} ready`);
      this.startEngine();
    });
  }

  startEngine() {
    // --latency-ms delays every engine output, and the close that follows the engine's exit, by
    // the same session timer delay. Timers of one delay fire in the order they were set, so the
    // client sees the output in order and the last of it before the close; a client that
    // disconnects first cancels them all (finish).
    const later = (event) => (text) => {
      if (!opts.latencyMs) return this.emit(this.namespace, event, text);
      this.later(opts.latencyMs, () => this.emit(this.namespace, event, text));
    };
    const stdout = later("stdout");
    const stderr = later("stderr");
    const closeAfterOutput = (code) => this.later(opts.latencyMs, () => this.close(code));
    if (!opts.engine) {
      this.engine = builtinEngine(stdout);
    } else {
      const [command, ...args] = opts.engine;
      const child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
      children.add(child);
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", stdout);
      child.stderr.on("data", stderr);
      child.stdin.on("error", () => {});
      child.on("error", (error) => {
        children.delete(child);
        log(`ENGINE ${this.id} failed to start: ${error.message}`);
        stderr(`fake-zhizi: engine failed to start (${error.code ?? "error"})\n`);
        closeAfterOutput(1011);
      });
      child.on("exit", (code, signal) => {
        children.delete(child);
        log(`ENGINE ${this.id} exited (${signal ?? code})`);
      });
      // "close" comes after the last stdout/stderr data, which "exit" does not guarantee.
      child.on("close", () => closeAfterOutput(1000));
      log(`ENGINE ${this.id} started pid ${child.pid}`);
      this.engine = {
        write: (line) => { if (child.stdin.writable) child.stdin.write(`${line}\n`); },
        kill: () => { if (child.exitCode === null && child.signalCode === null) child.kill(); },
      };
    }
    for (const line of this.pending.splice(0)) this.engine.write(line);
  }

  stdin(text) {
    for (const line of text.split("\n")) {
      const clean = line.replace(/\r$/, "");
      if (!clean.trim()) continue;
      if (opts.stdinLog) fs.appendFileSync(opts.stdinLog, `${JSON.stringify({ t: Date.now(), line: clean })}\n`);
      if (this.engine) this.engine.write(clean); else this.pending.push(clean);
    }
  }

  finish() {
    if (this.closed) return;
    this.closed = true;
    clearInterval(this.pingTimer);
    clearTimeout(this.pongTimer);
    for (const timer of this.timers) clearTimeout(timer);
    this.engine?.kill();
    this.grant.account.sockets--;
    if (this.usage) {
      const ended = Date.now();
      this.usage.finished = true;
      this.usage.endedAt = iso(ended);
      this.usage.duration = Math.max(0, Math.round((ended - Date.parse(this.usage.startedAt)) / 1000));
    }
    log(`WS ${this.id} close`);
  }
}

function randomId() { return crypto.randomBytes(15).toString("base64url"); }

// ---------------------------------------------------------------------------------------------
// Built-in minimal GTP responder (used without --engine)

const EMPTY_SUCCESS = new Set(["boardsize", "clear_board", "komi", "play", "undo", "time_settings", "kata-time_settings",
  "kgs-time_settings", "time_left", "kata-set-rules", "kata-set-rule", "kata-set-param", "set_free_handicap", "quit"]);
const COMMANDS = ["protocol_version", "name", "version", "known_command", "list_commands", ...EMPTY_SUCCESS, "kata-analyze"];

function builtinEngine(write) {
  let analysis = null;
  let boardSize = 19;
  const stop = () => {
    if (!analysis) return;
    clearInterval(analysis);
    analysis = null;
    write("\n");
  };
  const answer = (id, text) => write(`=${id}${text ? ` ${text}` : ""}\n\n`);
  const error = (id, text) => write(`?${id} ${text}\n\n`);
  const analyze = (id, args) => {
    let interval = 100; // centiseconds
    let ownership = false, rootInfo = false;
    let i = 0;
    if (/^(b|w|black|white)$/i.test(args[i] ?? "")) i++;
    if (/^\d+$/.test(args[i] ?? "")) interval = Number(args[i++]);
    for (; i + 1 < args.length; i += 2) {
      if (args[i] === "interval") interval = Number(args[i + 1]) || interval;
      if (args[i] === "ownership") ownership = args[i + 1] === "true";
      if (args[i] === "rootInfo") rootInfo = args[i + 1] === "true";
    }
    write(`=${id}\n`);
    let visits = 0;
    analysis = setInterval(() => {
      visits += 37;
      const second = Math.floor(visits / 3);
      let line = `info move D4 visits ${visits} edgeVisits ${visits} utility 0.0 winrate 0.5 scoreMean 0.5 scoreStdev 10.0 scoreLead 0.5 scoreSelfplay 0.5 prior 0.1 lcb 0.49 utilityLcb 0.0 weight ${visits} order 0 pv D4 Q16`
        + ` info move Q16 visits ${second} edgeVisits ${second} utility -0.01 winrate 0.49 scoreMean 0.3 scoreStdev 10.0 scoreLead 0.3 scoreSelfplay 0.3 prior 0.09 lcb 0.47 utilityLcb -0.02 weight ${second} order 1 pv Q16 D4`;
      if (rootInfo) line += ` rootInfo visits ${visits + second} utility 0.0 winrate 0.5 scoreMean 0.5 scoreSelfplay 0.5 scoreLead 0.5 scoreStdev 10.0 currentPlayer B`;
      if (ownership) line += ` ownership ${new Array(boardSize * boardSize).fill("0.0").join(" ")}`;
      write(`${line}\n`);
    }, Math.max(5, interval) * 10);
  };
  return {
    write(raw) {
      const words = raw.replace(/#.*$/, "").trim().split(/\s+/).filter(Boolean);
      if (!words.length) return;
      stop();
      const id = words.length > 1 && /^\d+$/.test(words[0]) ? words.shift() : "";
      const [command, ...args] = words;
      if (command === "protocol_version") answer(id, "2");
      else if (command === "name") answer(id, "KataGo");
      else if (command === "version") answer(id, "1.16.0");
      else if (command === "list_commands") answer(id, COMMANDS.join("\n"));
      else if (command === "known_command") answer(id, String(COMMANDS.includes(args[0])));
      else if (command === "kata-analyze") analyze(id, args);
      else if (EMPTY_SUCCESS.has(command)) {
        if (command === "boardsize") boardSize = Number(args[0]) || boardSize;
        answer(id, "");
      } else error(id, "unknown command");
    },
    kill() {
      clearInterval(analysis);
      analysis = null;
    },
  };
}

// ---------------------------------------------------------------------------------------------

server.on("error", (error) => {
  console.error(`fake-zhizi: ${error.message}`);
  process.exit(1);
});
server.listen(opts.port, opts.host, () => {
  const { port } = server.address();
  log(`FAKE-ZHIZI READY http://${opts.host.includes(":") ? `[${opts.host}]` : opts.host}:${port}`);
});
process.on("exit", () => { for (const child of children) child.kill(); });
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => process.exit(0));
}
