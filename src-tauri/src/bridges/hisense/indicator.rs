//! Tiny non-activating, click-through desktop layer for the remote's touch point.
//! The reader publishes only its latest state: old positions are never queued.
use super::{
    gesture::Mode,
    visual::{NoticeStyle, Snapshot, Style, Trail, TRAIL_SIDE},
};
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

#[derive(Clone)]
pub enum Notice {
    Mode(Mode),
    Trail(bool),
    Profile(String),
    Action {
        caption: String,
        description: String,
    },
}
impl Notice {
    fn title(&self) -> &str {
        match self {
            Self::Mode(mode) => mode.title(),
            Self::Trail(true) => "荧光笔开启",
            Self::Trail(false) => "荧光笔关闭",
            Self::Profile(name) => name.as_str(),
            Self::Action { description, .. } => description.as_str(),
        }
    }
    pub fn action(caption: String, description: String, previous: Option<Self>) -> Self {
        // Keep the result of mode/profile/trail switches, instead of replacing
        // it with the generic action name that initiated the switch.
        let description = match previous {
            Some(result @ (Self::Mode(_) | Self::Trail(_) | Self::Profile(_))) => {
                result.title().to_owned()
            }
            _ => description,
        };
        Self::Action {
            caption,
            description,
        }
    }
    fn caption(&self) -> Option<&str> {
        match self {
            Self::Action { caption, .. } => Some(caption.as_str()),
            _ => None,
        }
    }
}
const SIDE: i32 = 64;
#[link(name = "user32")]
unsafe extern "system" {
    fn SetThreadDpiAwarenessContext(context: isize) -> isize;
}
#[derive(Clone)]
struct Frame {
    position: (i32, i32),
    visible: bool,
    pressed: bool,
    updated: Instant,
    notice: Option<(Instant, Notice)>,
    style: Style,
    notice_style: NoticeStyle,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            position: (0, 0),
            visible: false,
            pressed: false,
            updated: Instant::now(),
            notice: None,
            style: Style::default(),
            notice_style: NoticeStyle::default(),
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
        let notice = frame.notice.clone();
        let style = frame.style;
        let notice_style = frame.notice_style;
        *frame = Frame {
            position,
            visible,
            pressed,
            updated: Instant::now(),
            notice,
            style,
            notice_style,
        };
        drop(frame);
        if let Some(worker) = &self.worker {
            if wake {
                worker.thread().unpark();
            }
        }
    }
    pub fn set_style(&self, style: Style) {
        let mut frame = self.frame.lock();
        if frame.style != style {
            frame.style = style;
            drop(frame);
            if let Some(worker) = &self.worker {
                worker.thread().unpark();
            }
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().take()
    }
    pub fn set_notice_style(&self, style: NoticeStyle) {
        let mut frame = self.frame.lock();
        if frame.notice_style != style {
            frame.notice_style = style;
            if !style.enabled {
                frame.notice = None;
            }
            drop(frame);
            if let Some(worker) = &self.worker {
                worker.thread().unpark();
            }
        }
    }
    pub fn notice(&self, mode: Notice) {
        let mut frame = self.frame.lock();
        if !frame.notice_style.enabled {
            return;
        }
        frame.notice = Some((Instant::now(), mode));
        drop(frame);
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
    fn paint(
        &self,
        point: (i32, i32),
        pressed: bool,
        pulse: Option<f32>,
        style: Style,
    ) -> Result<(), String> {
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, (SIDE * SIDE) as usize) };
        for y in 0..SIDE {
            for x in 0..SIDE {
                let d = ((x as f32 + 0.5 - 32.).powi(2) + (y as f32 + 0.5 - 32.).powi(2)).sqrt();
                let mut pixel = (0., 0., 0., 0.);
                // A dark outer edge keeps each chosen color visible on light apps.
                blend(
                    &mut pixel,
                    (15., 23., 42.),
                    ((9. - d) / 2.).clamp(0., 1.) * 0.28,
                );
                blend(
                    &mut pixel,
                    style.color.floats(),
                    (7. - d).clamp(0., 1.) * 0.96,
                );
                if pressed {
                    blend(
                        &mut pixel,
                        style.pressed_color.floats(),
                        (5.8 - d).clamp(0., 1.),
                    );
                } else {
                    blend(&mut pixel, style.color.floats(), (4.6 - d).clamp(0., 1.));
                }
                if let Some(t) = pulse {
                    let radius = 10. + 17. * t;
                    let coverage = (1.5 - (d - radius).abs()).clamp(0., 1.) * (1. - t) * 0.8;
                    blend(&mut pixel, style.pressed_color.floats(), coverage);
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
        self.present_region(point, (self.width, self.height), opacity)
    }
    fn present_region(
        &self,
        point: (i32, i32),
        dimensions: (i32, i32),
        opacity: u8,
    ) -> Result<(), String> {
        let destination = POINT {
            x: point.0,
            y: point.1,
        };
        let size = SIZE {
            cx: dimensions.0,
            cy: dimensions.1,
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
    fn paint_trail(&self, snapshot: &Snapshot, style: Style) -> Result<(), String> {
        let (width, height) = snapshot.size;
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
        };
        for y in 0..height {
            pixels[(y * self.width) as usize..(y * self.width + width) as usize].fill(0);
        }
        let radius = style.trail.radius();
        let halo = style.trail.glow_radius();
        let opacity = style.trail.opacity_percent as f32 / 100.;
        let glow_weight = (4. / 11.) * style.trail.glow_percent as f32 / 50.;
        let mut stamp = |x: f32, y: f32, life: f32| {
            let x0 = ((x - halo).floor() as i32).clamp(0, width - 1);
            let x1 = ((x + halo).ceil() as i32).clamp(0, width - 1);
            let y0 = ((y - halo).floor() as i32).clamp(0, height - 1);
            let y1 = ((y + halo).ceil() as i32).clamp(0, height - 1);
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let d = (px as f32 + 0.5 - x).hypot(py as f32 + 0.5 - y);
                    let core = (1. - d / radius).clamp(0., 1.) * (1. - glow_weight);
                    let glow = (1. - d / halo).clamp(0., 1.).powi(2) * glow_weight;
                    let alpha = (core + glow) * opacity * life;
                    let index = (py * self.width + px) as usize;
                    if (alpha * 255.).round() as u32 > pixels[index] >> 24 {
                        pixels[index] = style.trail_color.premultiplied(alpha);
                    }
                }
            }
        };
        if let Some(&(x, y, life)) = snapshot.points.first() {
            stamp(x, y, life);
        }
        for pair in snapshot.points.windows(2) {
            let (x0, y0, l0) = pair[0];
            let (x1, y1, l1) = pair[1];
            let steps = ((x1 - x0).hypot(y1 - y0) / radius.clamp(3., 10.))
                .ceil()
                .max(1.) as usize;
            for step in 1..=steps {
                let t = step as f32 / steps as f32;
                stamp(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, l0 + (l1 - l0) * t);
            }
        }
        self.present_region(snapshot.origin, snapshot.size, 255)
    }
    fn paint_notice(&self, mode: Notice, style: NoticeStyle) -> Result<(), String> {
        // Render white glyphs on an empty mask, then composite chosen colors.
        // This also supports opaque black text on a fully transparent card.
        unsafe {
            std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
                .fill(0);
        }
        let mut lines = Vec::new();
        if let Some(caption) = mode.caption() {
            lines.push((caption, 14, 400, 7, 30, COLORREF(0x00ffffff)));
            lines.push((
                mode.title(),
                20,
                600,
                30,
                self.height - 5,
                COLORREF(0x00ffffff),
            ));
        } else {
            lines.push((mode.title(), 20, 600, 0, self.height, COLORREF(0x00ffffff)));
        }
        for (label, size, weight, top, bottom, color) in lines {
            unsafe {
                let mut spec = LOGFONTW {
                    lfHeight: -size,
                    lfWeight: weight,
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
                SetTextColor(self.dc, color);
                let mut text: Vec<u16> = label.encode_utf16().collect();
                let mut rect = RECT {
                    left: 12,
                    top,
                    right: self.width - 12,
                    bottom,
                };
                let drawn = DrawTextW(
                    self.dc,
                    &mut text,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                let _ = GdiFlush();
                let _ = SelectObject(self.dc, previous);
                let _ = DeleteObject(font);
                if drawn == 0 {
                    return Err("提示文字绘制失败".into());
                }
            }
        }
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.pixels, (self.width * self.height) as usize)
        };
        let (half_w, half_h) = (self.width as f32 / 2., self.height as f32 / 2.);
        for y in 0..self.height {
            for x in 0..self.width {
                let qx = (x as f32 + 0.5 - half_w).abs() - (half_w - 12.);
                let qy = (y as f32 + 0.5 - half_h).abs() - (half_h - 12.);
                let distance = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - 12.;
                let edge = (0.5 - distance).clamp(0., 1.);
                let pixel = &mut pixels[(y * self.width + x) as usize];
                let ink = (*pixel & 255) as f32 / 255.;
                *pixel = style.pixel(edge, ink);
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
    let toast = Layer::new_size(360, 78)?;
    let mut last_notice: Option<Instant> = None;
    let mut last_notice_style = NoticeStyle::default();
    let mut toast_visible = false;
    let mut visible = false;
    let mut last_pressed = false;
    let mut pulse_start: Option<Instant> = None;
    let mut previous: Option<((i32, i32), bool)> = None;
    let mut trail = Trail::default();
    let mut trail_layer: Option<Layer> = None;
    let mut trail_visible = false;
    let mut trail_painted = Instant::now();
    let mut previous_style = Style::default();
    log::info!("HISENSE touch indicator ready click_through=true");
    while !stop.load(Ordering::Acquire) {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let f = frame.lock().clone();
        if let Some((started, enabled)) = f.notice.filter(|_| f.notice_style.enabled) {
            let age = started.elapsed().as_secs_f32();
            if age < 2.0 {
                if last_notice != Some(started) || last_notice_style != f.notice_style {
                    toast.paint_notice(enabled, f.notice_style)?;
                    last_notice = Some(started);
                    last_notice_style = f.notice_style;
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
        } else if toast_visible {
            toast.hide();
            toast_visible = false;
        }
        let show = f.visible && f.updated.elapsed() < Duration::from_millis(750);
        if previous_style != f.style {
            previous_style = f.style;
            trail.clear();
            previous = None;
        }
        let now = Instant::now();
        trail.configure(f.style.trail);
        trail.update(show && f.style.trail_enabled, f.position, now);
        if !show || !f.style.trail_enabled {
            if trail_visible {
                if let Some(layer) = &trail_layer {
                    layer.hide();
                }
                trail_visible = false;
            }
        } else if trail_painted.elapsed() >= Duration::from_millis(16) {
            if let Some(snapshot) = trail.snapshot(now) {
                if trail_layer.is_none() {
                    trail_layer = Some(Layer::new_size(TRAIL_SIDE, TRAIL_SIDE)?);
                }
                let paint = trail_layer.as_ref().unwrap();
                paint.paint_trail(&snapshot, f.style)?;
                if !trail_visible {
                    paint.show();
                    trail_visible = true;
                    if visible {
                        layer.show();
                    }
                }
            } else if trail_visible {
                if let Some(layer) = &trail_layer {
                    layer.hide();
                }
                trail_visible = false;
            }
            trail_painted = now;
        }
        if show {
            if f.pressed && !last_pressed {
                pulse_start = Some(Instant::now());
                log::info!("HISENSE touch indicator press feedback");
            }
            let pulse = pulse_start
                .map(|start| start.elapsed().as_secs_f32() / 0.28)
                .filter(|t| *t < 1.);
            if !visible || previous != Some((f.position, f.pressed)) || pulse_start.is_some() {
                layer.paint(f.position, f.pressed, pulse, f.style)?;
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
        std::thread::park_timeout(Duration::from_millis(
            if visible || toast_visible || trail_visible {
                8
            } else {
                250
            },
        ));
    }
    layer.hide();
    toast.hide();
    if let Some(layer) = trail_layer {
        layer.hide();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabling_notices_clears_current_and_drops_hidden_events() {
        let indicator = Indicator {
            frame: Arc::new(Mutex::new(Frame::default())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            error: Arc::new(Mutex::new(None)),
        };
        indicator.notice(Notice::Mode(Mode::Touch));
        assert!(indicator.frame.lock().notice.is_some());
        indicator.set_notice_style(NoticeStyle {
            enabled: false,
            ..Default::default()
        });
        assert!(indicator.frame.lock().notice.is_none());
        indicator.notice(Notice::Trail(true));
        indicator.set_notice_style(NoticeStyle::default());
        assert!(indicator.frame.lock().notice.is_none());
    }
    #[test]
    fn action_notice_preserves_switch_results_and_replaces_old_actions() {
        let action = Notice::action(
            "快捷 · 单击".into(),
            "切换模式".into(),
            Some(Notice::Mode(Mode::Move)),
        );
        assert_eq!(action.caption(), Some("快捷 · 单击"));
        assert_eq!(action.title(), "UWB 2/2 长按移动");
        let action = Notice::action("上 · 单击".into(), "Page Up".into(), Some(action));
        assert_eq!(action.title(), "Page Up");
        assert_eq!(
            Notice::action(
                "AI · 单击".into(),
                "开关".into(),
                Some(Notice::Trail(false))
            )
            .title(),
            "荧光笔关闭"
        );
        assert_eq!(
            Notice::action(
                "菜单 · 单击".into(),
                "切换配置".into(),
                Some(Notice::Profile("配置：Chrome".into()))
            )
            .title(),
            "配置：Chrome"
        );
    }
}
