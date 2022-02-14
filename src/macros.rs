#[macro_export]
macro_rules! wpanic_ifeq {
    ( $code:expr, $compared:expr ) => {{
        let res = unsafe { $code };
        if res == $compared {
            std::panic::panic_any(std::io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wpanic_ifne {
    ( $code:expr, $compared:expr ) => {{
        let res = unsafe { $code };
        if res != $compared {
            std::panic::panic_any(std::io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wpanic_ifnull {
    ( $code:expr ) => {{
        let res = unsafe { $code };
        if res as winapi::shared::minwindef::LPVOID == winapi::shared::ntdef::NULL {
            std::panic::panic_any(std::io::Error::last_os_error());
        }
        res
    }};
}

#[macro_export]
macro_rules! wpanic_ifisnull {
    ( $code:expr ) => {{
        let res = unsafe { $code };
        if res.is_null() {
            std::panic::panic_any(std::io::Error::last_os_error());
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
