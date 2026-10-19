# LizzieYzy Tauri

A desktop Go (weiqi) review workbench being migrated from the Java
[LizzieYzy Next](https://github.com/wimi321/lizzieyzy-next) (reference commit
`cb53acc55eda42c6c671b5f668cd3b597456bf07`) to Tauri 2 + Rust + TypeScript. The Java program's
behaviour, defaults and workbench layout are the reference; this client keeps its own identity
and data directory and never reads or writes the Java configuration.

## Current state

Implemented: opening, browsing and saving records, and analysis with one local GTP engine
(KataGo, Leela Zero, Sayuri, Sai or Leela 0.11, recognised by their GTP name like Java).

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

Every other Java menu item, toolbar button and shortcut is present in its Java place but marked
as not yet available (greyed in menus; a notice when used). Not available yet: comparison and
dual-engine analysis, remote/SSH and cloud engines, GPU and thread recommendations, benchmark
and automatic engine setup.

Verified so far on Linux (X11, WebKitGTK) with KataGo 1.18.2 (Eigen CPU). Windows and macOS
builds, native file dialogs, fonts, input methods and engines on those platforms have not been
verified yet.

## Layout

| Path | Contents |
| --- | --- |
| `crates/lizzie-core` | Platform-independent core: game record model, SGF/GIB readers, browsing, settings, the engine runtime (GTP framing, launch, analysis requests, rules), the credential sanitizer and the workbench command/view contract. No Tauri or DOM dependency. |
| `src-tauri` | Desktop shell: window, file dialogs, drag and drop, engine processes, logging, diagnostics export, and the optional acceptance channel. |
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
positions of visible controls). The feature is off by default; builds made without
`--features acceptance` do not contain the channel, whatever the profile.

## Data

Settings (`config.json`, `persist.json`) live in the platform configuration directory for the
identifier `io.github.qiyi71w.lizzieyzy-tauri`; logs (`logs/app.log`, `logs/crash.log`) live in
the platform data directory for the same identifier. On Linux these are
`~/.config/io.github.qiyi71w.lizzieyzy-tauri/` and
`~/.local/share/io.github.qiyi71w.lizzieyzy-tauri/logs/`.

## License

GPL-3.0-only (see `LICENSE.txt`). The images, stone sounds and the winrate font in `ui/public` are copied
unchanged from LizzieYzy Next (GPL-3.0), and the application icons are generated from its icon;
see `ui/public/assets/SOURCE.txt` and `ui/public/fonts/SOURCE.txt`.
