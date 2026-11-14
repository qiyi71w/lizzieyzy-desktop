# Updates

*Help > 检查更新* checks for a newer version, downloads it and installs it, with the options of
LizzieYzy Next. This page describes how that works and how a maintainer publishes a release the
program can update to.

## In the program

The 检查更新 page shows the current version and these options. They are saved at once, and the
Java settings `update-channel`, `update-source`, `auto-check-update` and `auto-download-update`
are imported:

| Option | Meaning |
| --- | --- |
| 更新通道 正式 / 测试 | 正式 offers only releases; 测试 also offers test releases and takes whichever verified one is newer. |
| 更新源 官网 / GitHub | Where 正式 reads its release list. The project has no site of its own yet, so both read GitHub Releases. 测试 always reads GitHub. |
| 启动时自动检查更新 | Off by default. Checks once after start and shows a found update without taking focus. |
| 自动下载更新 | Off by default; only with the option above. Downloads and verifies a found update in the background, then shows it. It never installs. |

Turning an automatic option off stops it at once; turning it on takes effect at the next start
(the option reads "下次启动生效" until then). Opening the page does not use the network.

检查更新 reads the release list. When a newer version has a package for this installation, the
发现新版本 window shows it. Nothing is installed until you click 立即更新, which downloads the
package unless 自动下载更新 already did (暂停 / 继续 / 取消; a paused or cancelled download
continues where it stopped), verifies its signature, and installs it:

| Installation | What 立即更新 does |
| --- | --- |
| Windows installer (`-setup.exe`) | Closes the program normally (saving as on any exit), runs the new installer, which reopens the program. |
| macOS app | Replaces the app bundle and restarts the program. |
| Linux AppImage | Replaces the AppImage file and restarts the program. |
| Windows portable zip, Linux deb | Not updated from within the program: the check says this installation cannot update itself. Download the new package from GitHub Releases. |

A package that fails verification is deleted and never installed. A failed installation leaves
the installed program as it was and says so; if the Windows installer cannot start after the
program has closed, the program opens again with that message, and 检查更新 → 立即更新 tries
again with the package already downloaded. Updates never touch settings, logs, saved games,
engines or weights.

Downloads use the proxy set in the program's network settings. The package is kept in the
program's cache folder (`updates/<version>`); downloading another version removes it.

## Update signing

Each package the program installs carries a signature made with the project's update key
(the Tauri updater's minisign key). The program contains the public key and checks the signature
after downloading and again before installing; the signature also binds the package to the
version it was published as, so an old signed package cannot be offered as a newer version.

This key is separate from operating system code signing: Windows packages are not code-signed
and macOS apps are ad-hoc signed (see [Installing](installing.md)), so the system's first-start
prompts for a downloaded package are unchanged.

A build without the update public key cannot check for updates and says so; the current
published builds have no update key yet.

## Publishing a release (maintainers)

### The update key

Create the key once, on a trusted machine, with a strong password:

```sh
npx tauri signer generate -w ~/.lizzieyzy-tauri-update.key
```

This writes the private key (`.key`, encrypted with the password) and the public key
(`.key.pub`). Keep the private key and its password outside the repository, never in a log, an
issue or CI output, and keep an offline backup of both: a lost key means installed programs can
no longer be updated, and a leaked one lets anyone sign updates. Replacing the key needs a release,
signed with the old key, that contains the new public key.

The public key is not secret. It can be built in with `--update-pubkey`, or set as
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.

### Building signed packages

On each build machine, with the private key in the environment of the Tauri CLI:

```sh
export TAURI_SIGNING_PRIVATE_KEY_PATH=~/.lizzieyzy-tauri-update.key
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=...   # read it from a password manager, not shell history
node tools/package.mjs --update-pubkey ~/.lizzieyzy-tauri-update.key.pub --out target/packages
```

Besides the packages, this writes for each update package a `.sig` file and
`<prefix>.update.json` listing the update entries of this platform: `windows-x86_64-nsis`,
`darwin-aarch64-app` / `darwin-x86_64-app` (an extra `.app.tar.gz`), `linux-x86_64-appimage`.

For Linux, run `UPDATE_PUBKEY=~/.lizzieyzy-tauri-update.key.pub tools/packaging/build-linux.sh`
with the same two variables; the key files are mounted read-only into the build container.

### The release manifest

Collect the `.update.json` files of all platforms and write the manifest:

```sh
node tools/update-manifest.mjs --tag v0.2.0 --out lizzieyzy-tauri-update.json target/packages/*.update.json
```

Upload the packages, their `.sig` files and `lizzieyzy-tauri-update.json` to the GitHub release
with tag `v0.2.0`:

- **Release (正式):** a normal release (not marked pre-release). The program reads
  `releases/latest/download/lizzieyzy-tauri-update.json`, the manifest of the latest release.
- **Test release (测试):** write the manifest with `--prerelease` and mark the GitHub release as
  pre-release. Also upload the same manifest to the release tagged `channel-beta`, replacing the
  previous one: 测试 reads `releases/download/channel-beta/lizzieyzy-tauri-update.json`.

### Trying an update locally

Acceptance builds (`--identity acceptance`) read the release list from a local server instead of
GitHub when `LIZZIEYZY_TAURI_UPDATE_BASE` is set (`<base>/stable/` and `<base>/beta/` serve the
manifests), and can take a test public key from `LIZZIEYZY_TAURI_UPDATE_PUBKEY`. Use a separate
test key for this, never the release key; write test manifests with `--base-url` pointing at the
local server.
