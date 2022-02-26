use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::{io, mem};

use winapi::shared::d3d9types::D3DCOLORVALUE;
use winapi::shared::minwindef::{FALSE, HINSTANCE, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::windef::HWND;
use winapi::um::d2d1::{
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush, D2D1_BRUSH_PROPERTIES, D2D1_COLOR_F,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RECT_F, D2D1_RENDER_TARGET_PROPERTIES, D2D1_SIZE_U,
};
use winapi::um::wingdi::{CreateSolidBrush, DeleteObject, RGB};
use winapi::um::winuser::{
    BeginPaint, CreateWindowExW, DefWindowProcW, DestroyWindow, EndPaint, FillRect, MoveWindow,
    ReleaseCapture, SendMessageW, SetCapture, TrackMouseEvent, MK_LBUTTON, PAINTSTRUCT, TME_LEAVE,
    TRACKMOUSEEVENT, WM_CREATE, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSELEAVE,
    WM_MOUSEMOVE, WM_PAINT, WM_SIZE, WM_USER, WS_CHILD, WS_VISIBLE,
};

use crate::component::Component;
use crate::wutils::Error;
use crate::{wnd_proc_gen, wpanic_ifeq, wpanic_ifne, wpanic_ifnull, wutils};

const BUTTON_CLASS: &str = "CUSTOM_BTN";
const TOGGLE_BUTTON_CLASS: &str = "CUSTOM_TBTN";
const CM_CLICK: UINT = WM_USER + 1;
const CM_PAINTLAST: UINT = WM_USER + 2;

type CbFn<T> = Box<dyn Fn(&T)>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    None,
    Hover,
    Down,
}

pub struct Colors {
    default: D3DCOLORVALUE,
    hover: D3DCOLORVALUE,
    down: D3DCOLORVALUE,
}

impl Colors {
    pub fn new(default: D3DCOLORVALUE, hover: D3DCOLORVALUE, down: D3DCOLORVALUE) -> Self {
        Self {
            default,
            hover,
            down,
        }
    }

    pub fn default(&self) -> D3DCOLORVALUE {
        self.default
    }

    pub fn hover(&self) -> D3DCOLORVALUE {
        self.hover
    }

    pub fn down(&self) -> D3DCOLORVALUE {
        self.down
    }
}

pub trait BaseButton: Component {
    fn state(&self) -> State;
    fn colors(&self) -> &Colors;
    fn on_click(&mut self, cb: CbFn<Self>);
    fn on_paint(&mut self, cb: CbFn<Self>);
    fn on_paint_last(&mut self, cb: CbFn<Self>);
}

pub struct Button<'a> {
    hwnd: HWND,
    is_own_d2d: bool,
    d2d_factory: &'a ID2D1Factory,
    d2d_render_target: Option<&'a ID2D1HwndRenderTarget>,
    d2d_brush: Option<&'a ID2D1SolidColorBrush>,
    state: State,
    track_mouse_leave: bool,
    is_down: bool,
    click_cb: Option<CbFn<Self>>,
    paint_cb: Option<CbFn<Self>>,
    paint_last_cb: Option<CbFn<Self>>,
    colors: Colors,
}

impl Drop for Button<'_> {
    fn drop(&mut self) {
        dbg!(("Drop button", self.hwnd));

        if let Some(ref brush) = self.d2d_brush {
            unsafe {
                brush.Release();
            }
        }

        if let Some(ref render_target) = self.d2d_render_target {
            unsafe {
                render_target.Release();
            }
        }

        if self.is_own_d2d {
            unsafe {
                self.d2d_factory.Release();
            }
        }

        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

impl Component for Button<'_> {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
        wutils::register_class(h_inst, BUTTON_CLASS, wnd_proc_btn)
    }
}

impl BaseButton for Button<'_> {
    fn state(&self) -> State {
        self.state
    }

    fn colors(&self) -> &Colors {
        &self.colors
    }

    fn on_click(&mut self, cb: CbFn<Self>) {
        self.click_cb = Some(cb);
    }

    fn on_paint(&mut self, cb: CbFn<Self>) {
        self.paint_cb = Some(cb);
    }

    fn on_paint_last(&mut self, cb: CbFn<Self>) {
        self.paint_last_cb = Some(cb);
    }
}

impl<'a> Button<'a> {
    pub fn new(
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        colors: Option<Colors>,
        d2d_factory: Option<&'a ID2D1Factory>,
    ) -> Result<Box<Self>, Error> {
        Self::register_class(h_inst)?;

        let mut is_own_d2d = false;

        let d2d_factory = match d2d_factory {
            Some(some) => some,
            None => {
                is_own_d2d = true;
                wutils::create_d2d_factory()?
            }
        };

        let mut me = Box::new(Self {
            hwnd: null_mut(),
            is_own_d2d,
            d2d_factory,
            d2d_render_target: None,
            d2d_brush: None,
            state: State::None,
            track_mouse_leave: false,
            is_down: false,
            click_cb: None,
            paint_cb: None,
            paint_last_cb: None,
            colors: colors.unwrap_or(Colors {
                default: wutils::color_from_argb(0xff646464),
                hover: wutils::color_from_argb(0xff505050),
                down: wutils::color_from_argb(0xff3c3c3c),
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

    pub fn set_colors(&mut self, colors: &Colors) {
        self.colors = Colors::new(colors.default, colors.hover, colors.down);
    }

    fn init_d2d(&mut self) {
        if let None = self.d2d_render_target {
            let mut render_target = MaybeUninit::<*mut ID2D1HwndRenderTarget>::uninit();

            wpanic_ifne!(
                self.d2d_factory.CreateHwndRenderTarget(
                    &D2D1_RENDER_TARGET_PROPERTIES::default(),
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd: self.hwnd,
                        ..Default::default()
                    },
                    render_target.as_mut_ptr() as _,
                ),
                0
            );

            self.d2d_render_target = Some(unsafe { &*render_target.assume_init() });
        }

        if let None = self.d2d_brush {
            let mut brush = MaybeUninit::<*mut ID2D1SolidColorBrush>::uninit();
            wpanic_ifne!(
                self.d2d_render_target().CreateSolidColorBrush(
                    &D2D1_COLOR_F::default(),
                    &D2D1_BRUSH_PROPERTIES {
                        opacity: 1.0,
                        ..Default::default()
                    },
                    brush.as_mut_ptr() as _
                ),
                0
            );

            self.d2d_brush = Some(unsafe { &*brush.assume_init() });
        }
    }

    pub fn d2d_factory(&self) -> &ID2D1Factory {
        self.d2d_factory
    }

    pub fn d2d_render_target(&self) -> &ID2D1HwndRenderTarget {
        self.d2d_render_target.as_deref().unwrap()
    }

    pub fn d2d_brush(&self) -> &ID2D1SolidColorBrush {
        self.d2d_brush.as_deref().unwrap()
    }

    fn paint(&mut self) {
        self.init_d2d();

        let mut ps = PAINTSTRUCT::default();
        let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));

        let target = self.d2d_render_target();
        unsafe {
            target.BeginDraw();
            target.Clear(null_mut());
        }

        if let Some(cb) = self.paint_cb.as_ref() {
            cb(self);
        } else {
            let bg_color = match self.state {
                State::None => self.colors.default,
                State::Hover => self.colors.hover,
                State::Down => self.colors.down,
            };

            let brush = self.d2d_brush();

            unsafe {
                brush.SetColor(&bg_color);

                let size = target.GetSize();

                target.FillRectangle(
                    &D2D1_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: size.width,
                        bottom: size.height,
                    },
                    brush as *const _ as _,
                );
            }
        }

        if let Some(cb) = self.paint_last_cb.as_ref() {
            cb(self);
        }

        unsafe {
            target.EndDraw(null_mut(), null_mut());
        }

        wpanic_ifeq!(EndPaint(self.hwnd, &ps), FALSE);
    }

    fn handle_message(&mut self, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_SIZE => {
                let rect = self.get_client_rect();
                let size = D2D1_SIZE_U {
                    width: rect.right as _,
                    height: rect.bottom as _,
                };
                self.init_d2d();
                wpanic_ifne!(self.d2d_render_target().Resize(&size), 0);
            }
            WM_ERASEBKGND => {
                return 1;
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
                self.invalidate_rect();
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

pub struct ToggleButton<'a> {
    hwnd: HWND,
    h_inst: HINSTANCE,
    is_own_d2d: bool,
    d2d_factory: &'a ID2D1Factory,
    button: Option<Box<Button<'a>>>,
    state: State,
    click_cb: Option<CbFn<Self>>,
    paint_cb: Option<CbFn<Self>>,
    paint_last_cb: Option<CbFn<Self>>,
    is_toggled: bool,
    colors: Colors,
    toggled_colors: Colors,
}

impl Drop for ToggleButton<'_> {
    fn drop(&mut self) {
        self.button = None;

        dbg!(("Drop t button", self.hwnd));
        if self.is_own_d2d {
            unsafe {
                self.d2d_factory.Release();
            }
        }

        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

impl Component for ToggleButton<'_> {
    fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn register_class(h_inst: HINSTANCE) -> Result<(), Error> {
        wutils::register_class(h_inst, TOGGLE_BUTTON_CLASS, wnd_proc_tbtn)
    }
}

impl BaseButton for ToggleButton<'_> {
    fn state(&self) -> State {
        self.state
    }

    fn colors(&self) -> &Colors {
        &self.colors
    }

    fn on_click(&mut self, cb: CbFn<Self>) {
        self.click_cb = Some(cb);
    }

    fn on_paint(&mut self, cb: CbFn<Self>) {
        self.paint_cb = Some(cb);
    }

    fn on_paint_last(&mut self, cb: CbFn<Self>) {
        self.paint_last_cb = Some(cb);
    }
}

impl<'a> ToggleButton<'a> {
    pub fn new(
        parent_hwnd: HWND,
        h_inst: HINSTANCE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        colors: Option<Colors>,
        toggled_colors: Option<Colors>,
        d2d_factory: Option<&'a ID2D1Factory>,
    ) -> Result<Box<Self>, Error> {
        Self::register_class(h_inst)?;

        let mut is_own_d2d = false;

        let d2d_factory = match d2d_factory {
            Some(some) => some,
            None => {
                is_own_d2d = true;
                wutils::create_d2d_factory()?
            }
        };

        let mut me = Box::new(Self {
            hwnd: null_mut(),
            h_inst,
            is_own_d2d,
            d2d_factory,
            button: None,
            state: State::None,
            click_cb: None,
            paint_cb: None,
            paint_last_cb: None,
            is_toggled: false,
            colors: colors.unwrap_or(Colors {
                default: wutils::color_from_argb(0xff646464),
                hover: wutils::color_from_argb(0xff505050),
                down: wutils::color_from_argb(0xff3c3c3c),
            }),
            toggled_colors: toggled_colors.unwrap_or(Colors {
                default: wutils::color_from_argb(0xff464646),
                hover: wutils::color_from_argb(0xff3c3c3c),
                down: wutils::color_from_argb(0xff323232),
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

    fn reposition_components(&self) {
        let rect = self.get_client_rect();

        if let Some(ref button) = self.button {
            wpanic_ifeq!(
                MoveWindow(
                    button.hwnd(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left - 10,
                    rect.bottom - rect.top - 10,
                    TRUE
                ),
                0
            );
        }
    }

    fn on_created(&mut self) {
        let mut button = Button::new(
            self.hwnd,
            self.h_inst,
            0,
            0,
            0,
            0,
            None,
            Some(self.d2d_factory),
        )
        .unwrap();

        let hwnd = self.hwnd;

        button.on_click(Box::new(move |_| unsafe {
            SendMessageW(hwnd, CM_CLICK, 0, 0);
        }));

        button.on_paint_last(Box::new(move |_| unsafe {
            SendMessageW(hwnd, CM_PAINTLAST, 0, 0);
        }));

        self.button = Some(button);
    }

    pub fn d2d_render_target(&self) -> &ID2D1HwndRenderTarget {
        self.button.as_ref().unwrap().d2d_render_target()
    }

    pub fn d2d_brush(&self) -> &ID2D1SolidColorBrush {
        self.button.as_ref().unwrap().d2d_brush()
    }

    pub fn is_toggled(&self) -> bool {
        self.is_toggled
    }

    pub fn toggle(&mut self) -> bool {
        self.is_toggled = !self.is_toggled;

        if let Some(ref mut button) = self.button {
            if self.is_toggled {
                button.set_colors(&self.toggled_colors);
            } else {
                button.set_colors(&self.colors);
            }
        }
        self.invalidate_rect();
        self.is_toggled
    }

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));

        let bg_brush = wpanic_ifnull!(CreateSolidBrush(RGB(0xff, 0xdd, 0xdd)));
        wpanic_ifeq!(FillRect(hdc, &ps.rcPaint, bg_brush), 0);
        wpanic_ifeq!(DeleteObject(bg_brush as _), FALSE);

        wpanic_ifeq!(EndPaint(self.hwnd, &ps), FALSE);
    }

    fn handle_message(&mut self, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_PAINT => {
                self.paint();
            }
            WM_CREATE => {
                self.on_created();
                self.reposition_components();
            }
            WM_SIZE => {
                self.reposition_components();
            }
            CM_CLICK => {
                self.toggle();
                if let Some(cb) = self.click_cb.as_ref() {
                    cb(self);
                }
            }
            CM_PAINTLAST => {
                if let Some(cb) = self.paint_last_cb.as_ref() {
                    cb(self);
                }
            }
            _ => {}
        }

        unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) }
    }
}

wnd_proc_gen!(Button, wnd_proc_btn);
wnd_proc_gen!(ToggleButton, wnd_proc_tbtn);
