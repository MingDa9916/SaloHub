use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    UI::{
        Input::KeyboardAndMouse::*,
        Input::Pointer::*,
        WindowsAndMessaging::{
            GetCursorPos, GetPhysicalCursorPos, GetSystemMetrics, SetCursorPos, PT_TOUCH,
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    },
};
const MARKER: usize = 0x4853524d;
pub fn extended(vk: u16, scan: u32) -> bool {
    scan & 0xff00 == 0xe000 || matches!(vk,0x21..=0x28|0x2c..=0x2e|0x5b|0x5c|0x6f|0x90|0xa3|0xa5)
}
pub fn key(vk: u16, down: bool) -> bool {
    let keypad_enter = vk == 0x10d;
    let vk = if keypad_enter { 0x0d } else { vk };
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
    let use_scan = scan != 0 && vk < 0xa6 && vk != 0x13;
    let mut flags = if down {
        KEYBD_EVENT_FLAGS(0)
    } else {
        KEYEVENTF_KEYUP
    };
    if use_scan {
        flags |= KEYEVENTF_SCANCODE;
        if keypad_enter || extended(vk, scan) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
    }
    send(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(if use_scan { 0 } else { vk }),
                wScan: if use_scan { (scan & 0xff) as u16 } else { 0 },
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    })
}
pub fn mouse(right: bool, down: bool) -> bool {
    let flags = match (right, down) {
        (false, true) => MOUSEEVENTF_LEFTDOWN,
        (false, false) => MOUSEEVENTF_LEFTUP,
        (true, true) => MOUSEEVENTF_RIGHTDOWN,
        (true, false) => MOUSEEVENTF_RIGHTUP,
    };
    send(INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MARKER,
            },
        },
    })
}
fn send(input: INPUT) -> bool {
    let ok = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) } == 1;
    if !ok {
        log::warn!(
            "HISENSE input rejected: {}",
            std::io::Error::last_os_error()
        );
    }
    ok
}
pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}
pub fn move_to(p: (i32, i32)) {
    let _ = unsafe { SetCursorPos(p.0, p.1) };
}
pub fn bounds() -> super::pointer::Bounds {
    super::pointer::Bounds {
        left: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) } as f64,
        top: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) } as f64,
        width: unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1) as f64,
        height: unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1) as f64,
    }
}
pub fn text(text: &str) {
    for ch in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            send(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: MARKER,
                    },
                },
            });
        }
    }
}

#[derive(Default)]
pub struct Output {
    held: BTreeMap<u16, usize>,
    touch_owners: BTreeSet<String>,
    touch_position: Option<(i32, i32)>,
    touch_ready: bool,
    gesture: super::gesture::Gesture,
    tap_until: Option<Instant>,
    mode_notice: Option<super::indicator::Notice>,
}
impl Output {
    pub fn gesture_threshold(&mut self, ms: u64) {
        self.gesture.set_threshold(ms);
    }
    pub fn pointer_mode(&self) -> super::gesture::Mode {
        self.gesture.mode()
    }
    pub fn ai_down(&mut self, source: &str) -> Result<(), String> {
        if self.gesture.ai_down(source) {
            self.mode_notice = Some(super::indicator::Notice::Mode(self.gesture.mode()));
            self.tap_until = None;
            self.touch_cancel();
            log::info!("HISENSE UWB mode={}", self.gesture.mode().as_str());
        }
        self.sync_gesture_drag()
    }
    pub fn ai_up(&mut self, source: &str) -> Result<(), String> {
        self.gesture.ai_up(source);
        self.sync_gesture_drag()
    }
    pub fn take_mode_notice(&mut self) -> Option<super::indicator::Notice> {
        self.mode_notice.take()
    }
    pub fn profile_notice(&mut self, name: String) {
        self.mode_notice = Some(super::indicator::Notice::Profile(name));
    }
    pub fn trail_notice(&mut self, enabled: bool) {
        self.mode_notice = Some(super::indicator::Notice::Trail(enabled));
    }
    pub fn pointer_motion_active(&self) -> bool {
        self.gesture.moving(Instant::now())
    }
    pub fn confirm_down(&mut self, source: &str) -> Result<(), String> {
        self.gesture.confirm_down(source, Instant::now());
        self.sync_gesture_drag()
    }
    pub fn confirm_up(&mut self, source: &str) -> Result<(), String> {
        let tap = self.gesture.confirm_up(source, Instant::now());
        self.sync_gesture_drag()?;
        if tap {
            if self.tap_until.take().is_some() {
                self.touch_release("gesture:tap")?;
            }
            self.touch_press("gesture:tap")?;
            self.tap_until = Some(Instant::now() + Duration::from_millis(45));
        }
        Ok(())
    }
    pub fn drag_down(&mut self, source: &str) -> Result<(), String> {
        if self.tap_until.take().is_some() {
            self.touch_release("gesture:tap")?;
        }
        self.gesture.drag_down(source);
        if let Err(error) = self.touch_press("gesture:drag") {
            self.gesture.drag_up(source);
            return Err(error);
        }
        Ok(())
    }
    pub fn drag_up(&mut self, source: &str) -> Result<(), String> {
        self.gesture.drag_up(source);
        self.sync_gesture_drag()
    }
    fn sync_gesture_drag(&mut self) -> Result<(), String> {
        let wanted = self.gesture.dragging_at(Instant::now());
        let active = self.touch_owners.contains("gesture:drag");
        if wanted && !active {
            if self.tap_until.take().is_some() {
                self.touch_release("gesture:tap")?;
            }
            self.touch_press("gesture:drag")?;
        } else if !wanted && active {
            self.touch_release("gesture:drag")?;
        }
        Ok(())
    }
    pub fn gesture_tick(&mut self) -> Result<(), String> {
        if self.tap_until.is_some_and(|until| Instant::now() >= until) {
            self.tap_until = None;
            self.touch_release("gesture:tap")?;
        }
        self.sync_gesture_drag()
    }
    pub fn cancel_gestures(&mut self) {
        self.gesture.clear();
        self.tap_until = None;
    }
    pub fn touch_active(&self) -> bool {
        self.touch_position.is_some() && !self.touch_owners.is_empty()
    }
    pub fn touch_press(&mut self, source: &str) -> Result<(), String> {
        if self.touch_owners.is_empty() && self.touch_position.is_some() {
            self.touch_finish(true)?;
        }
        if self.touch_position.is_none() {
            if !self.touch_ready {
                // Match Microsoft's native example: capacity 10, contact ID 0.
                // Keep initialization on this input worker and retry after failures.
                unsafe { InitializeTouchInjection(10, TOUCH_FEEDBACK_NONE) }
                    .map_err(|error| format!("触屏初始化失败：{error}"))?;
                self.touch_ready = true;
            }
            let point = touch_cursor()?;
            if let Err(error) = touch_frame(
                point,
                POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            ) {
                self.touch_ready = false;
                return Err(error);
            }
            self.touch_position = Some(point);
        }
        self.touch_owners.insert(source.into());
        Ok(())
    }
    pub fn touch_update(&mut self) -> Result<(), String> {
        if self.touch_position.is_some() && !self.touch_owners.is_empty() {
            let point = touch_cursor()?;
            touch_frame(
                point,
                POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
            )?;
            self.touch_position = Some(point);
        }
        Ok(())
    }
    pub fn touch_release(&mut self, source: &str) -> Result<(), String> {
        self.touch_owners.remove(source);
        if self.touch_owners.is_empty() {
            self.touch_finish(false)?;
        }
        Ok(())
    }
    pub fn touch_cancel(&mut self) {
        self.touch_owners.clear();
        for _ in 0..3 {
            if self.touch_finish(true).is_ok() {
                return;
            }
        }
        // ERROR_INVALID_PARAMETER cancels the Windows injection contacts.
        // Discard our stale contact too, so the next press can start a new DOWN.
        self.touch_position = None;
        self.touch_ready = false;
        log::warn!("HISENSE touch contact cancellation failed");
    }
    fn touch_finish(&mut self, cancelled: bool) -> Result<(), String> {
        if let Some(point) = self.touch_position {
            // UP must use the exact position of the last successful touch frame.
            if !cancelled {
                // Explicit UPDATE immediately before UP, using exactly the same
                // physical point even for extremely short button presses.
                touch_frame(
                    point,
                    POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
                )?;
            }
            touch_frame(
                point,
                if cancelled {
                    POINTER_FLAG_UP | POINTER_FLAG_CANCELED
                } else {
                    POINTER_FLAG_UP
                },
            )?;
            self.touch_position = None;
        }
        Ok(())
    }
    pub fn press(&mut self, keys: &[u16]) -> bool {
        let mut done = Vec::new();
        for &k in keys {
            let count = self.held.entry(k).or_default();
            if *count == 0 && !key(k, true) {
                for old in done.into_iter().rev() {
                    self.release(&[old]);
                }
                return false;
            }
            *count += 1;
            done.push(k);
        }
        true
    }
    pub fn release(&mut self, keys: &[u16]) {
        for &k in keys.iter().rev() {
            if let Some(n) = self.held.get_mut(&k) {
                if *n > 1 {
                    *n -= 1;
                } else if key(k, false) {
                    self.held.remove(&k);
                }
            }
        }
    }
    pub fn release_all(&mut self) {
        self.touch_cancel();
        for k in self.held.keys().copied().collect::<Vec<_>>() {
            for _ in 0..3 {
                if key(k, false) {
                    break;
                }
            }
        }
        self.held.clear();
    }
}

/// Windows touch contact, including stationary updates for press-and-hold.
/// https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-injecttouchinput
fn touch_frame(point: (i32, i32), flags: POINTER_FLAGS) -> Result<(), String> {
    let mut contact = POINTER_TOUCH_INFO::default();
    contact.pointerInfo.pointerType = PT_TOUCH;
    contact.pointerInfo.pointerId = 0;
    contact.pointerInfo.pointerFlags = flags;
    contact.pointerInfo.ptPixelLocation = POINT {
        x: point.0,
        y: point.1,
    };
    // Optional contact/pressure fields are intentionally absent. Windows supplies
    // their defaults, including a contact rectangle centered on this pixel.
    let phase = if flags.0 & POINTER_FLAG_DOWN.0 != 0 {
        "按下"
    } else if flags.0 & POINTER_FLAG_UP.0 != 0 {
        "抬起"
    } else {
        "移动/保持"
    };
    for _ in 0..3 {
        match unsafe { InjectTouchInput(&[contact]) } {
            Ok(()) => return Ok(()),
            Err(error) if error.code().0 as u32 == 0x80070015 => {
                std::thread::sleep(std::time::Duration::from_millis(1))
            }
            Err(error) => {
                log::warn!(
                    "HISENSE touch phase={phase} x={} y={} flags={:#x} error={error}",
                    point.0,
                    point.1,
                    flags.0
                );
                return Err(format!(
                    "触屏{phase}失败：{error}（坐标 {}, {}）",
                    point.0, point.1
                ));
            }
        }
    }
    Err("触屏点按暂时未就绪，请松开确定键后重试".into())
}

pub fn touch_cursor() -> Result<(i32, i32), String> {
    // GetCursorPos may return DPI-virtualized coordinates on a worker thread.
    // Touch injection always takes physical desktop pixels.
    let mut point = POINT::default();
    unsafe { GetPhysicalCursorPos(&mut point) }.map_err(|e| format!("读取触屏坐标失败：{e}"))?;
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err("无法确定触点所在屏幕，请松开后重试".into());
    }
    let rect = info.rcMonitor;
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return Err("触点屏幕范围无效".into());
    }
    Ok((
        point.x.clamp(rect.left, rect.right - 1),
        point.y.clamp(rect.top, rect.bottom - 1),
    ))
}
impl Drop for Output {
    fn drop(&mut self) {
        self.release_all();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrows_keep_extended_prefix() {
        assert!(extended(0x27, 0x4d));
        assert!(!extended(0x66, 0x4d));
        assert!(extended(0x25, 0xe04b));
    }
}
