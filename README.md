# LizzieYzy Tauri

A desktop Go (weiqi) review workbench being migrated from the Java
[LizzieYzy Next](https://github.com/wimi321/lizzieyzy-next) (reference commit
`cb53acc55eda42c6c671b5f668cd3b597456bf07`) to Tauri 2 + Rust + TypeScript. The Java program's
behaviour, defaults and workbench layout are the reference; this client keeps its own identity
and data directory and never reads or writes the Java configuration.

## Current state

Implemented: opening and browsing records.

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
- Settings available now: reading komi from SGF records, stone sounds on/off, and the UI language
  (zh-CN, zh-TW, en, ja, ko, th; applied after a restart, like Java). A setting that cannot be
  saved is reported and left unchanged.
- Help > 诊断与日志 shows this client's logs, opens the log folder and exports a diagnostics zip;
  Help > 清除所有个人数据 clears the recent-record list; View > 重置界面位置 resets the saved
  window placement and last folder.

Every other Java menu item, toolbar button and shortcut is present in its Java place but marked
as not yet available (greyed in menus; a notice when used). There is no engine or analysis yet.

Verified so far on Linux (X11, WebKitGTK). Packages of both identities (below) are built for
Windows x64, macOS Apple Silicon and Intel, and Linux x64.

- Windows 11 x64: the acceptance-identity installer has been installed, upgraded in place and
  uninstalled, and the acceptance-identity portable zip extracted and started. The
  normal-identity installer has been installed next to the acceptance identity, both running at
  the same time, and uninstalled; the normal-identity portable zip extracted and started. All of
  them opened records from the command line, and both installed identities also through the
  native Open dialog.
- macOS 27 on an Apple Silicon Mac: the Apple Silicon apps of both identities started from their
  dmg and opened records; the acceptance identity also through the native Open panel. The Intel
  app has been started only under Rosetta. The Gatekeeper first-open steps in
  [docs/installing.md](docs/installing.md) were followed for a quarantined copy of the normal
  identity.
- Ubuntu 24.04 and 26.04 containers under a virtual X display: the deb packages of both
  identities have been installed side by side and removed; only the normal-identity deb was
  upgraded in place, on 24.04. The normal-identity AppImage has been run on Ubuntu 24.04.

Not verified yet: macOS 15, Intel Macs, full GNOME desktops, Windows 10, the Windows SmartScreen
prompt, and fonts and input methods on Windows and macOS.

## Layout

| Path | Contents |
| --- | --- |
| `crates/lizzie-core` | Platform-independent core: game record model, SGF/GIB readers, browsing, settings and the workbench command/view contract. No Tauri or DOM dependency. |
| `src-tauri` | Desktop shell: window, file dialogs, drag and drop, logging, diagnostics export, and the optional acceptance channel. |
| `ui` | Web UI: menus, toolbars, canvas painters, dialogs and the Java UI strings. |
| `fixtures` | Sample records used by tests and manual checks. |
| `tools` | Maintenance scripts. |

The core owns the open record. The UI only renders `WorkbenchView` projections and sends
`Command` values through one Tauri command (`dispatch_command`); menus, toolbar buttons,
shortcuts and automated acceptance all use the same commands. Views carry an increasing `seq`,
and an open runs in the background under a ticket so a late result from an older open is
dropped.

## Development

Requirements: Rust (stable), Node.js 22+, and the [Tauri 2 system
prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
npm install
npx tauri dev                       # run with hot reload
npx tauri build --no-bundle         # release binary in target/release
npm run typecheck                   # TypeScript
cargo test --workspace              # core tests (records, browsing, contract)
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

### Packages

`node tools/package.mjs` builds the packages of the committed checkout for the machine it runs
on and writes them to `target/packages`, with a `.manifest.json` (source commit, toolchain,
sizes) and a `.sha256` file per platform. It refuses a working tree with changes unless
`--allow-dirty` is given, which the manifest then records.

| Platform | Build on | Command | Assets |
| --- | --- | --- | --- |
| Windows x64 | Windows with the MSVC build tools | `node tools/package.mjs` | `…_windows-x64-setup.exe` (NSIS), `…_windows-x64-portable.zip` |
| macOS Apple Silicon | a Mac | `node tools/package.mjs --target aarch64-apple-darwin` | `…_macos-arm64.dmg` |
| macOS Intel | a Mac | `node tools/package.mjs --target x86_64-apple-darwin` | `…_macos-x64.dmg` |
| Linux x64 | any Linux with Docker | `tools/packaging/build-linux.sh` | `…_linux-x64.deb`, `…_linux-x64.AppImage` |

Asset names are `<binary>_<version>_<os>-<arch><form>`, for example
`lizzieyzy-tauri_0.1.0_windows-x64-setup.exe`. Linux packages are built in an Ubuntu 24.04
container (`tools/packaging/linux-builder.Dockerfile`), because a binary built on a newer
system needs that system's glibc and does not start on Ubuntu 24.04; the script packages the
committed `HEAD` only.

`--identity acceptance` builds the same program as *LizzieYzy Tauri Acceptance*: identifier
`io.github.qiyi71w.lizzieyzy-tauri.acceptance`, binary `lizzieyzy-tauri-acceptance`, its own
install location, package name and data directories, and the acceptance channel compiled in
(`src-tauri/tauri.acceptance.conf.json`). It installs next to the normal program without
sharing anything with it. Packages of the normal identity never contain the channel.

The installers and the macOS app carry no paid code-signing certificate: the Windows files are
unsigned and the macOS app is ad-hoc signed and not notarized. See
[docs/installing.md](docs/installing.md) for what users see on first start.

## Data

Settings (`config.json`, `persist.json`) live in the platform configuration directory for the
identifier `io.github.qiyi71w.lizzieyzy-tauri`; logs (`logs/app.log`, `logs/crash.log`) live in
the platform data directory for the same identifier. On Linux these are
`~/.config/io.github.qiyi71w.lizzieyzy-tauri/` and
`~/.local/share/io.github.qiyi71w.lizzieyzy-tauri/logs/`; on Windows both are under
`%APPDATA%\io.github.qiyi71w.lizzieyzy-tauri\`. A failure before the window opens (for example
a missing WebView2 Runtime) is also written to `crash.log` and, on Windows, shown in a message
box.

## License

GPL-3.0-only (see `LICENSE.txt`). The images, stone sounds and the winrate font in `ui/public` are copied
unchanged from LizzieYzy Next (GPL-3.0), and the application icons are generated from its icon;
see `ui/public/assets/SOURCE.txt` and `ui/public/fonts/SOURCE.txt`.
