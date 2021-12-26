mod button;
mod tab_bar;
mod wutils;

use std::io;
use std::ptr::{null, null_mut};

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};
use winapi::um::wingdi::*;
use winapi::um::winuser::*;

use crate::button::{
    BaseButton, Button, Colors as ButtonColors, State as ButtonState, ToggleButton,
};
use crate::tab_bar::TabBar;
use crate::wutils::Component;

const WINDOW_CLASS_NAME: &str = "testwindowtabs.Window";
const WINDOW_TITLE: &str = "the testwindowtabs application";
const TITLE_BG_COLOR: (u8, u8, u8) = (150, 200, 180);
const TITLE_HOVER_COLOR: (u8, u8, u8) = (130, 180, 160);
const TITLE_DOWN_COLOR: (u8, u8, u8) = (120, 167, 148);
const TITLE_ITEM_COLOR: (u8, u8, u8) = (33, 33, 33);
const TITLE_ITEM_BLUR_COLOR: (u8, u8, u8) = (127, 127, 127);
const ICON_DIMENSION: i32 = 10;

pub struct Window {
    hwnd: HWND,
    h_inst: HINSTANCE,
    minimize_button: Option<Box<Button>>,
    maximize_button: Option<Box<Button>>,
    close_button: Option<Box<Button>>,
    tab_bar: Option<Box<TabBar>>,
}

impl Window {
    pub fn register_class(h_inst: HINSTANCE) {
        if let Ok(_) =
            wutils::component_registry().set_registered(h_inst as isize, WINDOW_CLASS_NAME)
        {
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
                lpszClassName: wutils::wide_string(WINDOW_CLASS_NAME).as_ptr(),
            };

            wpanic_ifeq!(RegisterClassW(&class), 0);
        }
    }

    pub fn new(parent_hwnd: HWND, h_inst: HINSTANCE) -> Box<Self> {
        Self::register_class(h_inst);

        let me = Box::new(Self {
            hwnd: null_mut(),
            h_inst,
            minimize_button: None,
            maximize_button: None,
            close_button: None,
            tab_bar: None,
        });

        let window_style = WS_THICKFRAME   // required for a standard resizeable window
        | WS_SYSMENU      // Explicitly ask for the titlebar to support snapping via Win + ← / Win + →
        | WS_MAXIMIZEBOX  // Add maximize button to support maximizing via mouse dragging
                        // to the top of the screen
        | WS_MINIMIZEBOX  // Add minimize button to support minimizing by clicking on the taskbar icon
        | WS_VISIBLE; // Make window visible after it is created (not important)

        wpanic_ifisnull!(CreateWindowExW(
            0,
            wutils::wide_string(WINDOW_CLASS_NAME).as_ptr(),
            wutils::wide_string(WINDOW_TITLE).as_ptr(),
            window_style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            500,
            500,
            parent_hwnd,
            null_mut(),
            h_inst,
            me.as_ref() as *const _ as _
        ));

        me
    }

    fn on_created(&mut self) {
        let title_bg = RGB(TITLE_BG_COLOR.0, TITLE_BG_COLOR.1, TITLE_BG_COLOR.2);
        let btn_hover = RGB(
            TITLE_HOVER_COLOR.0,
            TITLE_HOVER_COLOR.1,
            TITLE_HOVER_COLOR.2,
        );
        let btn_down = RGB(TITLE_DOWN_COLOR.0, TITLE_DOWN_COLOR.1, TITLE_DOWN_COLOR.2);

        let minimize_button = Button::new(
            self.hwnd,
            self.h_inst,
            0,
            0,
            0,
            0,
            Some(ButtonColors::new(title_bg, btn_hover, btn_down)),
        )
        .unwrap();

        let maximize_button = Button::new(
            self.hwnd,
            self.h_inst,
            0,
            0,
            0,
            0,
            Some(ButtonColors::new(title_bg, btn_hover, btn_down)),
        )
        .unwrap();

        let close_button = Button::new(
            self.hwnd,
            self.h_inst,
            0,
            0,
            0,
            0,
            Some(ButtonColors::new(
                title_bg,
                RGB(232, 17, 35),
                RGB(232, 73, 76),
            )),
        )
        .unwrap();

        let tab_bar = TabBar::new(0, 0, 0, 0, self.hwnd, self.h_inst).unwrap();

        self.minimize_button = Some(minimize_button);
        self.maximize_button = Some(maximize_button);
        self.close_button = Some(close_button);
        self.tab_bar = Some(tab_bar);

        let hwnd = self.hwnd;

        let minimize_button = self.minimize_button.as_mut().unwrap();
        let maximize_button = self.maximize_button.as_mut().unwrap();
        let close_button = self.close_button.as_mut().unwrap();

        minimize_button.on_click(Box::new(move |_| {
            wpanic_ifeq!(ShowWindow(hwnd, SW_MINIMIZE), FALSE);
        }));

        maximize_button.on_click(Box::new(move |_| {
            let mode = if wutils::window_is_maximized(hwnd).unwrap() {
                SW_NORMAL
            } else {
                SW_MAXIMIZE
            };

            wpanic_ifeq!(ShowWindow(hwnd, mode), FALSE);
        }));

        close_button.on_click(Box::new(move |_| {
            wpanic_ifeq!(PostMessageW(hwnd, WM_CLOSE, 0, 0), FALSE);
        }));

        minimize_button.on_paint_last(Box::new(move |button, hdc| {
            let has_focus = !unsafe { GetFocus() }.is_null();

            let title_bar_item_color = if has_focus || button.is_mouse_over() {
                RGB(TITLE_ITEM_COLOR.0, TITLE_ITEM_COLOR.1, TITLE_ITEM_COLOR.2)
            } else {
                RGB(
                    TITLE_ITEM_BLUR_COLOR.0,
                    TITLE_ITEM_BLUR_COLOR.1,
                    TITLE_ITEM_BLUR_COLOR.2,
                )
            };

            let button_icon_brush = wpanic_ifnull!(CreateSolidBrush(title_bar_item_color));

            let dpi = wutils::get_dpi_for_window(hwnd).unwrap();
            let icon_dimension = wutils::dpi_scale(ICON_DIMENSION, dpi);
            let mut icon_rect = RECT::default();
            icon_rect.right = icon_dimension;
            icon_rect.bottom = 1;
            wutils::center_rect_in_rect(&mut icon_rect, &button.get_client_rect());

            wpanic_ifeq!(FillRect(hdc, &icon_rect, button_icon_brush), 0);
            wpanic_ifeq!(DeleteObject(button_icon_brush as _), FALSE);
        }));

        maximize_button.on_paint_last(Box::new(move |button, hdc| {
            let has_focus = !unsafe { GetFocus() }.is_null();

            let title_bar_item_color = if has_focus || button.is_mouse_over() {
                RGB(TITLE_ITEM_COLOR.0, TITLE_ITEM_COLOR.1, TITLE_ITEM_COLOR.2)
            } else {
                RGB(
                    TITLE_ITEM_BLUR_COLOR.0,
                    TITLE_ITEM_BLUR_COLOR.1,
                    TITLE_ITEM_BLUR_COLOR.2,
                )
            };

            let colors = button.colors();
            match button.state() {
                ButtonState::None => {
                    wpanic_ifnull!(SetDCBrushColor(hdc, colors.default()));
                }
                ButtonState::Hover => {
                    wpanic_ifnull!(SetDCBrushColor(hdc, colors.hover()));
                }
                ButtonState::Down => {
                    wpanic_ifnull!(SetDCBrushColor(hdc, colors.down()));
                }
            }

            let dpi = wutils::get_dpi_for_window(hwnd).unwrap();
            let icon_dimension = wutils::dpi_scale(ICON_DIMENSION, dpi);
            let mut icon_rect = RECT::default();
            icon_rect.right = icon_dimension;
            icon_rect.bottom = icon_dimension;
            wutils::center_rect_in_rect(&mut icon_rect, &button.get_client_rect());

            let button_icon_pen = wpanic_ifnull!(CreatePen(PS_SOLID as _, 1, title_bar_item_color));

            wpanic_ifnull!(SelectObject(hdc, button_icon_pen as _));
            wpanic_ifnull!(SelectObject(hdc, GetStockObject(HOLLOW_BRUSH as _)));

            if wutils::window_is_maximized(hwnd).unwrap() {
                wpanic_ifeq!(
                    Rectangle(
                        hdc,
                        icon_rect.left + 2,
                        icon_rect.top,
                        icon_rect.right,
                        icon_rect.bottom - 2,
                    ),
                    FALSE
                );
                wpanic_ifnull!(SelectObject(hdc, GetStockObject(DC_BRUSH as _)));
                wpanic_ifeq!(
                    Rectangle(
                        hdc,
                        icon_rect.left,
                        icon_rect.top + 2,
                        icon_rect.right - 2,
                        icon_rect.bottom,
                    ),
                    FALSE
                );
            } else {
                wpanic_ifeq!(
                    Rectangle(
                        hdc,
                        icon_rect.left,
                        icon_rect.top,
                        icon_rect.right,
                        icon_rect.bottom,
                    ),
                    FALSE
                );
            }

            wpanic_ifeq!(DeleteObject(button_icon_pen as _), FALSE);
        }));

        close_button.on_paint_last(Box::new(move |button, hdc| {
            let has_focus = !unsafe { GetFocus() }.is_null();

            let title_bar_item_color = if has_focus {
                RGB(TITLE_ITEM_COLOR.0, TITLE_ITEM_COLOR.1, TITLE_ITEM_COLOR.2)
            } else {
                RGB(
                    TITLE_ITEM_BLUR_COLOR.0,
                    TITLE_ITEM_BLUR_COLOR.1,
                    TITLE_ITEM_BLUR_COLOR.2,
                )
            };

            let dpi = wutils::get_dpi_for_window(hwnd).unwrap();
            let icon_dimension = wutils::dpi_scale(ICON_DIMENSION, dpi);
            let mut icon_rect = RECT {
                right: icon_dimension,
                bottom: icon_dimension,
                ..Default::default()
            };
            wutils::center_rect_in_rect(&mut icon_rect, &button.get_client_rect());

            let button_icon_pen = if button.state() == ButtonState::None {
                wpanic_ifnull!(CreatePen(PS_SOLID as _, 1, title_bar_item_color))
            } else {
                wpanic_ifnull!(CreatePen(PS_SOLID as _, 1, RGB(0xff, 0xff, 0xff)))
            };

            let original = wpanic_ifnull!(SelectObject(hdc, button_icon_pen as _));

            wpanic_ifeq!(
                MoveToEx(hdc, icon_rect.left + 1, icon_rect.top + 1, null_mut()),
                FALSE
            );
            wpanic_ifeq!(LineTo(hdc, icon_rect.right, icon_rect.bottom), FALSE);
            wpanic_ifeq!(
                MoveToEx(hdc, icon_rect.left + 1, icon_rect.bottom - 1, null_mut()),
                FALSE
            );
            wpanic_ifeq!(LineTo(hdc, icon_rect.right, icon_rect.top), FALSE);

            wpanic_ifnull!(SelectObject(hdc, original));
            wpanic_ifeq!(DeleteObject(button_icon_pen as _), FALSE);
        }));
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
        let title_bar_rect = wutils::get_titlebar_rect(self.hwnd).unwrap();
        let button_rects = wutils::get_titlebar_button_rects(self.hwnd, &title_bar_rect).unwrap();
        self.reposition_component(self.minimize_button.as_ref(), button_rects.minimize);
        self.reposition_component(self.maximize_button.as_ref(), button_rects.maximize);
        self.reposition_component(self.close_button.as_ref(), button_rects.close);

        let mut tab_rect = title_bar_rect;
        tab_rect.top = wutils::FAKE_SHADOW_HEIGHT + 2;
        tab_rect.right = button_rects.minimize.left - 100;
        self.reposition_component(self.tab_bar.as_ref(), tab_rect);
    }

    fn handle_message(&mut self, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_NCCALCSIZE if wparam == 1 => {
                return 0;
            }
            WM_ACTIVATE => {
                for button in [
                    self.minimize_button.as_ref(),
                    self.maximize_button.as_ref(),
                    self.close_button.as_ref(),
                ] {
                    if let Some(button) = button {
                        button.invalidate_rect();
                    }
                }
            }
            WM_SIZE => {
                self.reposition_components();
            }
            WM_NCHITTEST => {
                // Let the default procedure handle resizing areas
                let result = unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) };

                match result {
                    HTNOWHERE | HTRIGHT | HTLEFT | HTTOPLEFT | HTTOP | HTTOPRIGHT
                    | HTBOTTOMRIGHT | HTBOTTOM | HTBOTTOMLEFT => {
                        return result;
                    }
                    _ => {}
                }

                // Looks like adjustment happening in NCCALCSIZE is messing with the detection
                // of the top hit area so manually fixing that.
                let dpi = wutils::get_dpi_for_window(self.hwnd).unwrap();
                let frame_y = wutils::get_system_metrics_for_dpi(SM_CYFRAME, dpi).unwrap();
                let padding = wutils::get_system_metrics_for_dpi(SM_CXPADDEDBORDER, dpi).unwrap();

                let cursor_point = MAKEPOINTS(lparam as u32);
                let mut cursor_point = POINT {
                    x: cursor_point.x as i32,
                    y: cursor_point.y as i32,
                };
                wpanic_ifeq!(ScreenToClient(self.hwnd, &mut cursor_point), FALSE);
                if cursor_point.y > 0 && cursor_point.y < frame_y + padding {
                    return HTTOP;
                }

                // Since we are drawing our own caption, this needs to be a custom test
                if cursor_point.y < wutils::get_titlebar_rect(self.hwnd).unwrap().bottom {
                    return HTCAPTION;
                }

                return HTCLIENT;
            }
            WM_PAINT => {
                let has_focus = !unsafe { GetFocus() }.is_null();

                let mut ps = PAINTSTRUCT::default();
                let hdc = wpanic_ifnull!(BeginPaint(self.hwnd, &mut ps));

                // Paint Background
                let bg_color = RGB(200, 250, 230);
                let bg_brush = wpanic_ifnull!(CreateSolidBrush(bg_color));

                wpanic_ifeq!(FillRect(hdc, &ps.rcPaint, bg_brush), 0);
                wpanic_ifeq!(DeleteObject(bg_brush as _), FALSE);

                // // Paint Title Bar
                let title_bar_color = RGB(150, 200, 180);
                let title_bar_brush = wpanic_ifnull!(CreateSolidBrush(title_bar_color));
                // let title_bar_hover_color = RGB(130, 180, 160);
                // let title_bar_hover_brush = CreateSolidBrush(title_bar_hover_color);

                let title_bar_rect = wutils::get_titlebar_rect(self.hwnd).unwrap();

                // Title Bar Background

                wpanic_ifeq!(FillRect(hdc, &title_bar_rect, title_bar_brush), 0);
                wpanic_ifeq!(DeleteObject(title_bar_brush as _), FALSE);

                // // Draw window title
                // let theme = OpenThemeData(self.hwnd, wutils::wide_string("WINDOW").as_ptr());

                // let mut logical_font = LOGFONTW::default();
                // let mut old_font: HFONT = null_mut();
                // if GetThemeSysFont(theme, TMT_CAPTIONFONT, &mut logical_font) >= 0 {
                //     let theme_font = CreateFontIndirectW(&mut logical_font);
                //     old_font = SelectObject(hdc, theme_font as _) as _;
                // }

                // let mut title_text_buffer: [u16; 255] = std::mem::zeroed();
                // GetWindowTextW(self.hwnd, title_text_buffer.as_mut_ptr(), 255);

                // let mut title_bar_text_rect = title_bar_rect;
                // // Add padding on the left
                // let text_padding = 10; // There seems to be no good way to get this offset
                // title_bar_text_rect.left += 200;
                // // Add padding on the right for the buttons
                // title_bar_text_rect.right = button_rects.minimize.left - text_padding;

                // // println!("{:?}", title_bar_text_rect);

                // let draw_theme_options = DTTOPTS {
                //     dwSize: std::mem::size_of::<DTTOPTS>() as _,
                //     dwFlags: DTT_TEXTCOLOR,
                //     crText: title_bar_item_color,
                //     ..Default::default()
                // };
                // let res = DrawThemeTextEx(
                //     theme,
                //     hdc,
                //     0,
                //     0,
                //     title_text_buffer.as_ptr(),
                //     -1,
                //     DT_VCENTER | DT_SINGLELINE | DT_WORD_ELLIPSIS,
                //     &mut title_bar_text_rect,
                //     &draw_theme_options,
                // );
                // if res != 0 {
                //     println!("error drawing text {:#x}", res);
                // }
                // if !old_font.is_null() {
                //     SelectObject(hdc, old_font as _);
                // }
                // CloseThemeData(theme);

                // Paint fake top shadow. Original is missing because of the client rect extension.
                let fake_top_shadow_color = if has_focus {
                    RGB(112, 112, 112)
                } else {
                    RGB(170, 170, 170)
                };
                let fake_top_shadow_brush = wpanic_ifnull!(CreateSolidBrush(fake_top_shadow_color));
                let fake_top_shadow_rect = wutils::fake_shadow_rect(self.hwnd).unwrap();
                wpanic_ifeq!(
                    FillRect(hdc, &fake_top_shadow_rect, fake_top_shadow_brush),
                    0
                );
                wpanic_ifeq!(DeleteObject(fake_top_shadow_brush as _), FALSE);

                wpanic_ifeq!(EndPaint(self.hwnd, &ps), FALSE);
            }
            WM_CREATE => {
                let mut size_rect = RECT::default();

                wpanic_ifeq!(GetWindowRect(self.hwnd, &mut size_rect), FALSE);

                // Inform the application of the frame change to force redrawing with the new
                // client area that is extended into the title bar
                wpanic_ifeq!(
                    SetWindowPos(
                        self.hwnd,
                        null_mut(),
                        size_rect.left,
                        size_rect.top,
                        size_rect.right - size_rect.left,
                        size_rect.bottom - size_rect.top,
                        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE,
                    ),
                    FALSE
                );

                self.on_created();
                self.reposition_components();
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                return 0;
            }
            _ => {}
        }

        unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) }
    }
}

fn main() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // Support high-dpi screens
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let h_inst = wpanic_ifisnull!(GetModuleHandleW(null()));

    let window = Window::new(null_mut(), h_inst);

    unsafe {
        CreateWindowExW(
            0,
            wutils::wide_string("BUTTON").as_ptr(),
            wutils::wide_string("OK").as_ptr(),
            WS_CHILD | WS_VISIBLE | BS_DEFPUSHBUTTON,
            4,
            100,
            100,
            50,
            window.hwnd,
            null_mut(),
            h_inst,
            null_mut(),
        );
    }

    Button::new(window.hwnd, h_inst, 4, 200, 100, 50, None).unwrap();
    let mut tbtn = ToggleButton::new(window.hwnd, h_inst, 154, 200, 100, 50, None, None).unwrap();
    tbtn.on_click(Box::new(|button| {
        println!("toggled! current state: {:?}", button.is_toggled());
    }));

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while GetMessageW(&mut msg, window.hwnd, 0, 0) == TRUE {
            TranslateMessage(&mut msg);
            DispatchMessageW(&mut msg);
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, message: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let window;

    if message == WM_NCCREATE || message == WM_CREATE {
        let cs = lparam as *const CREATESTRUCTW;
        window = unsafe { (*cs).lpCreateParams as *mut Window };
        unsafe { (*window).hwnd = hwnd };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, window as _) };
    } else {
        window = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Window;
    }

    if message == WM_NCCALCSIZE && wparam == 1 {
        let dpi = wutils::get_dpi_for_window(hwnd).unwrap();

        let frame_x = wutils::get_system_metrics_for_dpi(SM_CXFRAME, dpi).unwrap();
        let frame_y = wutils::get_system_metrics_for_dpi(SM_CYFRAME, dpi).unwrap();
        let padding = wutils::get_system_metrics_for_dpi(SM_CXPADDEDBORDER, dpi).unwrap();

        let params = unsafe { (lparam as *mut NCCALCSIZE_PARAMS).as_mut().unwrap() };

        let mut requested_client_rect = &mut params.rgrc[0];

        requested_client_rect.right -= frame_x + padding;
        requested_client_rect.left += frame_x + padding;
        requested_client_rect.bottom -= frame_y + padding;

        if wutils::window_is_maximized(hwnd).unwrap() {
            requested_client_rect.top += padding;
        }
    }

    if let Some(window) = unsafe { window.as_mut() } {
        return window.handle_message(message, wparam, lparam);
    }

    if message == WM_NCCALCSIZE && wparam == 1 {
        return 0;
    }

    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
