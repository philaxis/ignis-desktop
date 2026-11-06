// Window frame for frameless Ignis Desktop windows. Injected before the page's
// own scripts, in every window that has no native title bar.
//
// * Obsidian's tab header is the title bar: empty space in the top tab headers
//   moves the window, a double click maximizes or restores it.
// * Minimize, maximize and close sit in Obsidian's own
//   `.titlebar-button-container.mod-right`; `--frame-right-space` keeps the
//   tab header clear of them.
// * While there is no tab header (page still loading, server offline, or an
//   error page from a proxy), a thin strip along the top does the same job.
//
// The page talks to the app through the `frame_*` commands. Tauri only lets
// the configured server's origin (and the app's own pages) call them.
(() => {
  'use strict';
  if (window.top !== window || window.__ignisFrame) return;
  window.__ignisFrame = true;

  const BUTTON_WIDTH = 40;
  const STRIP_HEIGHT = 28;
  const STYLE_ID = 'ignis-frame-style';
  const STRIP_ID = 'ignis-frame-strip';

  const HEADER = '.workspace-tabs.mod-top .workspace-tab-header-container';
  const BUTTON_HOST = '.titlebar-button-container.mod-right';
  // Parts of the tab header that do something on their own.
  const INTERACTIVE = [
    '.workspace-tab-header',
    '.workspace-tab-header-new-tab',
    '.workspace-tab-header-tab-list',
    '.sidebar-toggle-button',
    '.titlebar-button',
    '.ignis-frame-button',
    'button, a, input, select, textarea, [contenteditable="true"]',
  ].join(', ');
  const DRAG_AREA = `${HEADER}, .titlebar, #${STRIP_ID}`;

  const CSS = `
    #${STRIP_ID} {
      position: fixed; top: 0; left: 0; right: 0; height: ${STRIP_HEIGHT}px;
      z-index: 2147483647; display: flex; justify-content: flex-end;
      background: var(--titlebar-background, #f6f6f6);
      border-bottom: 1px solid var(--divider-color, rgba(128, 128, 128, 0.3));
      box-sizing: border-box; user-select: none; -webkit-user-select: none;
    }
    @media (prefers-color-scheme: dark) {
      #${STRIP_ID} { background: var(--titlebar-background, #1f1f1f); }
    }
    /* Obsidian's native frame stays usable under a modal; so do these. */
    body .titlebar { z-index: 2147483000 !important; }
    .ignis-frame-buttons { display: flex; height: 100%; }
    .ignis-frame-buttons > .ignis-frame-button.ignis-frame-button {
      width: ${BUTTON_WIDTH}px; height: 100%; padding: 0; box-sizing: border-box;
      display: flex; align-items: center; justify-content: center;
      color: var(--icon-color, #5c5c5c); cursor: default;
      user-select: none; -webkit-user-select: none;
    }
    #${STRIP_ID} .ignis-frame-button { color: var(--icon-color, #777); }
    .ignis-frame-buttons > .ignis-frame-button.ignis-frame-button:hover,
    .ignis-frame-buttons > .ignis-frame-button.ignis-frame-button.is-hover {
      color: var(--icon-color-hover, currentColor);
      background-color: var(--background-modifier-hover, rgba(128, 128, 128, 0.2));
    }
    .ignis-frame-buttons > .ignis-frame-button.mod-close:hover {
      color: #fff; background-color: var(--background-modifier-error, #e81123);
    }
    .ignis-frame-button svg {
      width: 10px; height: 10px; fill: none; stroke: currentColor;
      stroke-width: 1; pointer-events: none;
    }
    .ignis-frame-button .restore, .ignis-frame-buttons[data-maximized="true"] .maximize { display: none; }
    .ignis-frame-buttons[data-maximized="true"] .restore { display: block; }
  `;

  const ICONS = {
    minimize: '<svg viewBox="0 0 10 10"><path d="M0 5.5h10"/></svg>',
    maximize:
      '<svg class="maximize" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9"/></svg>' +
      '<svg class="restore" viewBox="0 0 10 10"><path d="M2.5 2.5v-2h7v7h-2M0.5 2.5h7v7h-7z"/></svg>',
    close: '<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10"/></svg>',
  };
  const LABELS = { minimize: 'Minimize', maximize: 'Maximize', close: 'Close' };
  const COMMANDS = {
    minimize: 'frame_minimize',
    maximize: 'frame_toggle_maximize',
    close: 'frame_close',
  };

  const invoke = (command, args) => {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals) return Promise.resolve(undefined);
    return Promise.resolve(internals.invoke(command, args)).catch((error) => {
      console.debug('ignis-desktop: ' + command + ' failed', error);
    });
  };

  let stateTimer = 0;
  const refreshState = () => {
    invoke('frame_state').then((state) => {
      if (state) buttons.dataset.maximized = state.maximized ? 'true' : 'false';
    });
  };
  const scheduleStateRefresh = () => {
    clearTimeout(stateTimer);
    stateTimer = setTimeout(refreshState, 100);
  };

  const buttons = document.createElement('div');
  buttons.className = 'ignis-frame-buttons';
  buttons.dataset.maximized = 'false';
  for (const action of ['minimize', 'maximize', 'close']) {
    const button = document.createElement('div');
    button.className = `titlebar-button ignis-frame-button mod-${action}`;
    button.setAttribute('aria-label', LABELS[action]);
    button.dataset.action = action;
    button.innerHTML = ICONS[action];
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      invoke(COMMANDS[action]).then(scheduleStateRefresh);
    });
    buttons.appendChild(button);
  }

  let strip = null;
  const ensureStrip = () => {
    if (!strip) {
      strip = document.createElement('div');
      strip.id = STRIP_ID;
    }
    if (!strip.isConnected) document.body.appendChild(strip);
    return strip;
  };

  // The tab header is the frame once Obsidian has drawn it. Collapsed
  // sidebars keep (hidden) headers of their own, so look for a visible one.
  const obsidianHost = () => {
    const host = document.querySelector(BUTTON_HOST);
    if (!host) return null;
    const headers = document.querySelectorAll(HEADER);
    return Array.from(headers).some((h) => h.getClientRects().length > 0) ? host : null;
  };

  // Windows 11 shows the Snap Layouts flyout over a maximize button only if
  // the native side answers the hit test there, so the native side puts a
  // window over the button (see snap.rs). That window gets the mouse instead
  // of the page, so it reports hover and presses back through __ignisFrameSnap.
  const WINDOWS = /Windows/.test(navigator.userAgent);
  const maximizeButton = buttons.querySelector('.mod-maximize');
  let reportedRect = '';
  const reportSnapRect = () => {
    if (!WINDOWS) return;
    let rect = null;
    if (maximizeButton.isConnected) {
      const box = maximizeButton.getBoundingClientRect();
      if (box.width > 0 && box.height > 0) {
        const scale = window.devicePixelRatio || 1;
        rect = {
          x: Math.round(box.left * scale),
          y: Math.round(box.top * scale),
          width: Math.round(box.width * scale),
          height: Math.round(box.height * scale),
        };
      }
    }
    const key = JSON.stringify(rect);
    if (key === reportedRect) return;
    reportedRect = key;
    invoke('frame_snap_rect', { rect });
  };
  window.__ignisFrameSnap = (kind) => {
    if (kind === 'enter') maximizeButton.classList.add('is-hover');
    if (kind === 'leave') maximizeButton.classList.remove('is-hover', 'is-pressed');
    if (kind === 'down') maximizeButton.classList.add('is-pressed');
    if (kind === 'up') {
      maximizeButton.classList.remove('is-pressed');
      scheduleStateRefresh();
    }
  };

  const update = () => {
    if (!document.body) return;
    if (!document.getElementById(STYLE_ID)) {
      const style = document.createElement('style');
      style.id = STYLE_ID;
      style.textContent = CSS;
      (document.head || document.documentElement).appendChild(style);
    }
    const host = obsidianHost();
    if (host) {
      if (strip) strip.remove();
      if (buttons.parentElement !== host) host.appendChild(buttons);
      document.body.style.setProperty('--frame-right-space', `${BUTTON_WIDTH * 3}px`, 'important');
    } else {
      document.body.style.removeProperty('--frame-right-space');
      const fallback = ensureStrip();
      if (buttons.parentElement !== fallback) fallback.appendChild(buttons);
    }
    reportSnapRect();
  };

  // Starting a drag hands the mouse to the window manager, which can swallow
  // the first click's release; the page's own click count is not reliable
  // then. Count two quick presses at nearly the same spot as a double click.
  const DOUBLE_CLICK_MS = 450;
  const DOUBLE_CLICK_PX = 6;
  let lastPress = null;

  window.addEventListener(
    'mousedown',
    (event) => {
      if (event.button !== 0 || !(event.target instanceof Element)) return;
      if (event.target.closest(INTERACTIVE) || !event.target.closest(DRAG_AREA)) return;
      event.preventDefault();
      const previous = lastPress;
      const isDouble =
        event.detail === 2 ||
        (previous !== null &&
          event.timeStamp - previous.time < DOUBLE_CLICK_MS &&
          Math.abs(event.screenX - previous.x) <= DOUBLE_CLICK_PX &&
          Math.abs(event.screenY - previous.y) <= DOUBLE_CLICK_PX);
      if (isDouble) {
        lastPress = null;
        invoke('frame_toggle_maximize').then(scheduleStateRefresh);
      } else {
        lastPress = { time: event.timeStamp, x: event.screenX, y: event.screenY };
        invoke('frame_start_dragging');
      }
    },
    true,
  );
  window.addEventListener('resize', () => {
    scheduleStateRefresh();
    reportSnapRect();
  });

  document.addEventListener('DOMContentLoaded', () => {
    update();
    refreshState();
  });
  setInterval(update, 250);
  update();
})();
