# Draft for Ignis: pop-out windows in the browser

Draft only. Nothing here has been posted to [Nystik-gh/ignis](https://github.com/Nystik-gh/ignis). It is written so it can be pasted into an issue.

Checked against Ignis 0.9.0 and Obsidian 1.14.4 (the versions in the `nobbe/ignis:latest` image on 2026-11-07).

## Summary

`docs/using/limitations.md` says pop-out windows "rely on Electron's window APIs". In Obsidian 1.14.4 they rely on much less: a `window.open('about:blank', …)` call whose returned `window` Obsidian fills in itself, plus a few Electron-shaped objects that Ignis already provides. With four changes to the shim, pop-outs open, edit, save, close and merge back in a plain Chromium tab. Ignis Desktop (a Tauri wrapper) ships those changes as an init script today; this note is about moving them upstream so that browsers get them too.

## How Obsidian opens a pop-out

`Workspace.openPopout()` creates a `WorkspaceWindow`. Its constructor calls an internal `p0()`:

```js
const s = window.open('about:blank', '_blank',
  'popup,width=…,height=…,x=…,y=…,background=#rrggbb');   // 'parented=1' when full screen
if (!s) return null;                                       // → "Unable to open a new window."
s.document.head.appendChild(createEl('base', { href: location.href }));
w0(s);   // s.eval(window.globalEnhance.toString())
h0(s);   // adopts the window: see below
jM(s.document); woe(s); b0(s); gb(s.document); m0(s);
y0(s, [base]);   // copies <style>/<link> from the opener and keeps them in sync
```

The opener builds the whole document, so the pop-out only works if the object returned by `window.open` is the live window.

`h0(s)` climbs to the top window's `require`, so the pop-out needs `s.require`. If it has one it sets `s.electron = require('electron')` and `s.electronWindow = electron.remote.getCurrentWindow()`. Later code uses `electronWindow` for `isFocused()`, `getBounds()`, `isMaximized()`, `setFrameZoomLevel()`, `show()`, `hide()` and `setMinimumSize()`. The pop-out's `beforeunload` handler calls `electron.ipcRenderer.sendSync('is-quitting')` and, if that is false, closes the window object in the layout (detaches its tabs).

Ignis already provides `require`, `electron`, `@electron/remote` and an `is-quitting` handler (always `false`).

## What Ignis does today

| Where | What |
| --- | --- |
| `shim/src/guards/popout-guard.js`, `installPopoutOverrides()` | Forces `Platform.canPopoutWindow` to `false`. Replaces `Workspace.prototype.openPopout`, `openPopoutLeaf` and `moveLeafToPopout` with functions that show a notice. |
| `shim/src/guards/popout-guard.js`, `initPopoutGuard()` | Removes `floating` (the pop-outs) from layout files on read and puts the old value back on write. |
| `shim/src/globals/window.js`, `installWindowOpen()` | Turns every `window.open('about:blank', …)` and every call with `popup` in the features into a hidden, off-screen `<iframe>`. This exists for PDF export and graph export, which use the same call. |

## What had to change

All four were needed. Without any one of them the pop-out fails or is broken in a visible way.

1. **Do not replace the pop-out methods, and leave `Platform.canPopoutWindow` alone.** The originals work. (Ignis Desktop captures them before Ignis replaces them, then puts them back. A flag in `installPopoutOverrides` would be simpler for upstream.)
2. **Send the pop-out's `window.open` to the real `window.open`.** `installWindowOpen()` cannot tell a pop-out from a print or export popup by its URL or features. A reliable marker is the call itself: the pop-out is opened synchronously inside `Workspace.openPopout`. Wrapping that method and swapping `window.open` for exactly that call is enough. PDF export passes `popup,hide=true`; graph export passes `popup,x=…,y=…,width=…,height=…` without `background`.
3. **Remove `background=<colour>` from the features before calling the browser.** Obsidian adds it for Electron. Chromium reads `background` as "open in the background" and returns `null` from `window.open`, so Obsidian throws "Unable to open a new window." Apply the colour to `popup.document.documentElement.style.background` instead.
4. **Give the new window what `h0()` looks for.** Set `popup.require`, `module`, `Buffer` and `process` to the opener's (the iframe code already does this) before returning it. For correct focus styling, also give the popup its own `electronWindow` (`isFocused()` from `popup.document.hasFocus()`, `getBounds()` from `screenX/outerWidth`, `close()` → `popup.close()`) and dispatch a `focuschange` event on the popup when it gets or loses focus. Obsidian toggles `.is-focused` from that event. Electron sends it; browsers do not.

A sketch of the page side, which is what Ignis Desktop runs (the actual script is `src-tauri/src/popout.js` in this repository):

```js
const nativeOpen = window.open;
function popupOpen(url, target, features) {
  const kept = features.split(',').filter(p => p.split('=')[0].trim() !== 'background').join(',');
  const popup = nativeOpen.call(window, url, target, kept);
  if (popup) { popup.require = window.require; /* module, Buffer, process, electronWindow, focuschange */ }
  return popup;
}
Workspace.prototype.openPopout = function (...args) {
  const previous = window.open;
  window.open = popupOpen;
  try { return originalOpenPopout.apply(this, args); } finally { window.open = previous; }
};
```

## What I tested

Chromium (Puppeteer, popups allowed, user gesture supplied through `Runtime.evaluate`), with the changes above:

- `Workspace.moveLeafToPopout()` and `Workspace.openPopoutLeaf()` (what the "Move current tab to new window" and "Open current tab in new window" commands call) open a window with the note, the right title and Obsidian's styling.
- Typing in the pop-out saves to the vault and shows up in the main window.
- Closing the pop-out window (`beforeunload` run) removes its tab from the layout.
- Moving the tab back into the main window, the way Obsidian's drop handler does it, closes the empty pop-out by itself.
- `Ctrl+E` in the pop-out reaches Obsidian's hotkeys (Obsidian relays key events from the pop-out to the main window).

Also checked in the Ignis Desktop app (Tauri 2) on Linux (WebKitGTK) and Windows (WebView2): see the pull request.

## Things upstream would have to decide

- **Browsers block `window.open` without a user gesture.** Opening a pop-out from a command or a click is fine; Obsidian's *restore pop-outs on start* is not. That is probably why `initPopoutGuard()` strips `floating` on read. If pop-outs are enabled, keep stripping it, or restore the pop-outs after the first click.
- **The layout transform and new pop-outs.** `restoreLayoutFile()` writes back the `floating` value that was on disk when the layout was read. With pop-outs on, that overwrites the pop-outs of the current session. The transform should be dropped, or limited to the pop-outs that were not opened in this session.
- **Print and export popups keep using the iframe path.** That path should stay as it is.
- **Not tested:** Firefox, Safari, mobile browsers, touch drag of a tab between windows, and windows that are closed by the browser rather than by Obsidian (for example the browser's own close button on a pop-out in a tab-based browser).
