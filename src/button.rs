use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use std::{io, mem, thread};

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::wingdi::*;
use winapi::um::winuser::*;

use crate::wutils::{Component, Error};
use crate::{wnd_proc_gen, wpanic_ifeq, wpanic_ifnull, wutils};

const BUTTON_CLASS: &str = "CUSTOM_BTN";
const TOGGLE_BUTTON_CLASS: &str = "CUSTOM_TBTN";
const UM_INVALIDATE: u32 = WM_USER + 1;

type CbFn<T> = Box<dyn Fn(&T)>;
type CbFn2<T, U> = Box<dyn Fn(&T, U)>;

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
    click_cb: Option<CbFn<Self>>,
    paint_cb: Option<CbFn2<Self, HDC>>,
    paint_last_cb: Option<CbFn2<Self, HDC>>,
    colors: Colors,
}

pub struct ToggleButton {
    hwnd: HWND,
    state: State,
    track_mouse_leave: bool,
    is_down: bool,
    is_visual_down: bool,
    deferred_start: Option<Instant>,
    deferred_running: bool,
    click_cb: Option<CbFn<Self>>,
    paint_cb: Option<CbFn2<Self, HDC>>,
    paint_last_cb: Option<CbFn2<Self, HDC>>,
    is_toggled: bool,
    colors: Colors,
    toggled_colors: Colors,
}

pub trait BaseButton: Component {
    fn state(&self) -> State;
    fn colors(&self) -> &Colors;
    fn deferred_invalidate(&mut self);
    fn is_mouse_over(&self) -> bool;
    fn get_client_rect(&self) -> RECT;
    fn on_click(&mut self, cb: CbFn<Self>);
    fn on_paint(&mut self, cb: CbFn2<Self, HDC>);
    fn on_paint_last(&mut self, cb: CbFn2<Self, HDC>);

    fn invalidate_rect(&self) {
        wpanic_ifeq!(InvalidateRect(self.hwnd(), null(), FALSE), FALSE);
    }
}

impl Component for Button {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
        if let Ok(_) = wutils::component_registry().set_registered(h_inst as isize, BUTTON_CLASS) {
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                lpfnWndProc: Some(wnd_proc_btn),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_inst,
                hIcon: null_mut(),
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: wutils::wide_string(BUTTON_CLASS).as_ptr(),
            };

            if unsafe { RegisterClassW(&class) } == 0 {
                return Err(Error::WindowsInternal(io::Error::last_os_error()));
            }
        }

        Ok(())
    }
}

impl BaseButton for Button {
    fn state(&self) -> State {
        self.state
    }

    fn colors(&self) -> &Colors {
        &self.colors
    }

    fn deferred_invalidate(&mut self) {
        self.deferred_start = Some(Instant::now());
        if !self.deferred_running {
            self.deferred_running = true;
            wpanic_ifeq!(PostMessageW(self.hwnd, UM_INVALIDATE, 0, 0), FALSE);
        }
    }

    fn is_mouse_over(&self) -> bool {
        let mut cursor_point = POINT::default();
        wpanic_ifeq!(GetCursorPos(&mut cursor_point), FALSE);

        wpanic_ifeq!(ScreenToClient(self.hwnd, &mut cursor_point), FALSE);

        let rect = self.get_client_rect();

        unsafe { PtInRect(&rect, cursor_point) == TRUE }
    }

    fn get_client_rect(&self) -> RECT {
        wutils::get_client_rect(self.hwnd).unwrap()
    }

    fn on_click(&mut self, cb: CbFn<Self>) {
        self.click_cb = Some(cb);
    }

    fn on_paint(&mut self, cb: CbFn2<Self, HDC>) {
        self.paint_cb = Some(cb);
    }

    fn on_paint_last(&mut self, cb: CbFn2<Self, HDC>) {
        self.paint_last_cb = Some(cb);
    }
}

impl Button {
    pub fn new(
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        colors: Option<Colors>,
    ) -> Result<Box<Self>, Error> {
        Self::register_class(h_inst)?;

        let mut me = Box::new(Self {
            hwnd: null_mut(),
            state: State::None,
            track_mouse_leave: false,
            is_down: false,
            is_visual_down: false,
            deferred_start: None,
            deferred_running: false,
            click_cb: None,
            paint_cb: None,
            paint_last_cb: None,
            colors: colors.unwrap_or(Colors {
                default: RGB(100, 100, 100),
                hover: RGB(80, 80, 80),
                down: RGB(60, 60, 60),
            }),
        });

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wutils::wide_string(BUTTON_CLASS).as_ptr(),
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
                            unsafe {
                                PostMessageW(hwnd as _, UM_INVALIDATE, 0, 0);
                            }
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
                            cb(self);
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
}

impl Component for ToggleButton {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
        if let Ok(_) =
            wutils::component_registry().set_registered(h_inst as isize, TOGGLE_BUTTON_CLASS)
        {
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                lpfnWndProc: Some(wnd_proc_tbtn),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_inst,
                hIcon: null_mut(),
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: wutils::wide_string(TOGGLE_BUTTON_CLASS).as_ptr(),
            };

            if unsafe { RegisterClassW(&class) } == 0 {
                return Err(Error::WindowsInternal(io::Error::last_os_error()));
            }
        }

        Ok(())
    }
}

impl BaseButton for ToggleButton {
    fn state(&self) -> State {
        self.state
    }

    fn colors(&self) -> &Colors {
        &self.colors
    }

    fn deferred_invalidate(&mut self) {
        self.deferred_start = Some(Instant::now());
        if !self.deferred_running {
            self.deferred_running = true;
            wpanic_ifeq!(PostMessageW(self.hwnd, UM_INVALIDATE, 0, 0), FALSE);
        }
    }

    fn is_mouse_over(&self) -> bool {
        let mut cursor_point = POINT::default();
        wpanic_ifeq!(GetCursorPos(&mut cursor_point), FALSE);

        wpanic_ifeq!(ScreenToClient(self.hwnd, &mut cursor_point), FALSE);

        let rect = self.get_client_rect();

        unsafe { PtInRect(&rect, cursor_point) == TRUE }
    }

    fn get_client_rect(&self) -> RECT {
        wutils::get_client_rect(self.hwnd).unwrap()
    }

    fn on_click(&mut self, cb: CbFn<Self>) {
        self.click_cb = Some(cb);
    }

    fn on_paint(&mut self, cb: CbFn2<Self, HDC>) {
        self.paint_cb = Some(cb);
    }

    fn on_paint_last(&mut self, cb: CbFn2<Self, HDC>) {
        self.paint_last_cb = Some(cb);
    }
}

impl ToggleButton {
    pub fn new(
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        colors: Option<Colors>,
        toggled_colors: Option<Colors>,
    ) -> Result<Box<Self>, Error> {
        Self::register_class(h_inst)?;

        let mut me = Box::new(Self {
            hwnd: null_mut(),
            state: State::None,
            track_mouse_leave: false,
            is_down: false,
            is_visual_down: false,
            deferred_start: None,
            deferred_running: false,
            click_cb: None,
            paint_cb: None,
            paint_last_cb: None,
            is_toggled: false,
            colors: colors.unwrap_or(Colors {
                default: RGB(100, 100, 100),
                hover: RGB(80, 80, 80),
                down: RGB(60, 60, 60),
            }),
            toggled_colors: toggled_colors.unwrap_or(Colors {
                default: RGB(70, 70, 70),
                hover: RGB(60, 60, 60),
                down: RGB(50, 50, 50),
            }),
        });

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wutils::wide_string(TOGGLE_BUTTON_CLASS).as_ptr(),
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

    pub fn is_toggled(&self) -> bool {
        self.is_toggled
    }

    pub fn toggle(&mut self) -> bool {
        self.is_toggled = !self.is_toggled;
        self.invalidate_rect();
        self.is_toggled
    }

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));
        let o_pen = wpanic_ifnull!(SelectObject(hdc, GetStockObject(wutils::DC_PEN)));
        // let o_brush = wpanic_ifnull!(SelectObject(hdc, GetStockObject(wutils::DC_BRUSH)));

        if let Some(cb) = self.paint_cb.as_ref() {
            cb(self, hdc);
        } else {
            let colors = if self.is_toggled {
                &self.toggled_colors
            } else {
                &self.colors
            };

            let bg_color = if self.is_visual_down {
                colors.down
            } else {
                match self.state {
                    State::None => colors.default,
                    State::Hover => colors.hover,
                    State::Down => colors.down,
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
                        self.toggle();
                        if let Some(cb) = self.click_cb.as_ref() {
                            cb(self);
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
}

wnd_proc_gen!(Button, wnd_proc_btn);
wnd_proc_gen!(ToggleButton, wnd_proc_tbtn);
