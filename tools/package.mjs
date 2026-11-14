#!/usr/bin/env node
// Builds the distribution packages of the current checkout for one platform and architecture,
// gives them stable asset names and records their source commit, toolchain and SHA-256.
//
//   node tools/package.mjs [--identity formal|acceptance] [--target <rust triple>]
//                          [--out <dir>] [--update-pubkey <file>] [--allow-dirty]
//
// Windows x64: NSIS installer and portable zip, both carrying the board synchronization helper
// (qiyi71w/readboard, pinned below) as `readboard/` next to the program. macOS arm64/x64: dmg
// (build each architecture with its --target on a Mac). Linux x64: deb and AppImage (build on
// Ubuntu 24.04, see tools/packaging/build-linux.sh). The acceptance identity uses its own
// application identifier, binary and package names and contains the acceptance channel; the
// formal identity never does.
//
// Update signing (docs/updates.md): --update-pubkey builds the update public key into the
// program. When the Tauri CLI's private key variables are set (TAURI_SIGNING_PRIVATE_KEY or
// TAURI_SIGNING_PRIVATE_KEY_PATH, with TAURI_SIGNING_PRIVATE_KEY_PASSWORD), the packages the
// program updates itself from (NSIS installer, macOS `.app.tar.gz`, AppImage) are signed for this
// version and listed in `<prefix>.update.json` for tools/update-manifest.mjs.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = path.join(root, "src-tauri");
const ACCEPTANCE_CONFIG = path.join(tauriDir, "tauri.acceptance.conf.json");

const PLATFORMS = {
  "x86_64-pc-windows-msvc": { os: "windows", arch: "x64", bundles: ["nsis"] },
  "aarch64-apple-darwin": { os: "macos", arch: "arm64", bundles: ["dmg"] },
  "x86_64-apple-darwin": { os: "macos", arch: "x64", bundles: ["dmg"] },
  "x86_64-unknown-linux-gnu": { os: "linux", arch: "x64", bundles: ["deb", "appimage"] },
};
const HOST_TRIPLES = {
  "win32-x64": "x86_64-pc-windows-msvc",
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
};
// The board synchronization helper of the Windows packages: the release asset of
// qiyi71w/readboard, with the size and SHA-256 of the release's `.sha256` file.
const READBOARD = {
  version: "v3.2.0",
  asset: "readboard-webview2-v3.2.0.zip",
  size: 126901426,
  sha256: "6abb1c2764fe14f72f77a2444d314e037d1d1e95c6c95974cd6c9c67de6e9512",
};

function fail(message) {
  console.error(`package: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = { identity: "formal", target: null, out: path.join(root, "target", "packages"), updatePubkey: null, allowDirty: false };
  for (let i = 0; i < argv.length; i++) {
    const value = () => argv[++i] ?? fail(`${argv[i - 1]} needs a value`);
    switch (argv[i]) {
      case "--identity": options.identity = value(); break;
      case "--target": options.target = value(); break;
      case "--out": options.out = path.resolve(value()); break;
      case "--update-pubkey": options.updatePubkey = path.resolve(value()); break;
      case "--allow-dirty": options.allowDirty = true; break;
      default: fail(`unknown argument ${argv[i]}`);
    }
  }
  if (!["formal", "acceptance"].includes(options.identity)) fail("--identity is formal or acceptance");
  return options;
}

function run(file, args, opts = {}) {
  return execFileSync(file, args, { cwd: root, encoding: "utf8", ...opts });
}

function tryVersion(file, args) {
  try {
    return run(file, args, { stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return null;
  }
}

function sha256(file) {
  return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

/** The single file with this extension in a bundle directory. */
function single(dir, extension) {
  const found = fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => f.endsWith(extension)) : [];
  if (found.length !== 1) fail(`expected one ${extension} in ${dir}, found ${found.length}`);
  return path.join(dir, found[0]);
}

/** Windows' own bsdtar: it reads and writes zip archives; Git's GNU tar earlier on PATH cannot. */
function windowsTar() {
  return path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
}

/** The pinned helper archive, downloaded once into target/readboard and checked by size and SHA-256. */
async function readboardArchive() {
  const dir = path.join(root, "target", "readboard");
  const file = path.join(dir, READBOARD.asset);
  const valid = () => fs.existsSync(file) && fs.statSync(file).size === READBOARD.size && sha256(file) === READBOARD.sha256;
  if (valid()) return file;
  fs.mkdirSync(dir, { recursive: true });
  const url = `https://github.com/qiyi71w/readboard/releases/download/${READBOARD.version}/${READBOARD.asset}`;
  console.log(`package: downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok || !response.body) fail(`download ${url}: HTTP ${response.status}`);
  const partial = `${file}.part`;
  await pipeline(Readable.fromWeb(response.body), fs.createWriteStream(partial));
  fs.renameSync(partial, file);
  if (!valid()) {
    fs.rmSync(file, { force: true });
    fail(`${READBOARD.asset} does not have the pinned size ${READBOARD.size} and SHA-256 ${READBOARD.sha256}`);
  }
  return file;
}

/**
 * The helper's files, extracted afresh from the archive: the archive root, or its single
 * top-level directory (the layouts the hosted-update installer accepts). It must hold
 * readboard.exe, which the program looks for in `<resources>/readboard/`.
 */
function readboardDirectory(archive) {
  const dir = path.join(root, "target", "readboard", READBOARD.version);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  run(windowsTar(), ["-x", "-f", archive, "-C", dir]);
  const entries = fs.readdirSync(dir);
  const top = entries.length === 1 && fs.statSync(path.join(dir, entries[0])).isDirectory() ? path.join(dir, entries[0]) : dir;
  if (!fs.existsSync(path.join(top, "readboard.exe"))) fail(`${READBOARD.asset} has no readboard.exe`);
  return top;
}

const options = parseArgs(process.argv.slice(2));
const triple = options.target ?? HOST_TRIPLES[`${process.platform}-${process.arch}`];
const platform = PLATFORMS[triple];
if (!platform) fail(`unsupported platform ${triple ?? `${process.platform}-${process.arch}`}`);

const commit = run("git", ["rev-parse", "HEAD"]).trim();
const dirty = run("git", ["status", "--porcelain"]).trim() !== "";
if (dirty && !options.allowDirty) fail("the working tree has changes; commit them or pass --allow-dirty");

const base = JSON.parse(fs.readFileSync(path.join(tauriDir, "tauri.conf.json"), "utf8"));
const config = options.identity === "acceptance" ? { ...base, ...JSON.parse(fs.readFileSync(ACCEPTANCE_CONFIG, "utf8")) } : base;
const assetPrefix = `${config.mainBinaryName}_${config.version}_${platform.os}-${platform.arch}`;
const tauriCli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");

// The update key. The private key stays in the Tauri CLI's environment: it is never passed as an
// argument or written out.
const updatePubkey = options.updatePubkey ? fs.readFileSync(options.updatePubkey, "utf8").trim() : (config.plugins?.updater?.pubkey ?? "");
const signing = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY || process.env.TAURI_SIGNING_PRIVATE_KEY_PATH);
if (signing && !updatePubkey) fail("signing updates needs the update public key: pass --update-pubkey");

const releaseDir = path.join(root, "target", options.target ?? "", "release");
const bundleDir = path.join(releaseDir, "bundle");
// Bundle directories keep files from earlier builds; start empty so only this build is collected.
for (const kind of [...platform.bundles, "macos"]) fs.rmSync(path.join(bundleDir, kind), { recursive: true, force: true });

// Windows packages carry the helper as a resource directory `readboard/` (next to the program).
const readboardDir = platform.os === "windows" ? readboardDirectory(await readboardArchive()) : null;

const tauriArgs = ["build", "--ci", "--bundles", platform.bundles.join(",")];
if (options.target) tauriArgs.push("--target", options.target);
if (options.identity === "acceptance") tauriArgs.push("--config", ACCEPTANCE_CONFIG, "--features", "acceptance");
if (readboardDir) {
  const source = `${readboardDir.split(path.sep).join("/")}/`;
  tauriArgs.push("--config", JSON.stringify({ bundle: { resources: { [source]: "readboard/" } } }));
}
if (options.updatePubkey) tauriArgs.push("--config", JSON.stringify({ plugins: { updater: { pubkey: updatePubkey } } }));
run(process.execPath, [tauriCli, ...tauriArgs], { stdio: "inherit" });

fs.mkdirSync(options.out, { recursive: true });
const assets = [];
function publish(source, suffix, form) {
  const file = `${assetPrefix}${suffix}`;
  fs.copyFileSync(source, path.join(options.out, file));
  assets.push({ file, form });
}

for (const kind of platform.bundles) {
  switch (kind) {
    case "nsis": publish(single(path.join(bundleDir, "nsis"), ".exe"), "-setup.exe", "nsis"); break;
    case "dmg": publish(single(path.join(bundleDir, "dmg"), ".dmg"), ".dmg", "dmg"); break;
    case "deb": publish(single(path.join(bundleDir, "deb"), ".deb"), ".deb", "deb"); break;
    case "appimage": publish(single(path.join(bundleDir, "appimage"), ".AppImage"), ".AppImage", "appimage"); break;
  }
}

if (platform.os === "windows") {
  // Portable zip: one folder with the program, the board synchronization helper and the
  // licence. Data stays in the per-user directories of the identity, as for the installed
  // program.
  const staging = fs.mkdtempSync(path.join(options.out, ".portable-"));
  const folder = path.join(staging, config.productName);
  fs.mkdirSync(folder);
  fs.copyFileSync(path.join(releaseDir, `${config.mainBinaryName}.exe`), path.join(folder, `${config.mainBinaryName}.exe`));
  fs.cpSync(readboardDir, path.join(folder, "readboard"), { recursive: true });
  fs.copyFileSync(path.join(root, "LICENSE.txt"), path.join(folder, "LICENSE.txt"));
  const file = `${assetPrefix}-portable.zip`;
  fs.rmSync(path.join(options.out, file), { force: true });
  run(windowsTar(), ["-a", "-c", "-f", path.join(options.out, file), "-C", staging, config.productName]);
  fs.rmSync(staging, { recursive: true, force: true });
  assets.push({ file, form: "portable-zip" });
}

// The macOS update package: the app bundle as the `.app.tar.gz` the updater unpacks.
// COPYFILE_DISABLE keeps macOS tar from adding `._` metadata entries.
if (signing && platform.os === "macos") {
  const file = `${assetPrefix}.app.tar.gz`;
  run("tar", ["-czf", path.join(options.out, file), "-C", path.join(bundleDir, "macos"), `${config.productName}.app`], { env: { ...process.env, COPYFILE_DISABLE: "1" } });
  assets.push({ file, form: "app" });
}

for (const asset of assets) {
  const file = path.join(options.out, asset.file);
  asset.size = fs.statSync(file).size;
  asset.sha256 = sha256(file);
}

// Update entries, keyed `{os}-{arch}-{form}` as the program asks for them. The portable zip, the
// dmg and the deb are not installed by the updater.
const UPDATE_OS = { windows: "windows", macos: "darwin", linux: "linux" };
const UPDATE_ARCH = { x64: "x86_64", arm64: "aarch64" };
if (signing) {
  const platforms = {};
  for (const asset of assets.filter((a) => ["nsis", "app", "appimage"].includes(a.form))) {
    const file = path.join(options.out, asset.file);
    run(process.execPath, [tauriCli, "signer", "sign", "--app-version", config.version, file], { stdio: ["ignore", "ignore", "inherit"] });
    asset.signature = `${asset.file}.sig`;
    platforms[`${UPDATE_OS[platform.os]}-${UPDATE_ARCH[platform.arch]}-${asset.form}`] = {
      file: asset.file,
      size: asset.size,
      signature: fs.readFileSync(`${file}.sig`, "utf8").trim(),
    };
  }
  fs.writeFileSync(path.join(options.out, `${assetPrefix}.update.json`), JSON.stringify({ version: config.version, platforms }, null, 2) + "\n");
}

const manifest = {
  product: config.productName,
  identity: options.identity,
  identifier: config.identifier,
  version: config.version,
  os: platform.os,
  arch: platform.arch,
  target: triple,
  source: { commit, dirty },
  toolchain: {
    rustc: tryVersion("rustc", ["--version"]),
    node: process.version,
    tauriCli: JSON.parse(fs.readFileSync(path.join(root, "node_modules", "@tauri-apps", "cli", "package.json"), "utf8")).version,
  },
  update: { pubkey: updatePubkey || null, signed: signing },
  assets,
  bundled: readboardDir ? { readboard: { version: READBOARD.version, asset: READBOARD.asset, size: READBOARD.size, sha256: READBOARD.sha256 } } : {},
};
fs.writeFileSync(path.join(options.out, `${assetPrefix}.manifest.json`), JSON.stringify(manifest, null, 2) + "\n");
fs.writeFileSync(path.join(options.out, `${assetPrefix}.sha256`), assets.map((a) => `${a.sha256}  ${a.file}\n`).join(""));
for (const asset of assets) console.log(`${asset.sha256}  ${asset.file}`);
console.log(`package: ${options.identity} ${platform.os}-${platform.arch} from ${commit}${dirty ? " (dirty)" : ""} -> ${options.out}`);
