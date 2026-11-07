//! Explicit, bounded keyboard macro recording using the existing permanent hook.
//! Data stays in memory until the user saves a macro; injected input is ignored.
use super::macro_engine::Step;
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    sync::LazyLock,
    time::{Duration, Instant},
};

struct Recording {
    session: u64,
    active: bool,
    started: Instant,
    last: Instant,
    held: BTreeSet<u16>,
    drain: BTreeSet<u16>,
    steps: Vec<Step>,
}
impl Default for Recording {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            session: 0,
            active: false,
            started: now,
            last: now,
            held: BTreeSet::new(),
            drain: BTreeSet::new(),
            steps: Vec::new(),
        }
    }
}
static STATE: LazyLock<Mutex<Recording>> = LazyLock::new(|| Mutex::new(Recording::default()));
#[derive(Serialize)]
pub struct Snapshot {
    pub session: u64,
    pub active: bool,
    pub steps: Vec<Step>,
}
fn finish(r: &mut Recording) {
    r.active = false;
    let held = std::mem::take(&mut r.held);
    for key in held {
        r.steps.push(Step::KeyUp(key));
        r.drain.insert(key);
    }
}
pub fn start() -> Result<u64, String> {
    let mut r = STATE.lock();
    if r.active || !r.drain.is_empty() {
        return Err("请先停止录制并松开所有按键".into());
    }
    r.session = r.session.wrapping_add(1);
    r.steps.clear();
    r.started = Instant::now();
    r.last = r.started;
    r.active = true;
    Ok(r.session)
}
pub fn snapshot(session: u64, stop: bool) -> Result<Snapshot, String> {
    let mut r = STATE.lock();
    if r.session != session {
        return Err("录制会话已结束".into());
    }
    if r.active && (stop || r.started.elapsed() > Duration::from_secs(120)) {
        finish(&mut r);
    }
    Ok(Snapshot {
        session: r.session,
        active: r.active,
        steps: r.steps.clone(),
    })
}
pub fn active() -> bool {
    let mut r = STATE.lock();
    if r.active && r.started.elapsed() > Duration::from_secs(120) {
        finish(&mut r);
    }
    r.active
}
pub fn key(vk: u32, msg: u32, injected: bool, extended: bool) -> bool {
    if injected {
        return false;
    }
    let down = matches!(msg, 0x100 | 0x104);
    let up = matches!(msg, 0x101 | 0x105);
    if !down && !up {
        return false;
    }
    let key = if vk == 0x0d && extended {
        0x10d
    } else {
        vk as u16
    };
    let mut r = STATE.lock();
    if r.active && r.started.elapsed() > Duration::from_secs(120) {
        finish(&mut r);
    }
    if !r.active {
        return if r.drain.contains(&key) {
            if up {
                r.drain.remove(&key);
            }
            true
        } else {
            false
        };
    }
    if !super::macro_engine::valid_key(key) {
        return true;
    }
    // A key held before recording must still be allowed to release in Windows.
    if up && !r.held.contains(&key) {
        return false;
    }
    let changed = if down {
        r.held.insert(key)
    } else {
        r.held.remove(&key)
    };
    if !changed {
        return true;
    }
    // Reserve space for key releases so truncation can never leave keys held.
    if down && r.steps.len() + r.held.len() + 3 >= 256 {
        if down {
            r.drain.insert(key);
        }
        finish(&mut r);
        return true;
    }
    let now = Instant::now();
    let delay = now.duration_since(r.last).as_millis().min(60_000) as u64;
    if !r.steps.is_empty() && delay > 0 && r.steps.len() + r.held.len() + 3 < 256 {
        r.steps.push(Step::Delay(delay));
    }
    r.steps.push(if down {
        Step::KeyDown(key)
    } else {
        Step::KeyUp(key)
    });
    r.last = now;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_recording_ignores_injected_repeats_and_drains_keys_on_stop() {
        let session = start().unwrap();
        assert!(!key(65, 0x100, true, false));
        assert!(!key(0xa2, 0x101, false, false));
        assert!(key(65, 0x100, false, false));
        assert!(key(65, 0x100, false, false));
        assert!(key(65, 0x101, false, false));
        assert!(key(13, 0x100, false, true));
        let snapshot = snapshot(session, true).unwrap();
        assert!(!snapshot.active);
        assert_eq!(
            snapshot
                .steps
                .iter()
                .filter(|s| matches!(s, Step::KeyDown(65)))
                .count(),
            1
        );
        assert!(snapshot.steps.contains(&Step::KeyDown(0x10d)));
        assert!(snapshot.steps.contains(&Step::KeyUp(0x10d)));
        assert!(key(13, 0x101, false, true));
        assert!(!key(66, 0x100, false, false));
        let session = start().unwrap();
        for _ in 0..400 {
            key(65, 0x100, false, false);
            key(65, 0x101, false, false);
        }
        let result = super::snapshot(session, true).unwrap();
        assert!(result.steps.len() <= 256);
        assert!(!result.active);
        let mut held = BTreeSet::new();
        for step in result.steps {
            match step {
                Step::KeyDown(k) => {
                    held.insert(k);
                }
                Step::KeyUp(k) => {
                    held.remove(&k);
                }
                _ => {}
            }
        }
        assert!(held.is_empty());
    }
}
