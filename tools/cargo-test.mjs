#!/usr/bin/env node
// Runs the Rust tests like `cargo test --no-fail-fast`, but executes the test binaries a few at a
// time instead of one after another. Each binary still runs in its own process, from its package
// directory, as under cargo.
//
//   node tools/cargo-test.mjs [--jobs N] [cargo test selection] [-- test binary arguments]
//
// The selection goes to `cargo test --no-run` (for example `-p lizzie-core`, `--test panels`,
// `--features lizzieyzy-tauri/acceptance`); arguments after `--` reach every test binary (name
// filters, `--exact`, `--nocapture`). Without a target selection the doc tests run too, as cargo
// does; `--doc` runs only them. Binaries start longest first, by the durations earlier runs recorded in
// <target dir>/cargo-test-durations.json. At most --jobs (default 4) run at once: starting every
// binary together starves the timing-sensitive tests.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TARGET_SELECTION = new Set([
  "--lib", "--bin", "--bins", "--test", "--tests", "--example", "--examples",
  "--bench", "--benches", "--all-targets", "--doc",
]);

function fail(message) {
  console.error(`cargo-test: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const split = argv.indexOf("--");
  const own = split < 0 ? argv : argv.slice(0, split);
  const options = { jobs: 4, cargoArgs: [], binaryArgs: split < 0 ? [] : argv.slice(split + 1) };
  for (let i = 0; i < own.length; i++) {
    if (own[i] === "--jobs") {
      options.jobs = Number(own[++i]);
      if (!Number.isInteger(options.jobs) || options.jobs < 1) fail("--jobs needs a positive integer");
    } else {
      options.cargoArgs.push(own[i]);
    }
  }
  return options;
}

function cargo(args) {
  const result = spawnSync("cargo", args, {
    cwd: root, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "inherit"],
  });
  if (result.error) fail(`cannot run cargo: ${result.error.message}`);
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result.stdout;
}

function testJobs({ cargoArgs, binaryArgs }) {
  const metadata = JSON.parse(cargo(["metadata", "--format-version", "1", "--no-deps"]));
  const packages = new Map(metadata.packages.map((p) => [p.manifest_path, p.name]));
  // `--doc` selects only the doc tests, which cargo builds and runs itself (no `--no-run`).
  const docOnly = cargoArgs.includes("--doc");
  const built = docOnly ? "" : cargo(["test", "--no-run", "--message-format=json-render-diagnostics", ...cargoArgs]);
  const jobs = new Map();
  for (const line of built.split("\n")) {
    if (!line.startsWith("{")) continue;
    const message = JSON.parse(line);
    if (message.reason !== "compiler-artifact" || !message.executable || !message.profile.test) continue;
    const dir = path.dirname(message.manifest_path);
    const name = packages.get(message.manifest_path);
    jobs.set(message.executable, {
      key: `${name} ${message.target.kind[0]} ${message.target.name}`,
      cmd: message.executable,
      args: binaryArgs,
      cwd: dir,
      env: { ...process.env, CARGO_MANIFEST_DIR: dir, CARGO_PKG_NAME: name },
      ownSummaries: 1,
    });
  }
  const selected = cargoArgs.some((arg) => TARGET_SELECTION.has(arg.split("=")[0]));
  if (docOnly || !selected) {
    jobs.set("doc", {
      key: "doc tests",
      cmd: "cargo",
      args: ["test", ...(docOnly ? [] : ["--doc"]), ...cargoArgs, "--", ...binaryArgs],
      cwd: root,
      env: process.env,
      ownSummaries: Infinity,
    });
  }
  return { jobs: [...jobs.values()], durationsFile: path.join(metadata.target_directory, "cargo-test-durations.json") };
}

function run(job) {
  return new Promise((resolve) => {
    const start = performance.now();
    let output = "";
    let done = false;
    const finish = (ok, signal) => {
      if (done) return;
      done = true;
      // A binary's own summary is its last one: tests that re-run the binary as a child print
      // the child's summary first. The doc tests print one summary per crate.
      const counts = { passed: 0, failed: 0, ignored: 0 };
      const summaries = [...output.matchAll(/^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/gm)];
      for (const m of summaries.slice(-job.ownSummaries)) {
        counts.passed += Number(m[1]);
        counts.failed += Number(m[2]);
        counts.ignored += Number(m[3]);
      }
      resolve({ job, ok, signal, output, counts, seconds: (performance.now() - start) / 1000 });
    };
    const child = spawn(job.cmd, job.args, { cwd: job.cwd, env: job.env });
    child.stdout.on("data", (chunk) => { output += chunk; });
    child.stderr.on("data", (chunk) => { output += chunk; });
    child.on("error", (error) => {
      output += `${error.message}\n`;
      finish(false, null);
    });
    child.on("close", (code, signal) => finish(code === 0, signal));
  });
}

async function runAll(jobs, limit) {
  const results = [];
  let next = 0;
  async function worker() {
    while (next < jobs.length) {
      const result = await run(jobs[next++]);
      const { passed, failed, ignored } = result.counts;
      const summary = `${result.job.key}: ${passed} passed, ${failed} failed, ${ignored} ignored (${result.seconds.toFixed(1)}s)`;
      if (result.ok) {
        console.log(`ok   ${summary}`);
      } else {
        console.log(`FAIL ${summary}${result.signal ? `, killed by ${result.signal}` : ""}\n${result.output}`);
      }
      results.push(result);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, jobs.length) }, worker));
  return results;
}

const options = parseArgs(process.argv.slice(2));
const { jobs, durationsFile } = testJobs(options);
const durations = fs.existsSync(durationsFile) ? JSON.parse(fs.readFileSync(durationsFile, "utf8")) : {};
jobs.sort((a, b) => (durations[b.key] ?? Number.MAX_VALUE) - (durations[a.key] ?? Number.MAX_VALUE));

const start = performance.now();
const results = await runAll(jobs, options.jobs);
for (const result of results) durations[result.job.key] = result.seconds;
fs.writeFileSync(durationsFile, `${JSON.stringify(durations, null, 2)}\n`);

const total = { passed: 0, failed: 0, ignored: 0 };
for (const { counts } of results) for (const k in total) total[k] += counts[k];
const failedJobs = results.filter((r) => !r.ok).map((r) => r.job.key);
console.log(
  `\n${results.length} test binaries: ${total.passed} passed, ${total.failed} failed, ${total.ignored} ignored`
  + ` in ${((performance.now() - start) / 1000).toFixed(1)}s`
  + (failedJobs.length ? `\nfailed: ${failedJobs.join(", ")}` : ""),
);
process.exit(failedJobs.length ? 1 : 0);
