//! Tiny non-activating, click-through desktop layer for the remote's touch point.
//! The reader publishes only its latest state: old positions are never queued.
use super::gesture::Mode;
use parking_lot::Mutex;
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    core::w,
    Win32::{
        Foundation::{
            COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
        },
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

const SIDE: i32 = 64;
#[link(name = "user32")]
unsafe extern "system" {
    fn SetThreadDpiAwarenessContext(context: isize) -> isize;
}
#[derive(Clone, Copy)]
struct Frame {
    position: (i32, i32),
    visible: bool,
    pressed: bool,
    updated: Instant,
    notice: Option<(Instant, Mode)>,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            position: (0, 0),
            visible: false,
            pressed: false,
            updated: Instant::now(),
            notice: None,
        }
    }
}
pub struct Indicator {
    frame: Arc<Mutex<Frame>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
impl Indicator {
    pub fn new() -> Self {
        let frame = Arc::new(Mutex::new(Frame::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let (f, s, e) = (frame.clone(), stop.clone(), error.clone());
        let worker = std::thread::Builder::new()
            .name("hisense-touch-point".into())
            .spawn(move || {
                if let Err(problem) = run(f, s) {
                    log::warn!("HISENSE indicator failed: {problem}");
                    *e.lock() = Some(format!("触摸圆点显示失败：{problem}"));
                }
            });
        let worker = match worker {
            Ok(h) => Some(h),
            Err(problem) => {
                *error.lock() = Some(problem.to_string());
                None
            }
        };
        Self {
            frame,
            stop,
            worker,
            error,
        }
    }
    pub fn update(&self, visible: bool, position: (i32, i32), pressed: bool) {
        let mut frame = self.frame.lock();
        let wake = visible || frame.visible != visible;
        let notice = frame.notice;
        *frame = Frame {
            position,
            visible,
            pressed,
            updated: Instant::now(),
            notice,
        };
        drop(frame);
        if let Some(worker) = &self.worker {
            if wake {
                worker.thread().unpark();
            }
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().take()
    }
    pub fn notice(&self, mode: Mode) {
        self.frame.lock().notice = Some((Instant::now(), mode));
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}
impl Drop for Indicator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, message, wp, lp),
    }
}

struct Layer {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pixels: *mut u32,
    width: i32,
    height: i32,
}
impl Layer {
    fn new() -> Result<Self, String> {
        Self::new_size(SIDE, SIDE)
    }
    fn new_size(width: i32, height: i32) -> Result<Self, String> {
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None).map_err(|e| e.to_string())?.0);
            let class = w!("NexusPrimeHisenseTouchPoint");
            let wc = WNDCLASSW {
                hInstance: instance,
                lpszClassName: class,
                lpfnWndProc: Some(window_proc),
                ..Default::default()
            };
            // RegisterClass also returns zero if this same class was registered by an
            // earlier reader session. CreateWindowEx remains the authoritative check.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                class,
                w!("海信触摸圆点"),
                WS_POPUP,
                0,
                0,
                width,
                height,
                HWND::default(),
                HMENU::default(),
                instance,
                None,
            )
            .map_err(|e| e.to_string())?;
            let dc = CreateCompatibleDC(HDC::default());
            if dc.is_invalid() {
                let _ = DestroyWindow(hwnd);
                return Err("无法创建圆点画布".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(
                dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                HANDLE::default(),
                0,
            ) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(dc);
                    let _ = DestroyWindow(hwnd);
                    return Err(error.to_string());
                }
            };
            let previous = SelectObject(dc, bitmap);
            Ok(Self {
                hwnd,
                dc,
                bitmap,
                previous,
                pixels: bits.cast(),
                width,
                height,
            })
        }
    }
    fn paint(&self, point: (i32, i32), pressed: bool, pulse: Option<f32>) -> Result<(), String> {
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, (SIDE * SIDE) as usize) };
        for y in 0..SIDE {
            for x in 0..SIDE {
                let d = ((x as f32 + 0.5 - 32.).powi(2) + (y as f32 + 0.5 - 32.).powi(2)).sqrt();
                let mut pixel = (0., 0., 0., 0.);
                // Dark outer edge and white centre remain visible on light and dark apps.
                blend(
                    &mut pixel,
                    (15., 23., 42.),
                    ((9. - d) / 2.).clamp(0., 1.) * 0.28,
                );
                blend(
                    &mut pixel,
                    (255., 255., 255.),
                    (7. - d).clamp(0., 1.) * 0.96,
                );
                if pressed {
                    blend(&mut pixel, (59., 130., 246.), (5.8 - d).clamp(0., 1.));
                } else {
                    blend(&mut pixel, (255., 255., 255.), (4.6 - d).clamp(0., 1.));
                }
                if let Some(t) = pulse {
                    let radius = 10. + 17. * t;
                    let coverage = (1.5 - (d - radius).abs()).clamp(0., 1.) * (1. - t) * 0.8;
                    blend(&mut pixel, (59., 130., 246.), coverage);
                }
                let (r, g, b, a) = pixel;
                pixels[(y * SIDE + x) as usize] = ((a * 255.).round() as u32) << 24
                    | ((r.round() as u32) << 16)
                    | ((g.round() as u32) << 8)
                    | b.round() as u32;
            }
        }
        self.present((point.0 - SIDE / 2, point.1 - SIDE / 2), 255)
    }
    fn present(&self, point: (i32, i32), opacity: u8) -> Result<(), String> {
        let destination = POINT {
            x: point.0,
            y: point.1,
        };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source = POINT::default();
        let alpha = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: opacity,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                HDC::default(),
                Some(&destination),
                Some(&size),
                self.dc,
                Some(&source),
                COLORREF(0),
                Some(&alpha),
                ULW_ALPHA,
            )
        }
        .map_err(|e| e.to_string())
    }
    fn paint_notice(&self, mode: Mode) -> Result<(), String> {
        {
            let pixels = unsafe {
                std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
            };
            let (half_w, half_h) = (self.width as f32 / 2., self.height as f32 / 2.);
            for y in 0..self.height {
                for x in 0..self.width {
                    let qx = (x as f32 + 0.5 - half_w).abs() - (half_w - 12.);
                    let qy = (y as f32 + 0.5 - half_h).abs() - (half_h - 12.);
                    let distance = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - 12.;
                    let a = (0.5 - distance).clamp(0., 1.);
                    pixels[(y * self.width + x) as usize] = ((255. * a).round() as u32) << 24
                        | ((23. * a).round() as u32) << 16
                        | ((31. * a).round() as u32) << 8
                        | (43. * a).round() as u32;
                }
            }
        }
        unsafe {
            let mut spec = LOGFONTW {
                lfHeight: -20,
                lfWeight: 600,
                lfCharSet: DEFAULT_CHARSET,
                lfQuality: ANTIALIASED_QUALITY,
                ..Default::default()
            };
            for (destination, ch) in spec
                .lfFaceName
                .iter_mut()
                .zip("Microsoft YaHei UI".encode_utf16())
            {
                *destination = ch;
            }
            let font = CreateFontIndirectW(&spec);
            if font.is_invalid() {
                return Err("提示字体创建失败".into());
            }
            let previous = SelectObject(self.dc, font);
            SetBkMode(self.dc, TRANSPARENT);
            SetTextColor(self.dc, COLORREF(0x00ffffff));
            let mut text: Vec<u16> = mode.title().encode_utf16().collect();
            let mut rect = RECT {
                left: 12,
                top: 0,
                right: self.width - 12,
                bottom: self.height,
            };
            let drawn = DrawTextW(
                self.dc,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            let _ = GdiFlush();
            let _ = SelectObject(self.dc, previous);
            let _ = DeleteObject(font);
            if drawn == 0 {
                return Err("模式提示文字绘制失败".into());
            }
        }
        // GDI text clears alpha in glyph pixels; the card's text region is opaque.
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
        };
        for pixel in pixels {
            if *pixel >> 24 == 0 && *pixel & 0x00ffffff != 0 {
                *pixel |= 0xff000000;
            }
        }
        Ok(())
    }
    fn show(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }
    fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
// Premultiplied RGB required by UpdateLayeredWindow's per-pixel alpha.
fn blend(p: &mut (f32, f32, f32, f32), color: (f32, f32, f32), alpha: f32) {
    let keep = 1. - alpha;
    p.0 = color.0 * alpha + p.0 * keep;
    p.1 = color.1 * alpha + p.1 * keep;
    p.2 = color.2 * alpha + p.2 * keep;
    p.3 = alpha + p.3 * keep;
}

fn run(frame: Arc<Mutex<Frame>>, stop: Arc<AtomicBool>) -> Result<(), String> {
    // This window uses the same physical pixels as the touch-injection path.
    // Change only this dedicated UI thread, including on mixed-DPI desktops.
    unsafe {
        SetThreadDpiAwarenessContext(-4);
    }
    let layer = Layer::new()?;
    let toast = Layer::new_size(260, 64)?;
    let mut last_notice: Option<Instant> = None;
    let mut toast_visible = false;
    let mut visible = false;
    let mut last_pressed = false;
    let mut pulse_start: Option<Instant> = None;
    let mut previous: Option<((i32, i32), bool)> = None;
    log::info!("HISENSE touch indicator ready click_through=true");
    while !stop.load(Ordering::Acquire) {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let f = *frame.lock();
        if let Some((started, enabled)) = f.notice {
            let age = started.elapsed().as_secs_f32();
            if age < 2.0 {
                if last_notice != Some(started) {
                    toast.paint_notice(enabled)?;
                    last_notice = Some(started);
                }
                let monitor =
                    unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
                let mut info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                let got = unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool();
                let origin = if got {
                    (info.rcWork.left + 24, info.rcWork.top + 24)
                } else {
                    (24, 24)
                };
                let opacity = if age < 1.75 {
                    255
                } else {
                    ((2. - age) / 0.25 * 255.).clamp(0., 255.) as u8
                };
                toast.present(origin, opacity)?;
                if !toast_visible {
                    toast.show();
                    toast_visible = true;
                }
            } else if toast_visible {
                toast.hide();
                toast_visible = false;
            }
        }
        let show = f.visible && f.updated.elapsed() < Duration::from_millis(750);
        if show {
            if f.pressed && !last_pressed {
                pulse_start = Some(Instant::now());
                log::info!("HISENSE touch indicator press feedback");
            }
            let pulse = pulse_start
                .map(|start| start.elapsed().as_secs_f32() / 0.28)
                .filter(|t| *t < 1.);
            if !visible || previous != Some((f.position, f.pressed)) || pulse_start.is_some() {
                layer.paint(f.position, f.pressed, pulse)?;
                previous = Some((f.position, f.pressed));
            }
            if !visible {
                layer.show();
                visible = true;
            }
            if pulse.is_none() {
                pulse_start = None;
            }
        } else if visible {
            layer.hide();
            visible = false;
            previous = None;
            pulse_start = None;
        }
        last_pressed = show && f.pressed;
        std::thread::park_timeout(Duration::from_millis(if visible || toast_visible {
            8
        } else {
            250
        }));
    }
    layer.hide();
    toast.hide();
    Ok(())
}
