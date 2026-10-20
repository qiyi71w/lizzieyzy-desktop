# LizzieYzy Tauri

A desktop Go (weiqi) review workbench being migrated from the Java
[LizzieYzy Next](https://github.com/wimi321/lizzieyzy-next) (reference commit
`cb53acc55eda42c6c671b5f668cd3b597456bf07`) to Tauri 2 + Rust + TypeScript. The Java program's
behaviour, defaults and workbench layout are the reference; this client keeps its own identity
and data directory. It only reads the Java configuration, and only when the user imports it.

## Current state

Implemented: opening, browsing and saving records, analysis with one local GTP engine (KataGo,
Leela Zero, Sayuri, Sai or Leela 0.11, recognised by their GTP name like Java), and importing
the matching LizzieYzy Next settings.

- Open SGF and GIB files with the File menu, the toolbar button, `O`, a dropped file, a command
  line argument, or File > recent records. Paste an SGF record with `Ctrl+V` (it asks before
  replacing a record that has content).
- Browse with the Java keys and buttons: `Up`/`Down`, `Ctrl+Up`/`Ctrl+Down` and
  `PageUp`/`PageDown` (10 moves), `Home`/`End`, `Left`/`Right` (sibling branches),
  `Ctrl+Left` (branch point), `Shift+Up` (branch start), `Ctrl+Shift+Up`/`Down` (previous/next
  comment), `B` (back to the main branch), the mouse wheel, the bottom bar navigation buttons and
  move-number field, and clicks in the variation tree. The variation tree scrolls with its own
  scroll bars and mouse wheel; it re-centres on the current node when the position changes.
- Stepping forward plays the Java stone sounds (stone, capture, capture of three or more).
- The board shows setup stones, moves, passes, captures, comments, SGF markup (TR/SQ/CR/MA/LB),
  next-move rings, player names, the sub board, the variation tree and the empty winrate graph.
- A failed or cancelled open keeps the current record. Opening a 9x9 record after a 19x19 one (or
  back) replaces the board completely.
- Save with File > 保存, the toolbar save button or `Ctrl+S`: a record that came from (or was
  saved to) a file overwrites that same file, under its own name, after the Java "覆盖原文件?"
  prompt (Cancel is the default; "不再提示" turns it off, View > 主界面设置 >
  显示覆盖文件提示对话框 and View > 重置界面位置 turn it back on). A new, pasted or GIB record
  goes to the save chooser instead.
- Save As with File > 另存为 or `S`: the system save chooser opens in the last save folder (at
  first the home folder, Documents on Windows) with the Java default name (`<black>_Vs_<white>`
  or 未命名, then a timestamp). `.sgf` is added to a chosen name without it; if that file exists
  the Java "SGF文件已经存在" prompt asks first (the system chooser itself asks about a file
  picked directly). The record then belongs to the new file.
- Records are always written as UTF-8 SGF (GIB is read-only, as in Java). Setup stones, the side
  to play, real and placeholder passes, branches, comments, markup, `MN` and the saved analysis
  properties (`LZ`, `LZ2`, `LZOP`, `LZOP2`) and other unknown properties are kept, and LizzieYzy
  Next reads the files with the same meaning. A file is written to an owner-only temporary file
  next to it and then renamed, so a failed save ("保存失败") leaves the old file untouched; a
  replaced file keeps its permissions. Closing the window waits for saves still being written.
- Settings available now: reading komi from SGF records, stone sounds on/off, and the UI language
  (zh-CN, zh-TW, en, ja, ko, th; applied after a restart, like Java). A setting that cannot be
  saved is reported and left unchanged.
- Help > 诊断与日志 shows this client's logs, opens the log folder and exports a diagnostics zip;
  Help > 清除所有个人数据 clears the recent-record list; View > 重置界面位置 resets the saved
  window placement and last folder and turns the overwrite prompt back on.
- Engines: the engine list (更多引擎 / `Alt+X`) adds, edits, orders and removes engine profiles
  with Java's fields (name, command with Java quoting, initial commands, board size, komi,
  default engine, preload). The engine at start-up is the default one, the last one, none, or
  chosen from the list. `Ctrl+1`…`Ctrl+0` switch the foreground engine; with fast engine change
  the previous engine keeps running for a quick switch back. Analysis menu items close or
  restart engines. As in Java, an engine's board size applies to a new empty game, and its komi
  becomes the game komi unless the komi was set by hand (an engine that fails to start changes
  nothing).
- Analysis: candidates on the board (win rate, visits, score lead), the variation shown when
  hovering a candidate (captures are worked out by the core), the sub board cycling through
  candidates on click, the candidate table, the black-perspective win-rate line with its 50%
  fill and the score line (click to navigate), and the status and title lines. `Space` pauses
  and resumes for this session only (never saved); opening another record ends the pause. Time
  and visit limits stop the current position once and resume on the next one. Results stay with
  the position, komi and rules they were requested for, so browsing quickly, switching records,
  changing komi or rules never shows another context's analysis; changing komi or rules discards
  the stored analysis like Java.
- Rules: an SGF record's `RU` is applied to KataGo (`kata-set-rules`) before analysis. When the
  engine cannot confirm it, a dialog asks whether to continue with the engine's current rules
  (no pauses analysis). `Shift+D` opens the KataGo rules editor.
- GTP console (`E`) shows the engine traffic and sends commands. Like Java, `komi <v>` sets the
  game komi and `undo` steps back in the record; other commands that would change the engine's
  board (`play`, `clear_board`, `place_free_handicap`, …) are refused because the record owns the
  position. The 完整日志 option writes the GTP traffic to `logs/engine-trace.log`. A failing
  engine shows Java's failure dialog with its output, copy and diagnostics export. Engines are
  ended (`quit`, then killed after 3 s) when the window closes.
- Logs, the engine trace, failure details and exported diagnostics pass through a port of
  Java's credential sanitizer (`--password …`, `token=…`, JSON credential keys, authorization
  headers, URL user info, encoded forms); the live console shows raw traffic like Java.
- Importing LizzieYzy Next settings: offered at the first start (no settings of this client
  yet) and in Help > 从 LizzieYzy Next 导入设置…. Choose the Java settings folder (the one with
  `config.txt`; Java's per-user folders such as `~/.lizzieyzy-next` and, on Windows, the shared
  `LizzieYzyNext` folders are suggested when they exist). A preview lists every Java setting as
  imported, not usable here (invalid values; Java window positions) or left out (features not in
  this client yet, accounts and credentials), with the engine list. Imported: record settings
  (load komi, go to last move, replace prompt), language, stone sounds, recent records, last
  folder, the engine list with its default and start-up choice, each engine's thread policy and
  benchmark results and its GTP configuration profile, analysis limits and interval, fast engine
  change, limit notice, KataGo rules, the start-up benchmark choice and the network proxy (only
  values Java itself accepts). Java's marker for its own bundled KataGo package is reported as not
  applicable. Engines keep using the Java engine and
  model files: relative paths are resolved as Java launches them (a bare program name is looked
  up on `PATH`; relative files, including a Java engine's `-jar` file, in the Java folder or an
  optional Java program folder), and missing programs or `-model`/`-config` files are listed
  without starting anything. Remote (SSH) engines are left out, and so is a default or last
  engine choice that points to a left-out engine; an engine whose command holds a credential is
  imported only after ticking "复制凭据", and credential values never appear in the preview,
  report or logs. Once this client has its own settings, every setting the import would change is
  listed and replaced only after confirming. Importing writes this client's `config.json` and
  `persist.json` together (a failed write leaves both unchanged; an existing settings file that
  cannot be read is not replaced), restarts the engines with the imported choice, and shows the
  report; the language applies after a restart. Java's files are never written, so both programs
  can be used side by side.
- An analysis time limit of 0 seconds means 9999 minutes, as in Java.
- Engine resources (Settings > 引擎一键设置, the toolbar 一键设置 button, the bottom bar 更新官方权重
  button for the weights page, and the "AI 未就绪 - 点击修复" banner shown while no engine has a
  command): this client does not bundle an engine. Opening the dialog detects the computer (OS,
  CPU architecture and threads, memory, NVIDIA GPUs through `nvidia-smi` with VRAM, driver and
  compute capability, Apple chip and Rosetta through `sysctl`, CUDA/cuDNN libraries and the OpenCL
  driver on Linux, `OpenCL.dll` on Windows); a value that cannot be read is shown as unknown. It
  then lists every KataGo package for this platform with its tier (stable, optional, experimental),
  source, license, runtime needs, size and capability, recommends one with its reasons and missing
  dependencies, and offers the three Java model choices (balanced, strongest (default),
  lightweight). Nothing is downloaded, installed, switched or replaced without a click and a
  confirmation; experimental packages (Windows DirectML, OpenVINO, ROCm) are never recommended and
  ask first. Downloads resume and can be stopped (the partial file is kept for a later resume).
  Engine packages and the three Java models are checked against pinned SHA-256 digests and sizes,
  the NVIDIA runtime against the digests in NVIDIA's release manifests (TensorRT and NVRTC are
  pinned), and the strongest network against the digest katagotraining.org reports; other weights
  from the online list are only checked against their download size (at least 1 MiB). A mismatch
  deletes the partial file and is reported. An engine package can also be imported from a zip
  obtained elsewhere (accepted only when it is the same trusted package), a weight file
  (`.bin.gz`/`.txt.gz`) can be imported,
  and an existing KataGo can be chosen. "验证" runs `katago version` (and, for NVIDIA drivers
  between 528.33 and 570.65, KataGo's one-second inference probe). "使用此配置…" shows the exact
  profile it will add (or update, for a profile this dialog created whose command is unchanged)
  and saves it only after confirming; profiles you configured yourself are never changed. On
  Windows the NVIDIA page installs the CUDA 12.8 / cuDNN 9 / TensorRT 10.9 runtime from NVIDIA
  after confirming (drivers are never installed). The official online weight list
  (katagotraining.org) can be refreshed; a failed refresh keeps the previous list. Selected-move
  focus is not supported by these releases; contribution is not provided or verified here.
  Network access follows Settings > 网络代理 (direct by default, system, or a manual HTTP proxy).
- Engine performance: MoreEngines has Java's thread policy (CFG uses the engine's config file;
  Benchmark uses the measured recommendation and sends `kata-set-param numSearchThreads` at start),
  性能测试 opening the performance page, 扫描 to build a command from chosen files (including the
  ikatago account form), and GTP 配置 for engines that publish a configuration schema (saved with
  the profile, the engine's config file is not edited). The official benchmark runs only when
  clicked, or after start-up when "首次启动时引导运行智能测速优化" is turned on (off by default);
  it can be stopped. Its recommended search threads stay in use until the policy is changed or
  measured again; the measured server threads and batch size apply only while the engine, model
  and config are unchanged, and the page then marks the result as historical. Measured tuning
  reports can be reviewed, applied and restored; they and Apple Silicon tuning apply only while
  the content of the engine, model and config still matches, which is checked before the engine
  starts without blocking the window. Apple Silicon deep tuning is experimental and asks first.

Every other Java menu item, toolbar button and shortcut is present in its Java place but marked
as not yet available (greyed in menus; a notice when used). Not available yet: comparison and
dual-engine analysis, remote/SSH and cloud engines, the quick-analysis and HumanSL models of the
weights page, and whole-game measured tuning.

Verified so far on Linux (X11, WebKitGTK) with KataGo 1.18.2 (Eigen CPU), including real downloads
of the Linux CPU package and the lightweight model. Windows and macOS builds, native file dialogs,
fonts, input methods, GPU backends, the NVIDIA runtime installer and engines on those platforms
have not been verified yet.

## Layout

| Path | Contents |
| --- | --- |
| `crates/lizzie-core` | Platform-independent core: game record model, SGF/GIB readers and SGF writer, browsing, settings, the LizzieYzy Next settings reader (`java_import`), the engine runtime (GTP framing, launch, analysis requests, rules, thread policy and benchmark), engine resources (`resources`: trusted package/model catalog, machine detection, recommendation, downloads, installs, verification), the credential sanitizer and the workbench command/view contract. No Tauri or DOM dependency. |
| `src-tauri` | Desktop shell: window, file dialogs, drag and drop, engine and helper processes, HTTP downloads, logging, diagnostics export, and the optional acceptance channel. |
| `ui` | Web UI: menus, toolbars, canvas painters, dialogs and the Java UI strings. |
| `fixtures` | Sample records used by tests and manual checks. |
| `tools` | Maintenance scripts. |

The core owns the open record. The UI only renders `WorkbenchView` projections and sends
`Command` values through one Tauri command (`dispatch_command`); menus, toolbar buttons,
shortcuts and automated acceptance all use the same commands. Views carry an increasing `seq`,
and an open runs in the background under a ticket so a late result from an older open is
dropped. Engine results carry the record, position and komi they were requested for; the core
drops any that no longer match and sends the engine only the moves that changed.

Record content changes only through the core's document edit operation, which raises the
document `revision`. A save captures the text, document and revision when it starts and is
written by one background writer in order; its receipt (`save_outcome`) confirms only that
revision of that document. `DocumentView` reports `revision`, `saved_revision` (the revision that
is the content of the document's file) and `unsaved`, so edits made while a save runs, or a
record opened meanwhile, stay unsaved.

## Development

Requirements: Rust (stable), Node.js 22+, and the [Tauri 2 system
prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
npm install
npx tauri dev                       # run with hot reload
npx tauri build --no-bundle         # release binary in target/release
npm run typecheck                   # TypeScript
cargo test --workspace              # core tests (records, browsing, saving, engine protocol, contract)
cargo clippy --workspace --all-targets
```

### Generated files

- `ui/src/contract/generated.ts` is generated from the Rust types. After changing them run
  `UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract`; the plain test fails while the
  file is out of date.
- `ui/src/i18n/javaStrings.json` holds the Java UI strings used by the UI, taken from the Java
  `l10n/DisplayStrings_*.properties` bundles. Regenerate it with
  `python3 tools/extract_java_strings.py <java-checkout>`; the script requires the checkout to be
  at the reference commit and lists keys that are shown literally.

### Acceptance channel

Builds with `--features acceptance` (for example
`npx tauri build --debug --no-bundle --features acceptance`) open a loopback-only control
channel when `LIZZIEYZY_TAURI_ACCEPTANCE_PORT` is set. It takes newline-delimited JSON:
`{"command": <Command>}` runs a command exactly as the UI does, `{"query": "view"}` returns the
current view without consuming anything the UI receives, and `{"query": "ui"}` returns what the
window shows (title, focus, open dialogs, status lines, tree scroll, last stone sound and the
positions of visible controls). With the feature, `LIZZIEYZY_TAURI_RESOURCE_BASE=<url>` makes
engine resource downloads fetch `<url>/<file name>` instead of the trusted source (digests and
sizes stay pinned), for isolated download tests. The feature is off by default; builds made
without `--features acceptance` contain neither, whatever the profile.

## Data

Settings (`config.json`, `persist.json`) live in the platform configuration directory for the
identifier `io.github.qiyi71w.lizzieyzy-tauri`; logs (`logs/app.log`, `logs/crash.log`) live in
the platform data directory for the same identifier. On Linux these are
`~/.config/io.github.qiyi71w.lizzieyzy-tauri/` and
`~/.local/share/io.github.qiyi71w.lizzieyzy-tauri/logs/`. Downloaded and imported engine resources
live under `resources/` in the same data directory (`engines/<package>/` with a receipt,
`weights/`, `configs/gtp.cfg` and `configs/analysis.cfg` (created once from the package's example
files and never overwritten), `runtime/nvidia/` on Windows, `downloads/` for package archives; an
interrupted download stays next to its target as `<name>.part` and resumes from there); "打开程序目录"
opens it. LizzieYzy Next keeps its own
`config.txt` and `persist` elsewhere (for example `~/.lizzieyzy-next/`); this client never writes
there. The first start is detected by the missing `config.json`; declining the import offer
writes it.

### Engine resource sources

The trusted catalog (`crates/lizzie-core/resources/catalog.json`) pins qualified official
KataGo release assets separately for each platform and backend: v1.18.2 for CUDA and v1.18.1
for the other upstream packages. The macOS Metal packages are self-contained builds made by
this project from the official v1.18.2 tag and published as release assets of this repository.
Each package records its own origin, source tag and commit, release, licenses, archive and
executable SHA-256 digests and sizes. Downloads and offline imports must match that identity;
installation checks the root executable and all required files and directories before activation.
Packages live in `engines/<id>-<archive-sha256-prefix>/`, so a new package does not replace one
an existing profile still references.

Official Linux executables are AppImages and require `/dev/fuse` plus `fusermount` or
`fusermount3` on PATH. Recommendations check each package's system libraries, known CUDA/cuDNN
library versions, minimum NVIDIA driver and CPU AVX2/FMA requirements. Unknown facts remain
unknown and explicit verification reports runtime failures. Windows NVIDIA runtime components
come from NVIDIA's redistributable archives: CUDA 12.8, cuDNN 9.8.0, NVRTC 12.8.61 and TensorRT
10.9.0.34 are pinned, with license files kept next to the installed libraries.

Analysis is supported. Selected-move focus is not supported by these releases. Official
non-ONNX builds include upstream distributed support, but this app does not verify contribution
eligibility or provide a contribution flow here; DirectML and OpenVINO builds do not support it.
Models remain the b11 network from katagotraining.org (KataGo Neural Network License) and the
two b10 networks from the KataGo v1.17.1 release. License references are shown per asset; official
packages do not need the `licenses/` folder or `source-package.json` that LizzieYzy Next packages carry.

## License

GPL-3.0-only (see `LICENSE.txt`). The images, stone sounds and the winrate font in `ui/public` are copied
unchanged from LizzieYzy Next (GPL-3.0), and the application icons are generated from its icon;
see `ui/public/assets/SOURCE.txt` and `ui/public/fonts/SOURCE.txt`.
