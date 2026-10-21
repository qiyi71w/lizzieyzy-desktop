# Installing LizzieYzy Tauri

LizzieYzy Tauri is distributed as one package per system, architecture and form. It installs
next to LizzieYzy Next (Java) and never reads, changes or deletes the Java program's settings,
and it does not register itself for `.sgf` or `.gib` files.

| System | Package |
| --- | --- |
| Windows 10 22H2 / 11, x64 | `lizzieyzy-tauri_<version>_windows-x64-setup.exe` (installer) or `lizzieyzy-tauri_<version>_windows-x64-portable.zip` (no installation) |
| macOS 15 or later, Apple Silicon | `lizzieyzy-tauri_<version>_macos-arm64.dmg` |
| macOS 15 or later, Intel | `lizzieyzy-tauri_<version>_macos-x64.dmg` |
| Ubuntu 24.04 / 26.04, x64 | `lizzieyzy-tauri_<version>_linux-x64.deb` or `lizzieyzy-tauri_<version>_linux-x64.AppImage` |

## Check the download

Every set of packages comes with a `.sha256` file listing the SHA-256 of each package. Compare it
with the file you downloaded before opening it:

- Windows (PowerShell): `Get-FileHash .\lizzieyzy-tauri_<version>_windows-x64-setup.exe`
- macOS: `shasum -a 256 lizzieyzy-tauri_<version>_macos-arm64.dmg`
- Linux: `sha256sum -c lizzieyzy-tauri_<version>_linux-x64.sha256`

The packages are not signed with a paid code-signing certificate, so the system cannot tell you
who published them; download them only from the project's GitHub Releases and check the hash.

## Windows

**Installer.** Run the `-setup.exe`. It installs for the current user only and needs no
administrator rights; the default folder is `%LOCALAPPDATA%\LizzieYzy Tauri` and you can choose
another one. If the Microsoft Edge WebView2 Runtime is missing, the installer downloads and
installs it. Installing a newer version over an existing one updates it in place. Uninstall it
from *Settings > Apps*; your settings and logs are kept unless you tick the option to delete
the application data.

**Portable zip.** Extract the zip and run `lizzieyzy-tauri.exe` in the `LizzieYzy Tauri`
folder. Nothing is installed, but settings and logs are still kept in your user profile (see
below), not next to the program. The portable program needs the Microsoft Edge WebView2
Runtime, which Windows 11 includes and Windows 10 normally receives with Microsoft Edge. If it
is missing, the program says so in a message box, writes the error to its `crash.log` and
exits; install the *Evergreen* runtime from
<https://developer.microsoft.com/microsoft-edge/webview2/> and start it again.

**First start.** Because the files are not code-signed, Windows Defender SmartScreen can show
"Windows protected your PC" for a downloaded installer or program. Click *More info*, check
that the publisher is shown as unknown and that the file is the one whose hash you verified,
then click *Run anyway*. Do not turn SmartScreen off.

## macOS

Open the `.dmg` for your Mac's processor (Apple menu > *About This Mac*: "Apple M…" is Apple
Silicon, "Intel" is Intel) and drag *LizzieYzy Tauri* to *Applications*.

The app is ad-hoc signed and not notarized by Apple, so Gatekeeper blocks the first start of a
downloaded copy:

1. Open the app. macOS says it was not opened because Apple cannot verify that it is free of
   malware. Click *Done* (not *Move to Trash*).
2. Open *System Settings > Privacy & Security* and scroll to *Security*. Next to the message that
   *LizzieYzy Tauri.app* was blocked, click *Open Anyway*.
3. In the dialog that follows click *Open Anyway* again, then confirm with Touch ID or an
   administrator password.

The app then starts, and later starts open it without asking. This allows only this copy of the
app; do not turn Gatekeeper off. These steps were checked on macOS 27 on Apple Silicon; they have
not yet been checked on macOS 15 or on an Intel Mac.

## Linux

**deb (Ubuntu).** `sudo apt install ./lizzieyzy-tauri_<version>_linux-x64.deb` installs the
program and the WebKitGTK and GTK libraries it needs; it then appears in the application menu as
*LizzieYzy Tauri* and as the command `lizzieyzy-tauri`. Installing a newer `.deb` upgrades the
package `lizzie-yzy-tauri` in place; `sudo apt remove lizzie-yzy-tauri` removes the program and
keeps your settings.

**AppImage.** Make the file executable (`chmod +x`) and run it. It contains its own WebKitGTK
and uses the system's GTK, Wayland and graphics libraries. It needs FUSE to mount itself
(`fusermount3`, from the `fuse3` package, which Ubuntu desktops include). Without FUSE it
prints "Cannot mount AppImage"; run it with `--appimage-extract-and-run` instead.

Packages are built on Ubuntu 24.04 and need at least its system libraries (glibc 2.39).

## Where your data is

Settings (`config.json`, `persist.json`) and logs (`logs/app.log`, `logs/crash.log`) are kept per
user, in folders named after the application identifier `io.github.qiyi71w.lizzieyzy-tauri`:

| System | Settings | Logs |
| --- | --- | --- |
| Windows | `%APPDATA%\io.github.qiyi71w.lizzieyzy-tauri` | `%APPDATA%\io.github.qiyi71w.lizzieyzy-tauri\logs` |
| macOS | `~/Library/Application Support/io.github.qiyi71w.lizzieyzy-tauri` | same folder, `logs` |
| Linux | `~/.config/io.github.qiyi71w.lizzieyzy-tauri` | `~/.local/share/io.github.qiyi71w.lizzieyzy-tauri/logs` |

The installer and the portable zip use the same folders. Uninstalling or deleting the program
does not delete them. Help > 诊断与日志 shows the log files of the running program.

Test builds named *LizzieYzy Tauri Acceptance* (`lizzieyzy-tauri-acceptance_…`) are for automated
testing. They use the identifier `io.github.qiyi71w.lizzieyzy-tauri.acceptance`, their own
install folder, package and data folders, and do not touch the normal program's data.
