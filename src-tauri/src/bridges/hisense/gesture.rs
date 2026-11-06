//! Button state only; no OS input is emitted here.
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Touch,
    Move,
    Slide,
}
impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Touch => "touch",
            Self::Move => "move",
            Self::Slide => "slide",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Touch => "UWB 1/3 原始触摸",
            Self::Move => "UWB 2/3 长按移动",
            Self::Slide => "UWB 3/3 长按滑动",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Touch => Self::Move,
            Self::Move => Self::Slide,
            Self::Slide => Self::Touch,
        }
    }
}
pub struct Gesture {
    confirms: BTreeSet<String>,
    drags: BTreeSet<String>,
    ais: BTreeSet<String>,
    began: Option<Instant>,
    cancel_tap: bool,
    hold: Duration,
    mode: Mode,
}
impl Default for Gesture {
    fn default() -> Self {
        Self {
            confirms: BTreeSet::new(),
            drags: BTreeSet::new(),
            ais: BTreeSet::new(),
            began: None,
            cancel_tap: false,
            hold: Duration::from_millis(350),
            mode: Mode::Touch,
        }
    }
}
impl Gesture {
    pub fn set_threshold(&mut self, ms: u64) {
        self.hold = Duration::from_millis(ms);
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn ai_down(&mut self, source: &str) -> bool {
        let change = self.ais.is_empty();
        self.ais.insert(source.into());
        if change {
            self.mode = self.mode.next();
            self.confirms.clear();
            self.drags.clear();
            self.began = None;
            self.cancel_tap = false;
        }
        change
    }
    pub fn ai_up(&mut self, source: &str) {
        self.ais.remove(source);
    }
    fn confirm_long(&self, now: Instant) -> bool {
        self.began
            .is_some_and(|began| now.duration_since(began) >= self.hold)
    }
    pub fn dragging_at(&self, now: Instant) -> bool {
        !self.drags.is_empty()
            || match self.mode {
                Mode::Touch => !self.confirms.is_empty(),
                Mode::Move => false,
                Mode::Slide => self.confirm_long(now),
            }
    }
    pub fn moving(&self, now: Instant) -> bool {
        matches!(self.mode, Mode::Touch | Mode::Slide)
            || !self.drags.is_empty()
            || self.confirm_long(now)
    }
    pub fn confirm_down(&mut self, source: &str, now: Instant) {
        if self.confirms.is_empty() {
            self.began = Some(now);
            self.cancel_tap = !self.drags.is_empty();
        }
        self.confirms.insert(source.into());
    }
    pub fn confirm_up(&mut self, source: &str, now: Instant) -> bool {
        if !self.confirms.remove(source) || !self.confirms.is_empty() {
            return false;
        }
        let tap = self
            .began
            .take()
            .is_some_and(|began| now.duration_since(began) < self.hold)
            && !self.cancel_tap
            && self.mode != Mode::Touch;
        self.cancel_tap = false;
        tap
    }
    pub fn drag_down(&mut self, source: &str) {
        self.drags.insert(source.into());
        if !self.confirms.is_empty() {
            self.cancel_tap = true;
        }
    }
    pub fn drag_up(&mut self, source: &str) {
        self.drags.remove(source);
    }
    pub fn clear(&mut self) {
        self.confirms.clear();
        self.drags.clear();
        self.ais.clear();
        self.began = None;
        self.cancel_tap = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cycle(g: &mut Gesture) {
        g.ai_down("ai");
        g.ai_up("ai");
    }
    #[test]
    fn ai_cycles_once_per_press() {
        let mut g = Gesture::default();
        assert_eq!(g.mode(), Mode::Touch);
        assert!(g.ai_down("ai"));
        assert_eq!(g.mode(), Mode::Move);
        assert!(!g.ai_down("ai"));
        g.ai_up("ai");
        cycle(&mut g);
        assert_eq!(g.mode(), Mode::Slide);
        cycle(&mut g);
        assert_eq!(g.mode(), Mode::Touch);
    }
    #[test]
    fn touch_mode_is_free_and_contact_is_immediate() {
        let t = Instant::now();
        let mut g = Gesture::default();
        assert!(g.moving(t));
        g.confirm_down("ok", t);
        assert!(g.dragging_at(t));
        assert!(!g.confirm_up("ok", t + Duration::from_millis(80)));
        assert!(!g.dragging_at(t));
        assert!(g.moving(t));
    }
    #[test]
    fn move_mode_short_taps_and_long_only_moves() {
        let t = Instant::now();
        let mut g = Gesture::default();
        cycle(&mut g);
        assert!(!g.moving(t));
        g.confirm_down("ok", t);
        assert!(!g.dragging_at(t));
        assert!(g.confirm_up("ok", t + Duration::from_millis(80)));
        g.confirm_down("ok", t);
        let long = t + Duration::from_millis(400);
        assert!(g.moving(long));
        assert!(!g.dragging_at(long));
        assert!(!g.confirm_up("ok", long));
        assert!(!g.moving(long));
    }
    #[test]
    fn slide_mode_moves_freely_but_only_touches_when_long() {
        let t = Instant::now();
        let mut g = Gesture::default();
        cycle(&mut g);
        cycle(&mut g);
        assert!(g.moving(t));
        g.confirm_down("ok", t);
        assert!(g.moving(t + Duration::from_millis(100)));
        assert!(!g.dragging_at(t + Duration::from_millis(100)));
        assert!(g.confirm_up("ok", t + Duration::from_millis(100)));
        g.confirm_down("ok", t);
        let long = t + Duration::from_millis(400);
        assert!(g.moving(long));
        assert!(g.dragging_at(long));
        assert!(!g.confirm_up("ok", long));
        assert!(!g.dragging_at(long));
        assert!(g.moving(long));
    }
    #[test]
    fn mode_switch_and_aliases_never_add_taps() {
        let t = Instant::now();
        let mut g = Gesture::default();
        g.confirm_down("ok", t);
        cycle(&mut g);
        assert!(!g.confirm_up("ok", t));
        g.confirm_down("key:28", t);
        g.confirm_down("pen:tip", t);
        assert!(!g.confirm_up("key:28", t));
        assert!(g.confirm_up("pen:tip", t + Duration::from_millis(10)));
        g.clear();
        assert_eq!(g.mode(), Mode::Move);
    }
}
