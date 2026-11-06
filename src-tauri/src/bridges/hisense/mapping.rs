//! Device-independent click/hold recognizer. The USB worker owns timing, so
//! waiting for a second click never blocks incoming UWB coordinates.
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Plan<A> {
    pub single: A,
    pub long: Option<A>,
    pub multi: BTreeMap<u8, A>,
    pub interval: Duration,
    pub hold: Duration,
}
pub enum Event<A> {
    Down {
        owner: String,
        button: String,
        trigger: String,
        action: A,
    },
    Up(String),
}
struct State<A> {
    plan: Plan<A>,
    sources: BTreeSet<String>,
    since: Instant,
    held: bool,
    count: u8,
    deadline: Option<Instant>,
}
pub struct Mapping<A> {
    states: BTreeMap<String, State<A>>,
    sources: BTreeMap<String, String>,
    pulses: BTreeMap<String, Instant>,
    sequence: u64,
}
impl<A> Default for Mapping<A> {
    fn default() -> Self {
        Self {
            states: BTreeMap::new(),
            sources: BTreeMap::new(),
            pulses: BTreeMap::new(),
            sequence: 0,
        }
    }
}
impl<A: Clone> Mapping<A> {
    fn pulse(
        &mut self,
        button: &str,
        count: u8,
        plan: &Plan<A>,
        now: Instant,
        events: &mut Vec<Event<A>>,
    ) {
        let action = if count == 1 {
            Some(&plan.single)
        } else {
            plan.multi.get(&count)
        };
        if let Some(action) = action {
            self.sequence = self.sequence.wrapping_add(1);
            let owner = format!("mapping:tap:{button}:{}", self.sequence);
            self.pulses
                .insert(owner.clone(), now + Duration::from_millis(45));
            events.push(Event::Down {
                owner,
                button: button.into(),
                trigger: format!("click:{count}"),
                action: action.clone(),
            });
        }
    }
    pub fn down(
        &mut self,
        button: &str,
        source: &str,
        plan: Plan<A>,
        now: Instant,
    ) -> Vec<Event<A>> {
        let events = self.tick(now);
        if self.sources.contains_key(source) {
            return events;
        }
        self.sources.insert(source.into(), button.into());
        let state = self.states.entry(button.into()).or_insert(State {
            plan,
            sources: BTreeSet::new(),
            since: now,
            held: false,
            count: 0,
            deadline: None,
        });
        if state.sources.is_empty() {
            state.since = now;
            state.held = false;
            // A second DOWN belongs to the pending sequence; it may itself become
            // a long press, in which case no earlier short click may leak out.
            state.deadline = None;
        }
        state.sources.insert(source.into());
        events
    }
    pub fn up(&mut self, source: &str, now: Instant) -> Option<Vec<Event<A>>> {
        let button = self.sources.remove(source)?;
        let mut events = self.tick(now);
        let mut state = self.states.remove(&button)?;
        state.sources.remove(source);
        if !state.sources.is_empty() {
            self.states.insert(button, state);
            return Some(events);
        }
        if state.held {
            events.push(Event::Up(format!("mapping:hold:{button}")));
        } else {
            state.count += 1;
            let maximum = state.plan.multi.keys().copied().max().unwrap_or(1);
            if state.count >= maximum {
                self.pulse(&button, state.count, &state.plan, now, &mut events);
            } else {
                state.deadline = Some(now + state.plan.interval);
                self.states.insert(button, state);
            }
        }
        Some(events)
    }
    pub fn tick(&mut self, now: Instant) -> Vec<Event<A>> {
        let mut events = Vec::new();
        let expired: Vec<_> = self
            .pulses
            .iter()
            .filter(|(_, until)| now >= **until)
            .map(|(owner, _)| owner.clone())
            .collect();
        for owner in expired {
            self.pulses.remove(&owner);
            events.push(Event::Up(owner));
        }
        for button in self.states.keys().cloned().collect::<Vec<_>>() {
            let mut state = self.states.remove(&button).unwrap();
            if !state.sources.is_empty()
                && !state.held
                && now.duration_since(state.since) >= state.plan.hold
            {
                state.held = true;
                state.count = 0;
                state.deadline = None;
                events.push(Event::Down {
                    owner: format!("mapping:hold:{button}"),
                    button: button.clone(),
                    trigger: "long".into(),
                    action: state
                        .plan
                        .long
                        .as_ref()
                        .unwrap_or(&state.plan.single)
                        .clone(),
                });
            }
            if state.sources.is_empty() && state.deadline.is_some_and(|until| now >= until) {
                self.pulse(&button, state.count, &state.plan, now, &mut events);
            } else {
                self.states.insert(button, state);
            }
        }
        events
    }
    /// Call together with output.release_all on pause/disconnect/config changes.
    pub fn clear(&mut self) {
        self.states.clear();
        self.sources.clear();
        self.pulses.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan() -> Plan<&'static str> {
        Plan {
            single: "single",
            long: Some("long"),
            multi: [(2, "double"), (3, "triple"), (4, "quad")].into(),
            interval: Duration::from_millis(300),
            hold: Duration::from_millis(280),
        }
    }
    fn actions(events: Vec<Event<&'static str>>) -> Vec<&'static str> {
        events
            .into_iter()
            .filter_map(|e| {
                if let Event::Down { action, .. } = e {
                    Some(action)
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn short_click_waits_only_when_multi_is_configured() {
        let t = Instant::now();
        let mut m = Mapping::default();
        assert!(actions(m.down("menu", "a", plan(), t)).is_empty());
        assert!(actions(m.up("a", t + Duration::from_millis(20)).unwrap()).is_empty());
        assert!(actions(m.tick(t + Duration::from_millis(319))).is_empty());
        assert_eq!(
            actions(m.tick(t + Duration::from_millis(320))),
            vec!["single"]
        );
        assert!(actions(m.tick(t + Duration::from_secs(2))).is_empty());
        let mut p = plan();
        p.multi.clear();
        m.down("menu", "a", p, t + Duration::from_secs(3));
        assert_eq!(
            actions(m.up("a", t + Duration::from_millis(3020)).unwrap()),
            vec!["single"]
        );
    }
    #[test]
    fn exact_click_count_fires_once_without_lower_actions() {
        for count in 2..=4 {
            let t = Instant::now();
            let mut m = Mapping::default();
            let mut got = Vec::new();
            for n in 0..count {
                got.extend(actions(m.down(
                    "home",
                    "a",
                    plan(),
                    t + Duration::from_millis(n * 60),
                )));
                got.extend(actions(
                    m.up("a", t + Duration::from_millis(n * 60 + 20)).unwrap(),
                ));
            }
            got.extend(actions(m.tick(t + Duration::from_secs(1))));
            assert_eq!(
                got,
                vec![match count {
                    2 => "double",
                    3 => "triple",
                    _ => "quad",
                }]
            );
        }
    }
    #[test]
    fn hold_cancels_pending_click_and_stops_on_release() {
        let t = Instant::now();
        let mut m = Mapping::default();
        m.down("home", "a", plan(), t);
        m.up("a", t + Duration::from_millis(20));
        m.down("home", "a", plan(), t + Duration::from_millis(60));
        assert_eq!(
            actions(m.tick(t + Duration::from_millis(340))),
            vec!["long"]
        );
        assert!(matches!(
            m.up("a", t + Duration::from_millis(400))
                .unwrap()
                .as_slice(),
            [Event::Up(_)]
        ));
        assert!(actions(m.tick(t + Duration::from_secs(2))).is_empty());
    }
    #[test]
    fn delayed_worker_still_recognizes_hold_before_up() {
        let t = Instant::now();
        let mut m = Mapping::default();
        m.down("back", "a", plan(), t);
        let events = m.up("a", t + Duration::from_millis(300)).unwrap();
        assert!(matches!(
            events.as_slice(),
            [Event::Down { action: "long", .. }, Event::Up(_)]
        ));
    }
    #[test]
    fn duplicate_sources_do_not_count_as_extra_clicks() {
        let t = Instant::now();
        let mut m = Mapping::default();
        let mut p = plan();
        p.multi.clear();
        m.down("ok", "keyboard", p.clone(), t);
        m.down("ok", "pen", p.clone(), t);
        assert!(m
            .up("keyboard", t + Duration::from_millis(20))
            .unwrap()
            .is_empty());
        assert_eq!(
            actions(m.up("pen", t + Duration::from_millis(30)).unwrap()),
            vec!["single"]
        );
        assert!(m.up("pen", t + Duration::from_millis(40)).is_none());
    }
    #[test]
    fn clear_cancels_click_hold_and_delayed_pulse_without_replay() {
        let t = Instant::now();
        let mut m = Mapping::default();
        m.down("home", "a", plan(), t);
        m.up("a", t + Duration::from_millis(20));
        m.down("menu", "b", plan(), t);
        m.clear();
        assert!(m.tick(t + Duration::from_secs(3)).is_empty());
        assert!(m.up("b", t + Duration::from_secs(3)).is_none());
    }
}
