//! Windows 11 Snap Layouts on the maximize button that `frame.js` draws.
//!
//! Windows offers the Snap Layouts flyout over a maximize button only when the
//! window under the cursor answers the non-client hit test with
//! `HTMAXBUTTON`. The page's pixels belong to the WebView2 content window,
//! which cannot answer that. So a small child window of the app window sits
//! over the page's button, never paints, and answers `HTMAXBUTTON`; a click on
//! it maximizes or restores the window. The page below it gets no mouse input
//! at all, so the sink reports enter, leave, down and up to the page, which
//! draws the hover and pressed states itself.
//!
//! The page measures its button and passes the rectangle (client pixels)
//! through the `frame_snap_rect` command whenever it changes. Other platforms
//! have no such flyout and do nothing.

#[cfg(not(windows))]
mod imp {
    use tauri::{Runtime, WebviewWindow};

    use super::SnapRect;

    pub fn place<R: Runtime>(
        _window: &WebviewWindow<R>,
        _rect: Option<SnapRect>,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
mod imp {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, LazyLock, Mutex, PoisonError,
        },
    };

    use tauri::{Runtime, WebviewWindow};
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
            Graphics::Gdi::{ScreenToClient, HBRUSH},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
                Input::KeyboardAndMouse::{
                    TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
                },
                Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
                WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, IsZoomed, RegisterClassExW, SetWindowPos,
                    ShowWindow, HCURSOR, HICON, HTMAXBUTTON, HTTRANSPARENT, HWND_TOP,
                    SM_CXPADDEDBORDER, SM_CYSIZEFRAME, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE,
                    SWP_NOSIZE, SWP_SHOWWINDOW, SW_MAXIMIZE, SW_RESTORE, WINDOW_EX_STYLE,
                    WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
                    WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NCRBUTTONDOWN,
                    WM_NCRBUTTONUP, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSEXW, WNDCLASS_STYLES,
                    WS_CHILD, WS_CLIPSIBLINGS,
                },
            },
        },
    };

    use super::SnapRect;

    const CLASS_NAME: PCWSTR = w!("IGNIS_DESKTOP_SNAP_BUTTON");
    /// Id of the subclass on the app window that keeps the sink on top.
    const SUBCLASS_ID: usize = 0x6967_6e73;

    /// What the mouse does over the sink; the page draws the matching state.
    #[derive(Clone, Copy)]
    enum Event {
        Enter,
        Leave,
        Down,
        Up,
    }

    impl Event {
        fn name(self) -> &'static str {
            match self {
                Event::Enter => "enter",
                Event::Leave => "leave",
                Event::Down => "down",
                Event::Up => "up",
            }
        }
    }

    struct Sink {
        parent: HWND,
        notify: Box<dyn Fn(Event) + Send + Sync>,
        tracking: AtomicBool,
        pressed: AtomicBool,
    }

    // The window handles are only used on the UI thread that owns the windows.
    unsafe impl Send for Sink {}
    unsafe impl Sync for Sink {}

    /// Sinks by their own window handle, and by the app window they sit in.
    #[derive(Default)]
    struct Registry {
        by_sink: HashMap<isize, Arc<Sink>>,
        sink_of_parent: HashMap<isize, isize>,
    }

    static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

    fn lock() -> std::sync::MutexGuard<'static, Registry> {
        REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn sink_of(hwnd: HWND) -> Option<Arc<Sink>> {
        lock().by_sink.get(&(hwnd.0 as isize)).cloned()
    }

    /// Registers the window class once; false if that failed.
    static CLASS_REGISTERED: LazyLock<bool> = LazyLock::new(|| unsafe {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES::default(),
            lpfnWndProc: Some(sink_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: HINSTANCE(GetModuleHandleW(PCWSTR::null()).unwrap_or_default().0),
            hIcon: HICON::default(),
            hCursor: HCURSOR::default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: CLASS_NAME,
            hIconSm: HICON::default(),
        };
        RegisterClassExW(&class) != 0
    });

    pub fn place<R: Runtime>(
        window: &WebviewWindow<R>,
        rect: Option<SnapRect>,
    ) -> Result<(), String> {
        let parent = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
        let existing = lock()
            .sink_of_parent
            .get(&(parent.0 as isize))
            .map(|sink| HWND(*sink as *mut _));
        let sink = match (existing, rect) {
            (Some(sink), _) => sink,
            (None, None) => return Ok(()),
            (None, Some(_)) => create(window, parent)?,
        };
        unsafe {
            match rect.filter(|r| r.width > 0 && r.height > 0) {
                Some(r) => {
                    let _ = SetWindowPos(
                        sink,
                        Some(HWND_TOP),
                        r.x,
                        r.y,
                        r.width,
                        r.height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
                None => {
                    let _ = SetWindowPos(
                        sink,
                        Some(HWND_TOP),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE,
                    );
                }
            }
        }
        Ok(())
    }

    fn create<R: Runtime>(window: &WebviewWindow<R>, parent: HWND) -> Result<HWND, String> {
        if !*CLASS_REGISTERED {
            return Err("could not register the maximize button window class".into());
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS_NAME,
                PCWSTR::null(),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                Some(parent),
                None,
                GetModuleHandleW(PCWSTR::null())
                    .ok()
                    .map(|module| HINSTANCE(module.0)),
                None,
            )
        }
        .map_err(|e| e.to_string())?;

        let target = window.clone();
        let sink = Arc::new(Sink {
            parent,
            notify: Box::new(move |event| {
                let _ = target.eval(format!(
                    "window.__ignisFrameSnap && window.__ignisFrameSnap('{}')",
                    event.name()
                ));
            }),
            tracking: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
        });
        {
            let mut registry = lock();
            registry.by_sink.insert(hwnd.0 as isize, sink);
            registry
                .sink_of_parent
                .insert(parent.0 as isize, hwnd.0 as isize);
        }
        unsafe {
            let _ = SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, hwnd.0 as usize);
        }
        Ok(hwnd)
    }

    /// The top resize band stays with the window, so the window can still be
    /// resized from its top edge across the button.
    fn in_top_resize_band(hwnd: HWND, parent: HWND, y_screen: i32) -> bool {
        unsafe {
            if IsZoomed(parent).as_bool() {
                return false;
            }
            let dpi = GetDpiForWindow(hwnd);
            let band = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
                + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
            let mut point = POINT { x: 0, y: y_screen };
            let _ = ScreenToClient(parent, &mut point);
            point.y < band
        }
    }

    unsafe extern "system" fn sink_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let Some(sink) = sink_of(hwnd) else {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        };
        match msg {
            WM_NCHITTEST => {
                let y_screen = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if in_top_resize_band(hwnd, sink.parent, y_screen) {
                    LRESULT(HTTRANSPARENT as isize)
                } else {
                    LRESULT(HTMAXBUTTON as isize)
                }
            }
            WM_NCMOUSEMOVE => {
                if !sink.tracking.swap(true, Ordering::SeqCst) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                    (sink.notify)(Event::Enter);
                }
                LRESULT(0)
            }
            WM_NCMOUSELEAVE => {
                sink.tracking.store(false, Ordering::SeqCst);
                sink.pressed.store(false, Ordering::SeqCst);
                (sink.notify)(Event::Leave);
                LRESULT(0)
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                sink.pressed.store(true, Ordering::SeqCst);
                (sink.notify)(Event::Down);
                LRESULT(0)
            }
            WM_NCLBUTTONUP => {
                if sink.pressed.swap(false, Ordering::SeqCst) {
                    unsafe {
                        let command = if IsZoomed(sink.parent).as_bool() {
                            SW_RESTORE
                        } else {
                            SW_MAXIMIZE
                        };
                        let _ = ShowWindow(sink.parent, command);
                    }
                    (sink.notify)(Event::Up);
                }
                LRESULT(0)
            }
            // No system menu from the button.
            WM_NCRBUTTONDOWN | WM_NCRBUTTONUP => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// Keeps the sink above the web view after the window changes size, and
    /// forgets it when the window goes away.
    unsafe extern "system" fn parent_proc(
        parent: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        sink: usize,
    ) -> LRESULT {
        match msg {
            WM_SIZE | WM_WINDOWPOSCHANGED => unsafe {
                let _ = SetWindowPos(
                    HWND(sink as *mut _),
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            },
            WM_NCDESTROY => {
                let mut registry = lock();
                registry.by_sink.remove(&(sink as isize));
                registry.sink_of_parent.remove(&(parent.0 as isize));
                drop(registry);
                unsafe {
                    let _ = RemoveWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID);
                }
            }
            _ => {}
        }
        unsafe { DefSubclassProc(parent, msg, wparam, lparam) }
    }
}

use serde::Deserialize;
use tauri::{Runtime, WebviewWindow};

/// The maximize button's place in the window, in client pixels.
#[derive(Clone, Copy, Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct SnapRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Puts the hit-test window over the maximize button, or removes it (`None`).
pub fn place<R: Runtime>(window: &WebviewWindow<R>, rect: Option<SnapRect>) -> Result<(), String> {
    imp::place(window, rect)
}
