use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::mem::MaybeUninit;
use std::os::windows::prelude::OsStrExt;
use std::ptr::null_mut;
use std::sync::{Mutex, Once};

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::shared::winerror::{HRESULT, S_OK};
use winapi::um::uxtheme::*;
use winapi::um::winuser::*;

pub const CS_ACTIVE: i32 = 1;
pub const DC_BRUSH: i32 = 18;
pub const DC_PEN: i32 = 19;
pub const TMT_CAPTIONFONT: i32 = 801;
pub const WP_CAPTION: i32 = 1;

pub const TOP_AND_BOTTOM_BORDERS: i32 = 2;
pub const FAKE_SHADOW_HEIGHT: i32 = 1;

#[derive(Debug)]
pub enum Error {
    Generic(String),
    WindowsInternal(io::Error),
    Hresult(HRESULT),
    ComponentAlreadyRegistered,
}

pub trait Component {
    fn hwnd(&self) -> HWND;
    fn register_class(h_inst: HINSTANCE) -> Result<(), Error>;
}

#[derive(Debug)]
pub struct TitleBarButtonRects {
    pub close: RECT,
    pub maximize: RECT,
    pub minimize: RECT,
}

pub struct ComponentRegistry {
    registry: Mutex<HashMap<isize, HashMap<&'static str, bool>>>,
}

impl ComponentRegistry {
    fn new() -> ComponentRegistry {
        ComponentRegistry {
            registry: Mutex::new(HashMap::with_capacity(3)),
        }
    }

    pub fn set_registered(&self, h_inst: isize, class_name: &'static str) -> Result<(), Error> {
        let mut guard = self.registry.lock().unwrap();
        let registry = guard.borrow_mut();
        if !registry.contains_key(&h_inst) {
            registry.insert(h_inst, HashMap::with_capacity(10));
        }

        let hregistry = registry.get_mut(&h_inst).unwrap();
        if hregistry.contains_key(class_name) {
            return Err(Error::ComponentAlreadyRegistered);
        }

        hregistry.insert(class_name, true);

        Ok(())
    }
}

pub fn component_registry() -> &'static ComponentRegistry {
    static mut SINGLETON: MaybeUninit<ComponentRegistry> = MaybeUninit::uninit();
    static ONCE: Once = Once::new();

    unsafe {
        ONCE.call_once(|| {
            SINGLETON.write(ComponentRegistry::new());
        });

        SINGLETON.assume_init_ref()
    }
}

pub fn wide_string(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn dpi_scale(value: i32, dpi: u32) -> i32 {
    (value as f32 * dpi as f32 / 96f32) as _
}

pub fn get_client_rect(handle: HWND) -> Result<RECT, Error> {
    let mut rect = RECT::default();

    if unsafe { GetClientRect(handle, &mut rect) } != TRUE {
        return Err(Error::WindowsInternal(io::Error::last_os_error()));
    }

    Ok(rect)
}

pub fn get_dpi_for_window(handle: HWND) -> Result<u32, Error> {
    let dpi = unsafe { GetDpiForWindow(handle) };
    if dpi == 0 {
        return Err(Error::Generic(String::from("Failed to get DPI")));
    }

    Ok(dpi)
}

pub fn get_system_metrics_for_dpi(n_index: i32, dpi: u32) -> Result<i32, Error> {
    let res = unsafe { GetSystemMetricsForDpi(n_index, dpi) };
    if res == 0 {
        return Err(Error::WindowsInternal(io::Error::last_os_error()));
    }
    Ok(res)
}

pub fn with_theme(
    handle: HWND,
    exec: Box<dyn Fn(HTHEME) -> Result<(), Error>>,
) -> Result<(), Error> {
    let theme = unsafe { OpenThemeData(handle, wide_string("WINDOW").as_ptr()) };
    if theme.is_null() {
        return Err(Error::WindowsInternal(io::Error::last_os_error()));
    }

    exec(theme)?;

    let res = unsafe { CloseThemeData(theme) };
    if res != S_OK {
        return Err(Error::Hresult(res));
    }

    Ok(())
}

pub fn get_titlebar_rect(handle: HWND) -> Result<RECT, Error> {
    let theme = unsafe { OpenThemeData(handle, wide_string("WINDOW").as_ptr()) };
    if theme.is_null() {
        return Err(Error::WindowsInternal(io::Error::last_os_error()));
    }

    let rect = RECT::default();
    let mut size = SIZE::default();

    let res = unsafe {
        GetThemePartSize(
            theme,
            null_mut(),
            WP_CAPTION,
            CS_ACTIVE,
            &rect,
            TS_TRUE,
            &mut size,
        )
    };

    if res != S_OK {
        return Err(Error::Hresult(res));
    }

    let res = unsafe { CloseThemeData(theme) };
    if res != S_OK {
        return Err(Error::Hresult(res));
    }

    // if window_is_maximized(handle).unwrap_or(false) {
    //     size.cy -= 4;
    // }

    let dpi = get_dpi_for_window(handle)?;

    let height = dpi_scale(size.cy, dpi) + TOP_AND_BOTTOM_BORDERS;

    let mut rect = get_client_rect(handle)?;

    rect.bottom = rect.top + height;

    Ok(rect)
}

pub fn fake_shadow_rect(handle: HWND) -> Result<RECT, Error> {
    let mut rect = get_client_rect(handle)?;

    rect.bottom = rect.top + FAKE_SHADOW_HEIGHT;
    Ok(rect)
}

pub fn get_titlebar_button_rects(
    handle: HWND,
    title_bar_rect: &RECT,
) -> Result<TitleBarButtonRects, Error> {
    let dpi = unsafe { GetDpiForWindow(handle) };
    if dpi == 0 {
        return Err(Error::Generic(String::from("Failed to get DPI")));
    }

    let mut button_rects = TitleBarButtonRects {
        close: RECT::default(),
        maximize: RECT::default(),
        minimize: RECT::default(),
    };
    // Sadly SM_CXSIZE does not result in the right size buttons for Win10
    let button_width = dpi_scale(47, dpi);

    button_rects.close = *title_bar_rect;
    button_rects.close.top += FAKE_SHADOW_HEIGHT;
    // if window_is_maximized(handle).unwrap_or(false) {
    //     button_rects.close.right -= 2;
    // }

    button_rects.close.left = button_rects.close.right - button_width;

    button_rects.maximize = button_rects.close;
    button_rects.maximize.left -= button_width;
    button_rects.maximize.right -= button_width;

    button_rects.minimize = button_rects.maximize;
    button_rects.minimize.left -= button_width;
    button_rects.minimize.right -= button_width;

    Ok(button_rects)
}

pub fn window_is_maximized(handle: HWND) -> Result<bool, Error> {
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as _,
        ..Default::default()
    };

    if unsafe { GetWindowPlacement(handle, &mut placement) } != TRUE {
        return Err(Error::WindowsInternal(io::Error::last_os_error()));
    }

    Ok(placement.showCmd == SW_SHOWMAXIMIZED as _)
}

pub fn center_rect_in_rect(to_center: &mut RECT, outer_rect: &RECT) {
    let to_width = to_center.right - to_center.left;
    let to_height = to_center.bottom - to_center.top;
    let outer_width = outer_rect.right - outer_rect.left;
    let outer_height = outer_rect.bottom - outer_rect.top;
    let padding_x = (outer_width - to_width) / 2;
    let padding_y = (outer_height - to_height) / 2;
    to_center.left = outer_rect.left + padding_x;
    to_center.top = outer_rect.top + padding_y;
    to_center.right = to_center.left + to_width;
    to_center.bottom = to_center.top + to_height;
}

#[macro_export]
macro_rules! wpanic_ifeq {
    ( $code:expr, $compared:expr ) => {{
        let res = unsafe { $code };
        if res == $compared {
            std::panic::panic_any(io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wpanic_ifnull {
    ( $code:expr ) => {{
        let res = unsafe { $code };
        if res as LPVOID == winapi::shared::ntdef::NULL {
            std::panic::panic_any(io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wpanic_ifisnull {
    ( $code:expr ) => {{
        let res = unsafe { $code };
        if res.is_null() {
            std::panic::panic_any(io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wnd_proc_gen {
    ( $component_class:ident, $fn_name:ident ) => {
        extern "system" fn $fn_name(
            hwnd: winapi::shared::windef::HWND,
            message: winapi::shared::minwindef::UINT,
            wparam: winapi::shared::minwindef::WPARAM,
            lparam: winapi::shared::minwindef::LPARAM,
        ) -> winapi::shared::minwindef::LRESULT {
            let component;

            if message == winapi::um::winuser::WM_NCCREATE
                || message == winapi::um::winuser::WM_CREATE
            {
                let cs = lparam as *const winapi::um::winuser::CREATESTRUCTW;
                unsafe {
                    component = (*cs).lpCreateParams as *mut $component_class;
                    (*component).hwnd = hwnd;
                    winapi::um::winuser::SetWindowLongPtrW(
                        hwnd,
                        winapi::um::winuser::GWLP_USERDATA,
                        component as _,
                    );
                }
            } else {
                component = unsafe {
                    winapi::um::winuser::GetWindowLongPtrW(hwnd, winapi::um::winuser::GWLP_USERDATA)
                } as *mut $component_class;
            }

            if let Some(component) = unsafe { component.as_mut() } {
                return component.handle_message(message, wparam, lparam);
            }

            unsafe { winapi::um::winuser::DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    };
}
