use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use std::{io, mem, thread};

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::wingdi::*;
use winapi::um::winuser::*;

use crate::wutils::{Component, Error};
use crate::{wnd_proc_gen, wpanic_ifeq, wpanic_ifnull, wutils};

const CLASS_NAME: &str = "CUSTOM_BTN";
const UM_INVALIDATE: u32 = WM_USER + 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    None,
    Hover,
    Down,
}

pub struct Colors {
    default: COLORREF,
    hover: COLORREF,
    down: COLORREF,
}

impl Colors {
    pub fn new(default: COLORREF, hover: COLORREF, down: COLORREF) -> Self {
        Self {
            default,
            hover,
            down,
        }
    }

    pub fn default(&self) -> COLORREF {
        self.default
    }

    pub fn hover(&self) -> COLORREF {
        self.hover
    }

    pub fn down(&self) -> COLORREF {
        self.down
    }
}

pub struct Button {
    hwnd: HWND,
    state: State,
    track_mouse_leave: bool,
    is_down: bool,
    is_visual_down: bool,
    deferred_start: Option<Instant>,
    deferred_running: bool,
    colors: Colors,
    click_cb: Option<Box<dyn Fn()>>,
    paint_cb: Option<Box<dyn Fn(&Self, HDC)>>,
    paint_last_cb: Option<Box<dyn Fn(&Self, HDC)>>,
}

impl Component for Button {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Button {
    pub fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
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

        Ok(())
    }

    pub fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
        colors: Option<Colors>,
    ) -> Result<Box<Self>, Error> {
        let mut me = Box::new(Self {
            hwnd: null_mut(),
            state: State::None,
            track_mouse_leave: false,
            is_down: false,
            is_visual_down: false,
            deferred_start: None,
            deferred_running: false,
            colors: colors.unwrap_or(Colors {
                default: RGB(100, 100, 100),
                hover: RGB(80, 80, 80),
                down: RGB(60, 60, 60),
            }),
            click_cb: None,
            paint_cb: None,
            paint_last_cb: None,
        });

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wutils::wide_string(CLASS_NAME).as_ptr(),
                wutils::wide_string("").as_ptr(),
                WS_CHILD | WS_VISIBLE,
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

        (*me).hwnd = hwnd;

        Ok(me)
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    pub fn invalidate_rect(&self) {
        wpanic_ifeq!(InvalidateRect(self.hwnd, null(), FALSE), FALSE);
    }

    pub fn deferred_invalidate(&mut self) {
        self.deferred_start = Some(Instant::now());
        if !self.deferred_running {
            self.deferred_running = true;
            wpanic_ifeq!(PostMessageW(self.hwnd, UM_INVALIDATE, 0, 0), FALSE);
        }
    }

    pub fn is_mouse_over(&self) -> bool {
        let mut cursor_point = POINT::default();
        wpanic_ifeq!(GetCursorPos(&mut cursor_point), FALSE);

        wpanic_ifeq!(ScreenToClient(self.hwnd, &mut cursor_point), FALSE);

        let rect = self.get_client_rect();

        unsafe { PtInRect(&rect, cursor_point) == TRUE }
    }

    pub fn get_client_rect(&self) -> RECT {
        wutils::get_client_rect(self.hwnd).unwrap()
    }

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));
        let o_pen = wpanic_ifnull!(SelectObject(hdc, GetStockObject(wutils::DC_PEN)));
        // let o_brush = wpanic_ifnull!(SelectObject(hdc, GetStockObject(wutils::DC_BRUSH)));

        if let Some(cb) = self.paint_cb.as_ref() {
            cb(self, hdc);
        } else {
            let bg_color = if self.is_visual_down {
                self.colors.down
            } else {
                match self.state {
                    State::None => self.colors.default,
                    State::Hover => self.colors.hover,
                    State::Down => self.colors.down,
                }
            };

            let bg_brush = wpanic_ifnull!(CreateSolidBrush(bg_color));
            wpanic_ifeq!(FillRect(hdc, &ps.rcPaint, bg_brush), 0);
            wpanic_ifeq!(DeleteObject(bg_brush as _), FALSE);
        }

        if let Some(cb) = self.paint_last_cb.as_ref() {
            cb(self, hdc);
        }

        // wpanic_ifnull!(SelectObject(hdc, o_brush));
        wpanic_ifnull!(SelectObject(hdc, o_pen));
        wpanic_ifeq!(EndPaint(self.hwnd, &ps), FALSE);
    }

    fn handle_message(&mut self, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            UM_INVALIDATE => {
                self.invalidate_rect();
                if let Some(started) = self.deferred_start {
                    if started.elapsed().as_millis() < 50 {
                        let hwnd = self.hwnd as isize;
                        thread::spawn(move || {
                            thread::sleep(Duration::new(0, 10000000));
                            wpanic_ifeq!(PostMessageW(hwnd as _, UM_INVALIDATE, 0, 0), FALSE);
                        });
                    } else {
                        self.deferred_running = false;
                        self.is_visual_down = false;
                    }
                }
            }
            WM_PAINT => {
                self.paint();
            }
            WM_MOUSELEAVE => {
                self.track_mouse_leave = false;
                self.state = State::None;
                self.invalidate_rect();
            }
            WM_MOUSEMOVE => {
                let old_state = self.state;

                if !self.track_mouse_leave {
                    self.track_mouse_leave = true;

                    let mut trk = TRACKMOUSEEVENT {
                        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };

                    wpanic_ifeq!(TrackMouseEvent(&mut trk), FALSE);
                }

                if self.is_mouse_over() && self.is_down {
                    if wparam & MK_LBUTTON > 0 {
                        self.state = State::Down;
                    } else {
                        self.state = State::Hover;
                    }
                } else {
                    self.state = State::Hover;
                }

                if old_state != self.state {
                    self.invalidate_rect();
                }
            }
            WM_LBUTTONDOWN => {
                self.state = State::Down;
                self.is_down = true;
                self.is_visual_down = true;
                self.invalidate_rect();
                self.deferred_invalidate();
                unsafe { SetCapture(self.hwnd) };
                return 1;
            }
            WM_LBUTTONUP => {
                let old_state = self.state;

                if self.is_mouse_over() {
                    self.state = State::Hover;

                    if self.is_down {
                        if let Some(cb) = self.click_cb.as_ref() {
                            cb();
                        }
                    }
                } else {
                    self.state = State::None;
                }

                self.is_down = false;
                if old_state != self.state {
                    self.invalidate_rect();
                }

                wpanic_ifeq!(ReleaseCapture(), FALSE);
            }
            _ => {}
        }

        unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) }
    }

    pub fn on_click(&mut self, cb: Box<dyn Fn()>) {
        self.click_cb = Some(cb);
    }

    pub fn on_paint(&mut self, cb: Box<dyn Fn(&Self, HDC)>) {
        self.paint_cb = Some(cb);
    }

    pub fn on_paint_last(&mut self, cb: Box<dyn Fn(&Self, HDC)>) {
        self.paint_last_cb = Some(cb);
    }
}

wnd_proc_gen!(Button, wnd_proc);
