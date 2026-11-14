#!/usr/bin/env node
// Writes the update manifest `lizzieyzy-tauri-update.json` of one release from the
// `<prefix>.update.json` files tools/package.mjs leaves next to the signed packages of each
// platform (docs/updates.md).
//
//   node tools/update-manifest.mjs --tag <release tag> [--prerelease] [--base-url <url>]
//                                  [--out <file>] <prefix>.update.json...
//
// Packages are downloaded from the release's assets (`--base-url` replaces that address, for a
// test release source). `--prerelease` marks a 测试 release: the 正式 channel never offers it.

import fs from "node:fs";
import path from "node:path";

const REPOSITORY = "https://github.com/qiyi71w/lizzieyzy-desktop";

function fail(message) {
  console.error(`update-manifest: ${message}`);
  process.exit(1);
}

const options = { tag: null, prerelease: false, baseUrl: null, out: "lizzieyzy-tauri-update.json", fragments: [] };
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  const value = () => argv[++i] ?? fail(`${argv[i - 1]} needs a value`);
  switch (argv[i]) {
    case "--tag": options.tag = value(); break;
    case "--prerelease": options.prerelease = true; break;
    case "--base-url": options.baseUrl = value().replace(/\/+$/, ""); break;
    case "--out": options.out = value(); break;
    default:
      if (argv[i].startsWith("--")) fail(`unknown argument ${argv[i]}`);
      options.fragments.push(argv[i]);
  }
}
if (!options.tag) fail("--tag is required");
if (options.fragments.length === 0) fail("no <prefix>.update.json given");

const base = options.baseUrl ?? `${REPOSITORY}/releases/download/${encodeURIComponent(options.tag)}`;
let version = null;
const platforms = {};
for (const fragment of options.fragments) {
  const entry = JSON.parse(fs.readFileSync(fragment, "utf8"));
  // Every package is signed for the release's version; the updater refuses any other pairing.
  if (version !== null && entry.version !== version) fail(`${path.basename(fragment)} is version ${entry.version}, not ${version}`);
  version = entry.version;
  for (const [key, item] of Object.entries(entry.platforms)) {
    if (platforms[key]) fail(`${key} appears twice`);
    platforms[key] = { url: `${base}/${encodeURIComponent(item.file)}`, signature: item.signature, size: item.size };
  }
}
if (Object.keys(platforms).length === 0) fail("the files list no signed packages");

const manifest = {
  version,
  pub_date: new Date().toISOString().replace(/\.\d+Z$/, "Z"),
  prerelease: options.prerelease,
  notesUrl: `${REPOSITORY}/releases/tag/${encodeURIComponent(options.tag)}`,
  platforms,
};
fs.writeFileSync(options.out, JSON.stringify(manifest, null, 2) + "\n");
console.log(`update-manifest: ${version}${options.prerelease ? " (测试)" : ""}, ${Object.keys(platforms).sort().join(", ")} -> ${options.out}`);
