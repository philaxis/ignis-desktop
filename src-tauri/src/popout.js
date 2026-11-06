// Turns Obsidian's pop-out windows back on in an Ignis page.
//
// Ignis turns them off in three ways: `Platform.canPopoutWindow` is forced to
// false, `Workspace.prototype.{openPopout,openPopoutLeaf,moveLeafToPopout}`
// are replaced by functions that show a notice, and `window.open` hands every
// `about:blank` / `popup` request to a hidden iframe. This script undoes the
// first two and routes the third to a real window.
//
// Runs in every Ignis window before the page's own scripts. It does nothing
// in a pop-out's own document.
(() => {
  if (window.top !== window || window.opener || location.protocol === 'about:') return;

  // The real `window.open`, before Ignis wraps it.
  const nativeOpen = window.open;
  const PATCHED = ['openPopout', 'openPopoutLeaf', 'moveLeafToPopout'];

  // A pop-out is an `about:blank` document that the opener fills in. Give it
  // what Obsidian's `h0()` expects to find on a window it adopts: `require`
  // (so it can reach `electron`) and an `electronWindow` that describes this
  // window instead of the main one.
  function preparePopup(popup) {
    popup.require = window.require;
    popup.module = window.module;
    popup.Buffer = window.Buffer;
    popup.process = window.process;
    popup.global = popup;

    const mainWindow = window.require('@electron/remote').getCurrentWindow();
    const popupWindow = Object.create(mainWindow, {
      isFocused: { value: () => popup.document.hasFocus() },
      isMaximized: { value: () => false },
      isMinimized: { value: () => false },
      isFullScreen: { value: () => !!popup.document.fullscreenElement },
      getBounds: {
        value: () => ({
          x: popup.screenX,
          y: popup.screenY,
          width: popup.outerWidth,
          height: popup.outerHeight,
        }),
      },
      focus: { value: () => popup.focus() },
      close: { value: () => popup.close() },
      show: { value: () => {} },
      hide: { value: () => {} },
      setMinimumSize: { value: () => {} },
      setFrameZoomLevel: { value: () => {} },
    });
    // Obsidian assigns the main window's object after this; keep ours.
    Object.defineProperty(popup, 'electronWindow', {
      configurable: true,
      get: () => popupWindow,
      set: () => {},
    });

    // Obsidian sets `.is-focused` from `electronWindow.isFocused()` whenever
    // this event fires. Electron sends it; a browser window does not.
    const notify = () => popup.dispatchEvent(new popup.Event('focuschange'));
    popup.addEventListener('focus', notify);
    popup.addEventListener('blur', notify);
  }

  // Obsidian adds `background=<color>` for Electron. Chromium reads the same
  // key as "open in the background" and then returns null instead of the
  // window, so it is removed here and applied to the document instead.
  // In a frameless window the pop-out has no title bar either. Run the same
  // frame script in the pop-out's document, with a transport that goes through
  // this (main) page, because the pop-out itself cannot call app commands.
  function drawFrame(popup) {
    if (!window.__ignisFrameSource || !window.__TAURI_INTERNALS__) return;
    popup.__ignisFrameInvoke = (command, args) => {
      if (command === 'frame_snap_rect') return undefined;
      const action = command.replace(/^frame_/, '').replace(/_/g, '-');
      return window.__TAURI_INTERNALS__.invoke('popout_frame', { action });
    };
    popup.eval(window.__ignisFrameSource);
    // Empty space in the tab header moves the pop-out; a double click
    // maximizes or restores it.
    const interactive =
      '.workspace-tab-header, .workspace-tab-header-new-tab, .workspace-tab-header-tab-list, .titlebar-button, .ignis-frame-button, button, a, input, select, textarea';
    popup.addEventListener(
      'mousedown',
      (event) => {
        const target = event.target;
        if (event.button !== 0 || !target || !target.closest) return;
        if (target.closest(interactive) || !target.closest('.workspace-tab-header-container, .titlebar')) return;
        event.preventDefault();
        popup.__ignisFrameInvoke(event.detail === 2 ? 'frame_toggle_maximize' : 'frame_start_dragging');
      },
      true,
    );
  }

  function popupOpen(url, target, features) {
    let background;
    const withoutBackground = String(features || '')
      .split(',')
      .filter((part) => {
        const [key, value] = part.split('=');
        if (key.trim() !== 'background') return true;
        background = value;
        return false;
      })
      .join(',');
    const popup = nativeOpen.call(window, url, target, withoutBackground);
    if (!popup) return popup;
    preparePopup(popup);
    drawFrame(popup);
    if (background) popup.document.documentElement.style.background = background;
    return popup;
  }

  // Obsidian calls `window.open('about:blank', ...)` from inside
  // `Workspace.openPopout`. Swap in the real one for exactly that call.
  function wrapOpenPopout(original) {
    return function (...args) {
      const previous = window.open;
      window.open = popupOpen;
      try {
        return original.apply(this, args);
      } finally {
        window.open = previous;
      }
    };
  }

  // Ignis captures the `obsidian` module in `window.__ignisCapturedObsidian`
  // and, right after, overwrites the pop-out methods. Take the originals
  // first, ignore that overwrite, then put `canPopoutWindow` back.
  function onObsidianModule(obsidian) {
    const { Workspace, Platform } = obsidian;
    if (!Workspace || !Platform) return;
    const proto = Workspace.prototype;
    const canPopout = Object.getOwnPropertyDescriptor(Platform, 'canPopoutWindow');
    const methods = {};
    for (const name of PATCHED) methods[name] = proto[name];
    methods.openPopout = wrapOpenPopout(methods.openPopout);

    let restoreQueued = false;
    const restore = () => {
      if (canPopout) Object.defineProperty(Platform, 'canPopoutWindow', canPopout);
      else delete Platform.canPopoutWindow;
    };
    for (const name of PATCHED) {
      Object.defineProperty(proto, name, {
        configurable: true,
        get: () => methods[name],
        set: () => {
          if (restoreQueued) return;
          restoreQueued = true;
          queueMicrotask(restore);
        },
      });
    }
  }

  let captured;
  Object.defineProperty(window, '__ignisCapturedObsidian', {
    configurable: true,
    get: () => captured,
    set: (value) => {
      captured = value;
      try {
        onObsidianModule(value);
      } catch (e) {
        console.error('[ignis-desktop] pop-out support not installed:', e);
      }
    },
  });
})();
