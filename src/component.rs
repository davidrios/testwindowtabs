use std::ptr::null;

use winapi::shared::minwindef::{FALSE, HINSTANCE};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::InvalidateRect;

use crate::wpanic_ifeq;
use crate::wutils::{self, Error};

pub trait Component {
    fn hwnd(&self) -> HWND;
    fn register_class(h_inst: HINSTANCE) -> Result<(), Error>;

    fn get_client_rect(&self) -> RECT {
        wutils::get_client_rect(self.hwnd()).unwrap()
    }

    fn invalidate_rect(&self) {
        wpanic_ifeq!(InvalidateRect(self.hwnd(), null(), FALSE), FALSE);
    }

    fn is_mouse_over(&self) -> bool {
        wutils::is_mouse_over(self.hwnd()).unwrap()
    }
}
