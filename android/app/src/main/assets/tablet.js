// Runs in the Ignis page before any of its own scripts (document start).
//
// 1. Tablet layout. Ignis shows Obsidian's mobile layout only on screens
//    narrower than 600px: on wider screens its shim removes the
//    `EmulateMobile` flag before Obsidian reads it. Keeping the flag gives
//    Obsidian's own tablet layout (drawers, edge swipes, the editing toolbar).
// 2. Real insets. The `emulate-mobile` body class makes Obsidian pretend there
//    is a phone notch (a fixed 59px top inset). Ignis strips it for phones;
//    this does the same, so the WebView's real safe-area insets apply.
// 3. One layout per window. In the mobile layout Obsidian keeps its tabs in
//    `.obsidian/workspace-mobile.json`, and Ignis's `?workspace=` parameter
//    only redirects the desktop `workspace.json`. Without this every app
//    window (and every phone using Ignis) would share one layout. Reads and
//    writes of that file go to `workspace-mobile.<workspace>.json` instead.
// 4. New window. Obsidian's tablet layout has no "open in new window", so a
//    command "Open a new window" (and Ctrl+Shift+N on a keyboard) asks the
//    app for another window, which Android puts next to this one.
(() => {
  if (window.top !== window) return;

  const FLAG = 'EmulateMobile';
  localStorage.setItem(FLAG, 'true');
  const removeItem = Storage.prototype.removeItem;
  Storage.prototype.removeItem = function (key) {
    if (this === localStorage && key === FLAG) return;
    return removeItem.call(this, key);
  };

  const stripEmulateClass = () => {
    const body = document.body;
    if (body.classList.contains('emulate-mobile')) body.classList.remove('emulate-mobile');
    new MutationObserver(() => {
      if (body.classList.contains('emulate-mobile')) body.classList.remove('emulate-mobile');
    }).observe(body, { attributes: true, attributeFilter: ['class'] });
  };
  if (document.body) stripEmulateClass();
  else document.addEventListener('DOMContentLoaded', stripEmulateClass, { once: true });

  // The app turns a `window.open` of the server page into a new window.
  const openNewWindow = () => window.open(location.origin + location.pathname + '?ignis-app=new-window', '_blank');
  window.addEventListener('keydown', (event) => {
    const isN = event.code === 'KeyN' || event.key === 'n' || event.key === 'N';
    if (event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && isN) {
      event.preventDefault();
      event.stopImmediatePropagation();
      openNewWindow();
    }
  }, true);

  const workspace = new URLSearchParams(location.search).get('workspace') || '';
  const validWorkspace = /^[A-Za-z0-9_.-]{1,64}$/.test(workspace);

  const redirectWorkspaceFile = (app) => {
    const adapter = app && app.vault && app.vault.adapter;
    if (!validWorkspace || !adapter || adapter.__ignisTablet) return;
    adapter.__ignisTablet = true;
    const from = app.vault.configDir + '/workspace-mobile.json';
    const to = app.vault.configDir + '/workspace-mobile.' + workspace + '.json';
    const mapPath = (path) => (path === from ? to : path);
    for (const name of ['read', 'write', 'exists', 'stat', 'remove']) {
      const original = adapter[name];
      if (typeof original !== 'function') continue;
      adapter[name] = function (path, ...rest) {
        return original.call(this, mapPath(path), ...rest);
      };
    }
  };

  const addNewWindowCommand = (app) => {
    app.workspace.onLayoutReady(() => {
      app.commands.addCommand({ id: 'ignis-app:new-window', name: 'Open a new window', icon: 'lucide-app-window', callback: openNewWindow });
    });
  };

  // Obsidian creates its App with `window.app = new App(...)` and reads the
  // workspace file afterwards, so patching at assignment is early enough.
  let current;
  Object.defineProperty(window, 'app', {
    configurable: true,
    enumerable: true,
    get: () => current,
    set: (value) => {
      current = value;
      try {
        redirectWorkspaceFile(value);
      } catch (e) {
        console.error('[ignis-app] could not separate the window layout', e);
      }
      try {
        addNewWindowCommand(value);
      } catch (e) {
        console.error('[ignis-app] could not add the new-window command', e);
      }
    },
  });
})();
