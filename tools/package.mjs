#!/usr/bin/env node
// Builds the distribution packages of the current checkout for one platform and architecture,
// gives them stable asset names and records their source commit, toolchain and SHA-256.
//
//   node tools/package.mjs [--identity formal|acceptance] [--target <rust triple>]
//                          [--out <dir>] [--allow-dirty]
//
// Windows x64: NSIS installer and portable zip. macOS arm64/x64: dmg (build each architecture
// with its --target on a Mac). Linux x64: deb and AppImage (build on Ubuntu 24.04, see
// tools/packaging/build-linux.sh). The acceptance identity uses its own application identifier,
// binary and package names and contains the acceptance channel; the formal identity never does.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
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

function fail(message) {
  console.error(`package: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const options = { identity: "formal", target: null, out: path.join(root, "target", "packages"), allowDirty: false };
  for (let i = 0; i < argv.length; i++) {
    const value = () => argv[++i] ?? fail(`${argv[i - 1]} needs a value`);
    switch (argv[i]) {
      case "--identity": options.identity = value(); break;
      case "--target": options.target = value(); break;
      case "--out": options.out = path.resolve(value()); break;
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

const releaseDir = path.join(root, "target", options.target ?? "", "release");
const bundleDir = path.join(releaseDir, "bundle");
// Bundle directories keep files from earlier builds; start empty so only this build is collected.
for (const kind of [...platform.bundles, "macos"]) fs.rmSync(path.join(bundleDir, kind), { recursive: true, force: true });

const tauriArgs = ["build", "--ci", "--bundles", platform.bundles.join(",")];
if (options.target) tauriArgs.push("--target", options.target);
if (options.identity === "acceptance") tauriArgs.push("--config", ACCEPTANCE_CONFIG, "--features", "acceptance");
run(process.execPath, [path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js"), ...tauriArgs], { stdio: "inherit" });

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
  // Portable zip: one folder with the program and its licence. Data stays in the per-user
  // directories of the identity, as for the installed program.
  const staging = fs.mkdtempSync(path.join(options.out, ".portable-"));
  const folder = path.join(staging, config.productName);
  fs.mkdirSync(folder);
  fs.copyFileSync(path.join(releaseDir, `${config.mainBinaryName}.exe`), path.join(folder, `${config.mainBinaryName}.exe`));
  fs.copyFileSync(path.join(root, "LICENSE.txt"), path.join(folder, "LICENSE.txt"));
  const file = `${assetPrefix}-portable.zip`;
  fs.rmSync(path.join(options.out, file), { force: true });
  // Windows' own bsdtar writes zip archives with -a; Git's GNU tar earlier on PATH cannot.
  const tar = path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
  run(tar, ["-a", "-c", "-f", path.join(options.out, file), "-C", staging, config.productName]);
  fs.rmSync(staging, { recursive: true, force: true });
  assets.push({ file, form: "portable-zip" });
}

for (const asset of assets) {
  const file = path.join(options.out, asset.file);
  asset.size = fs.statSync(file).size;
  asset.sha256 = sha256(file);
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
  assets,
};
fs.writeFileSync(path.join(options.out, `${assetPrefix}.manifest.json`), JSON.stringify(manifest, null, 2) + "\n");
fs.writeFileSync(path.join(options.out, `${assetPrefix}.sha256`), assets.map((a) => `${a.sha256}  ${a.file}\n`).join(""));
for (const asset of assets) console.log(`${asset.sha256}  ${asset.file}`);
console.log(`package: ${options.identity} ${platform.os}-${platform.arch} from ${commit}${dirty ? " (dirty)" : ""} -> ${options.out}`);
