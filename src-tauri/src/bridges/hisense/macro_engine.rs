//! Nonblocking macro scheduler. The USB worker remains the sole input owner.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    Once,
    Hold,
    Toggle,
    Sequence,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum Step {
    KeyDown(u16),
    KeyUp(u16),
    MouseDown(bool),
    MouseUp(bool),
    Delay(u64),
}
pub fn valid_key(key: u16) -> bool {
    (1..=0xfe).contains(&key) || key == 0x10d
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Definition {
    pub name: String,
    pub mode: Mode,
    pub color: String,
    pub use_default_delay: bool,
    pub default_delay_ms: u64,
    pub show_key_transitions: bool,
    pub press: Vec<Step>,
    #[serde(default)]
    pub hold: Vec<Step>,
    #[serde(default)]
    pub release: Vec<Step>,
}
impl Definition {
    pub fn realtime(&self) -> bool {
        matches!(self.mode, Mode::Hold | Mode::Sequence)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.contains('\0')
            || super::visual::Rgb::parse(&self.color).is_none()
            || self.default_delay_ms > 10_000
        {
            return Err("宏名称、颜色或标准延迟无效".into());
        }
        let sections = [&self.press, &self.hold, &self.release];
        if sections.iter().all(|s| s.is_empty())
            || (self.mode != Mode::Sequence && self.press.is_empty())
            || sections.iter().any(|s| s.len() > 256)
        {
            return Err("宏每段最多 256 步，执行内容不能为空".into());
        }
        for step in sections.into_iter().flatten() {
            match step {
                Step::KeyDown(key) | Step::KeyUp(key) if !valid_key(*key) => {
                    return Err("宏按键无效".into())
                }
                Step::Delay(ms) if *ms > 60_000 => return Err("单步延迟不能超过 60 秒".into()),
                _ => {}
            }
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Key {
        key: u16,
        down: bool,
    },
    Mouse {
        owner: String,
        right: bool,
        down: bool,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Press,
    Hold,
    Release,
}
struct Run {
    id: String,
    definition: Definition,
    owners: BTreeSet<String>,
    phase: Phase,
    index: usize,
    due: Instant,
    keys: BTreeSet<u16>,
    mouse: BTreeSet<bool>,
}
impl Run {
    fn cleanup(&mut self) -> Vec<Event> {
        let mut events: Vec<_> = std::mem::take(&mut self.keys)
            .into_iter()
            .map(|key| Event::Key { key, down: false })
            .collect();
        events.extend(
            std::mem::take(&mut self.mouse)
                .into_iter()
                .map(|right| Event::Mouse {
                    owner: self.id.clone(),
                    right,
                    down: false,
                }),
        );
        events
    }
    fn steps(&self) -> &[Step] {
        match self.phase {
            Phase::Press => &self.definition.press,
            Phase::Hold => &self.definition.hold,
            Phase::Release => &self.definition.release,
        }
    }
    fn advance(&mut self, now: Instant, budget: &mut usize, events: &mut Vec<Event>) -> bool {
        while *budget > 0 && self.due <= now {
            *budget -= 1;
            if self.index >= self.steps().len() {
                match self.definition.mode {
                    Mode::Once => return true,
                    Mode::Hold if self.owners.is_empty() => return true,
                    Mode::Hold | Mode::Toggle => {
                        self.index = 0;
                        self.due = now + Duration::from_millis(20);
                    }
                    Mode::Sequence => match self.phase {
                        Phase::Press => {
                            self.phase = if self.owners.is_empty() {
                                Phase::Release
                            } else {
                                Phase::Hold
                            };
                            self.index = 0;
                        }
                        Phase::Hold => {
                            if self.owners.is_empty() {
                                self.phase = Phase::Release;
                            }
                            self.index = 0;
                            self.due = now + Duration::from_millis(20);
                        }
                        Phase::Release => return true,
                    },
                }
                continue;
            }
            let step = self.steps()[self.index].clone();
            self.index += 1;
            match step {
                Step::Delay(ms) => {
                    if !self.definition.use_default_delay {
                        self.due = now + Duration::from_millis(ms);
                    }
                }
                Step::KeyDown(key) => {
                    if self.keys.insert(key) {
                        events.push(Event::Key { key, down: true });
                    }
                }
                Step::KeyUp(key) => {
                    if self.keys.remove(&key) {
                        events.push(Event::Key { key, down: false });
                    }
                }
                Step::MouseDown(right) => {
                    if self.mouse.insert(right) {
                        events.push(Event::Mouse {
                            owner: self.id.clone(),
                            right,
                            down: true,
                        });
                    }
                }
                Step::MouseUp(right) => {
                    if self.mouse.remove(&right) {
                        events.push(Event::Mouse {
                            owner: self.id.clone(),
                            right,
                            down: false,
                        });
                    }
                }
            }
            if self.definition.use_default_delay && !matches!(step, Step::Delay(_)) {
                self.due = now + Duration::from_millis(self.definition.default_delay_ms);
            }
        }
        false
    }
}
#[derive(Default)]
pub struct Engine {
    runs: BTreeMap<String, Run>,
}
impl Engine {
    pub fn down(
        &mut self,
        owner: &str,
        id: &str,
        definition: &Definition,
        now: Instant,
    ) -> Result<Vec<Event>, String> {
        if let Some(run) = self.runs.get_mut(id) {
            if definition.mode == Mode::Toggle {
                return Ok(self.runs.remove(id).unwrap().cleanup());
            }
            run.owners.insert(owner.into());
            return Ok(Vec::new());
        }
        if self.runs.len() >= 16 {
            return Err("同时运行的宏最多 16 个，请先停止宏".into());
        }
        definition.validate()?;
        self.runs.insert(
            id.into(),
            Run {
                id: format!("macro:{id}"),
                definition: definition.clone(),
                owners: [owner.into()].into(),
                phase: Phase::Press,
                index: 0,
                due: now,
                keys: BTreeSet::new(),
                mouse: BTreeSet::new(),
            },
        );
        Ok(Vec::new())
    }
    pub fn up(&mut self, owner: &str, now: Instant) -> Vec<Event> {
        let mut events = Vec::new();
        let mut remove = Vec::new();
        for (id, run) in &mut self.runs {
            if !run.owners.remove(owner) || !run.owners.is_empty() {
                continue;
            }
            match run.definition.mode {
                Mode::Hold => {
                    events.extend(run.cleanup());
                    remove.push(id.clone());
                }
                Mode::Sequence => {
                    run.phase = Phase::Release;
                    run.index = 0;
                    run.due = now;
                }
                _ => {}
            }
        }
        for id in remove {
            self.runs.remove(&id);
        }
        events
    }
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        let mut events = Vec::new();
        let mut budget = 32;
        let mut done = Vec::new();
        for (id, run) in &mut self.runs {
            if run.advance(now, &mut budget, &mut events) {
                events.extend(run.cleanup());
                done.push(id.clone());
            }
        }
        for id in done {
            self.runs.remove(&id);
        }
        events
    }
    pub fn clear(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.runs)
            .into_values()
            .flat_map(|mut r| r.cleanup())
            .collect()
    }
    pub fn active(&self) -> usize {
        self.runs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition(mode: Mode) -> Definition {
        Definition {
            name: "test".into(),
            mode,
            color: "#3478f6".into(),
            use_default_delay: false,
            default_delay_ms: 50,
            show_key_transitions: true,
            press: vec![Step::KeyDown(65), Step::Delay(100), Step::KeyUp(65)],
            hold: vec![],
            release: vec![Step::KeyDown(66), Step::KeyUp(66)],
        }
    }
    #[test]
    fn standard_delay_replaces_explicit_recorded_delays() {
        let t = Instant::now();
        let mut e = Engine::default();
        let mut d = definition(Mode::Once);
        d.use_default_delay = true;
        e.down("a", "x", &d, t).unwrap();
        assert_eq!(
            e.tick(t),
            vec![Event::Key {
                key: 65,
                down: true
            }]
        );
        assert_eq!(
            e.tick(t + Duration::from_millis(50)),
            vec![Event::Key {
                key: 65,
                down: false
            }]
        );
        e.tick(t + Duration::from_millis(100));
        assert_eq!(e.active(), 0);
    }
    #[test]
    fn once_continues_after_release_and_is_not_queued_twice() {
        let t = Instant::now();
        let mut e = Engine::default();
        let d = definition(Mode::Once);
        e.down("a", "x", &d, t).unwrap();
        assert_eq!(
            e.tick(t),
            vec![Event::Key {
                key: 65,
                down: true
            }]
        );
        assert!(e.up("a", t).is_empty());
        e.down("a", "x", &d, t).unwrap();
        assert_eq!(
            e.tick(t + Duration::from_millis(100)),
            vec![Event::Key {
                key: 65,
                down: false
            }]
        );
        assert_eq!(e.active(), 0);
    }
    #[test]
    fn hold_cancel_and_toggle_stop_release_owned_keys() {
        let t = Instant::now();
        for mode in [Mode::Hold, Mode::Toggle] {
            let mut e = Engine::default();
            let d = definition(mode);
            e.down("a", "x", &d, t).unwrap();
            e.tick(t);
            let events = if mode == Mode::Hold {
                e.up("a", t)
            } else {
                e.up("a", t);
                e.down("b", "x", &d, t).unwrap()
            };
            assert_eq!(
                events,
                vec![Event::Key {
                    key: 65,
                    down: false
                }]
            );
            assert_eq!(e.active(), 0);
        }
    }
    #[test]
    fn sequence_release_interrupts_wait_and_runs_release_segment() {
        let t = Instant::now();
        let mut e = Engine::default();
        e.down("a", "x", &definition(Mode::Sequence), t).unwrap();
        e.tick(t);
        e.up("a", t);
        assert_eq!(
            e.tick(t),
            vec![
                Event::Key {
                    key: 66,
                    down: true
                },
                Event::Key {
                    key: 66,
                    down: false
                },
                Event::Key {
                    key: 65,
                    down: false
                }
            ]
        );
    }
    #[test]
    fn zero_delay_loops_are_bounded_and_clear_releases_mouse() {
        let t = Instant::now();
        let mut e = Engine::default();
        let mut d = definition(Mode::Toggle);
        d.press = vec![Step::MouseDown(false)];
        e.down("a", "x", &d, t).unwrap();
        assert!(e.tick(t).len() <= 32);
        assert!(!e.clear().is_empty());
        assert_eq!(e.active(), 0);
        d.press = vec![Step::Delay(60001)];
        assert!(d.validate().is_err());
    }
}
