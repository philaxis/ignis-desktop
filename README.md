# Ignis Desktop

> **Unofficial — not affiliated with Ignis or Obsidian.**

**A native desktop app for your Ignis server — real OS windows, as many as you want, with every shortcut going to Obsidian, not the browser.**

[Ignis](https://github.com/Nystik-gh/ignis) runs Obsidian on a server and serves it to a web browser. Ignis Desktop is not a browser tab and not a PWA: it is a small desktop application (built with [Tauri](https://tauri.app)) that opens your Ignis server in its own native windows. No address bar, no tabs, no browser chrome. Open as many windows as you like, put them on different monitors, and every key you press (`Ctrl+P`, `Ctrl+W`, `Ctrl+F`, `Ctrl+Tab`, `F5`, ...) is delivered to the page instead of being handled by the browser.

![Two Ignis Desktop windows on the same vault, with different workspaces. An edit made in one window shows up in the other.](docs/screenshots/two-windows.png)

*Two windows, two Ignis workspaces, one vault. The text was typed in the lower window; the upper one picked it up within a second.*

## Why use it instead of a browser tab

| | Browser tab / PWA | Ignis Desktop |
| --- | --- | --- |
| Windows | One tab per view, or one PWA window. Another window means a duplicate of the whole page. | Any number of native windows. Each one has its own Ignis workspace layout, remembered between runs. |
| Shortcuts | The browser handles some keys first: `Ctrl+W` closes the tab, `Ctrl+P` prints, `Ctrl+F` opens the browser's find bar, `Ctrl+R` reloads. | Browser shortcuts are switched off. Obsidian gets the keys, so `Ctrl+P` opens the command palette. |
| Address bar and tabs | Always there (a PWA hides them, but a browser still owns the window). | None. The window is just Ignis. |
| Memory | Shares a browser process with every other tab and extension. | Not measured. It runs only Ignis, with no extensions and no other tabs, but no benchmark backs a number yet. |

## Features

- **Multiple native windows.** `Ctrl+Shift+N` or the tray menu opens another window on the same vault. Anything the page opens with `window.open` or `target=_blank` becomes a native window too.
- **One workspace per window.** The first window loads `?vault=<vault>`. Every further window loads `?vault=<vault>&workspace=<id>`, so the windows do not overwrite each other's layout. Ignis syncs edits between windows within about a second.
- **Links do the right thing.** A request for a new window that points at a note opens a native window on that note (`&file=<path>`). Links to other sites open in your system browser.
- **Your keys, not the browser's.** Only `Ctrl+Shift+N` is handled by the app. Everything else goes to the page. On Windows the WebView2 accelerator keys are turned off (`AreBrowserAcceleratorKeysEnabled = false`).
- **Remembers your windows.** Size, position and maximized state are stored per window, and the windows that were open when you quit come back on the next start.
- **Shared data.** All windows use one webview data folder, so settings stored by the page (for example your plugin trust decision) apply to every window.
- **Tray icon** with *New window*, *Change server address…* and *Quit*.
- **Drag and drop** events are passed to the page, so Ignis's file upload by drop is not intercepted by the app.

## Install (Windows)

1. Download the latest `.msi` (or the `.exe` installer) from the [Releases](../../releases) page.
2. Run it. The installers are not code-signed yet, so Windows SmartScreen may ask you to confirm ("More info", then "Run anyway").
3. Start **Ignis Desktop** from the Start menu.

Ignis Desktop uses the Microsoft Edge WebView2 runtime, which is preinstalled on Windows 11 and on current Windows 10.

## First run

The first start shows a small window.

![The first-run window: server address and vault.](docs/screenshots/first-run.png)

It asks for:

- **Server address**, for example `https://ignis.example.ts.net`. If you leave out the scheme, `https://` is assumed.
- **Vault** (optional), the vault name as it appears in Ignis's `?vault=` URL parameter. Leave it empty to pick a vault inside Ignis.

Press *Save and open*. The settings are stored in `config.json`:

| OS | Location |
| --- | --- |
| Windows | `%APPDATA%\dev.philaxis.ignis-desktop\config.json` |
| Linux | `~/.config/dev.philaxis.ignis-desktop/config.json` |

To change the address later, use *Change server address…* in the tray menu. Open windows are pointed at the new server.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+N` | New window (with its own workspace). Works while the Ignis page has focus. |
| Anything else | Sent to the page, so it is whatever Obsidian binds it to. |

Closing a window forgets it; closing the last window quits the app (it does not stay in the tray). Use the tray's *Quit* to exit and have all currently open windows restored on the next start.

Each window id (`main`, `w2`, `w3`, ...) maps to a fixed Ignis workspace. A new window takes the lowest unused id, so a closed window's layout comes back when you open a new one. Ignis stores these layouts in the vault as `.obsidian/workspace.<id>.json`.

## What your Ignis server needs

- **HTTPS.** Ignis has no authentication of its own and, outside a secure context, browsers (and WebView2) disable features it needs. `http://localhost` and `http://127.0.0.1` count as secure, so a server on the same machine works over plain HTTP; anything else should use HTTPS.
- **Authentication in front of it.** Ignis does not provide any. [Tailscale](https://tailscale.com) is a convenient way to get HTTPS and a private network at once. On the machine that runs Ignis (listening on port 8080):

  ```sh
  tailscale serve --bg 8080
  ```

  Tailscale prints the resulting address, something like `https://ignis.example.ts.net`. Enter that in Ignis Desktop on any device in your tailnet.
- If a login page or single sign-on sits in front of Ignis, it loads inside the window like any other page.

## Limitations

- **Windows is the target.** The code builds and runs on Linux (that is how it is developed), but there is no Linux or macOS installer, and macOS is untested. On Linux, WebKitGTK's own shortcuts are not changed.
- **Unsigned installers.** Expect a SmartScreen prompt.
- **No Ignis pop-out windows.** Ignis opens notes as tabs and has no pop-out windows. Ignis Desktop gives you more windows, each with its own workspace, rather than moving a tab into a new window.
- **`Ctrl+Shift+N` needs the page.** It is handled by a small script inside the page. If Ignis is not reachable and the window shows an error page, use the tray menu instead.
- **Page-initiated navigation is not restricted.** A plain link to another site that is not opened as a new window navigates inside the window, which keeps login redirects working.
- **No auto-update.** Download new versions from the Releases page.
- **Everything that Ignis does not support still does not work**, such as plugins that need Node. See the [Ignis documentation](https://ignis.thiefling.com/docs/).

## Build from source

You need [Rust](https://rustup.rs) (stable), Node.js 22 and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS. On Debian/Ubuntu that means:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libssl-dev
```

Then:

```sh
npm ci
npm run tauri dev                    # run with a debug build
npm run tauri build -- --bundles msi # Windows installer (needs WiX, fetched automatically)
cargo test --manifest-path src-tauri/Cargo.toml   # unit tests
```

Releases are built by GitHub Actions: pushing a tag such as `v0.1.0` runs [`release.yml`](.github/workflows/release.yml), which builds the Windows installers and attaches them to a draft release. The workflow can also be started by hand to get the installers as a build artifact.

### How it is put together

| Path | What it does |
| --- | --- |
| `src-tauri/src/urls.rs` | Server address normalising, window URL building, and deciding what a `window.open` request becomes. |
| `src-tauri/src/config.rs` | `config.json` (server, vault, open windows and their geometry), window ids. |
| `src-tauri/src/windows.rs` | Window creation, geometry tracking and restore, new-window handling, WebView2 shortcut setting. |
| `src-tauri/src/main.rs` | App setup, tray menu, the two commands used by the settings page. |
| `ui/index.html` | The local first-run page. Ignis windows get no access to the app's IPC. |

## License

[MIT](LICENSE). Ignis Desktop contains no Ignis or Obsidian code: it only loads the URL of a server you run yourself.

## Unofficial

Ignis Desktop is an independent project. It is not affiliated with, endorsed by, or sponsored by Ignis, its authors, or Obsidian / Dynalist Inc. "Obsidian" is a trademark of its owner and is mentioned here only to describe what Ignis serves.
