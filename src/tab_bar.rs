use std::collections::HashMap;
use std::io;
use std::ptr::*;

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::wingdi::*;
use winapi::um::winuser::*;

use crate::button::{BaseButton, Button, ToggleButton};
use crate::wutils::{Component, Error};
use crate::{wnd_proc_gen, wpanic_ifeq, wpanic_ifnull, wutils};

const CLASS_NAME: &str = "TAB_BAR";
const UM_ADDTAB: u32 = WM_USER + 1;
const UM_CLICKTAB: u32 = WM_USER + 2;

pub struct TabBar {
    hwnd: HWND,
    h_inst: HINSTANCE,
    add_button: Option<Box<Button>>,
    tab_count: u32,
    tab_order: Vec<u32>,
    tab_buttons: HashMap<u32, Box<ToggleButton>>,
}

impl Component for TabBar {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
        if let Ok(_) = wutils::component_registry().set_registered(h_inst as isize, CLASS_NAME) {
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_inst,
                hIcon: null_mut(),
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: wutils::wide_string(CLASS_NAME).as_ptr(),
            };

            if unsafe { RegisterClassW(&class) } == 0 {
                return Err(Error::WindowsInternal(io::Error::last_os_error()));
            }
        }

        Ok(())
    }
}

impl TabBar {
    pub fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
    ) -> Result<Box<Self>, Error> {
        Self::register_class(h_inst)?;

        let me = Box::new(Self {
            hwnd: null_mut(),
            h_inst,
            add_button: None,
            tab_count: 0,
            tab_order: Vec::with_capacity(100),
            tab_buttons: HashMap::with_capacity(100),
        });

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wutils::wide_string(CLASS_NAME).as_ptr(),
                wutils::wide_string("").as_ptr(),
                WS_CHILD | WS_OVERLAPPED | WS_VISIBLE,
                x,
                y,
                width,
                height,
                parent_hwnd,
                null_mut(),
                h_inst,
                me.as_ref() as *const _ as _,
            )
        };

        if hwnd.is_null() {
            return Err(Error::WindowsInternal(io::Error::last_os_error()));
        }

        Ok(me)
    }

    pub fn get_client_rect(&self) -> RECT {
        wutils::get_client_rect(self.hwnd).unwrap()
    }

    fn on_created(&mut self) {
        let mut add_button = Button::new(self.hwnd, self.h_inst, 0, 0, 0, 0, None).unwrap();

        let hwnd = self.hwnd;
        add_button.on_click(Box::new(move || {
            wpanic_ifeq!(PostMessageW(hwnd, UM_ADDTAB, 0, 0), FALSE);
        }));

        self.add_button = Some(add_button);
    }

    fn reposition_component<T: Component>(&self, button_ref: Option<&Box<T>>, rect: RECT) {
        if let Some(button) = button_ref {
            wpanic_ifeq!(
                MoveWindow(
                    button.hwnd(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    TRUE
                ),
                0
            );
        }
    }

    fn reposition_components(&self) {
        let my_rect = self.get_client_rect();

        let mut btn_rect = my_rect;
        btn_rect.top = btn_rect.bottom - 40;
        btn_rect.left = 4;
        btn_rect.right = btn_rect.left + 10;

        for idx in &self.tab_order {
            self.reposition_component(self.tab_buttons.get(idx), btn_rect);
            btn_rect.left += 12;
            btn_rect.right = btn_rect.left + 10;
        }

        btn_rect.right += 30;
        self.reposition_component(self.add_button.as_ref(), btn_rect);
    }

    fn handle_message(&mut self, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            UM_ADDTAB => {
                self.add_item();
            }
            UM_CLICKTAB => {
                dbg!(wparam);
            }
            WM_SIZE => {
                self.reposition_components();
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));

                // Paint Background
                let bg_color = RGB(0xff, 0xff, 0xff);
                let bg_brush = wpanic_ifnull!(CreateSolidBrush(bg_color));
                wpanic_ifeq!(FillRect(hdc, &ps.rcPaint, bg_brush), 0);
                wpanic_ifeq!(DeleteObject(bg_brush as _), FALSE);
            }
            WM_CREATE => {
                self.on_created();
                self.reposition_components();
            }
            _ => {}
        }
        unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) }
    }

    pub fn add_item(&mut self) {
        let hwnd = self.hwnd;
        let idx = self.tab_count;
        self.tab_count += 1;
        let mut button = ToggleButton::new(self.hwnd, self.h_inst, 0, 0, 0, 0, None, None).unwrap();

        button.on_click(Box::new(move || {
            wpanic_ifeq!(PostMessageW(hwnd, UM_CLICKTAB, idx as usize, 0), FALSE);
        }));

        self.tab_order.push(idx);
        self.tab_buttons.insert(idx, button);
        self.reposition_components();
    }
}

wnd_proc_gen!(TabBar, wnd_proc);
