//! Bounded, transient pointer history. No OS input and no coordinate backlog.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const TRAIL_SIDE: i32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);
impl Rgb {
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self(
            u8::from_str_radix(&hex[..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..], 16).ok()?,
        ))
    }
    pub fn floats(self) -> (f32, f32, f32) {
        (self.0 as f32, self.1 as f32, self.2 as f32)
    }
    pub fn premultiplied(self, alpha: f32) -> u32 {
        let a = alpha.clamp(0., 1.);
        ((a * 255.).round() as u32) << 24
            | ((self.0 as f32 * a).round() as u32) << 16
            | ((self.1 as f32 * a).round() as u32) << 8
            | (self.2 as f32 * a).round() as u32
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoticeStyle {
    pub enabled: bool,
    pub transparency_percent: u32,
    pub background: Rgb,
    pub text: Rgb,
}
impl Default for NoticeStyle {
    fn default() -> Self {
        Self {
            enabled: true,
            transparency_percent: 50,
            background: Rgb(23, 31, 43),
            text: Rgb(255, 255, 255),
        }
    }
}
impl NoticeStyle {
    // The text mask stays opaque even when the card background is transparent.
    pub fn pixel(self, edge: f32, ink: f32) -> u32 {
        let ink = ink.clamp(0., 1.);
        let background = edge.clamp(0., 1.)
            * (1. - self.transparency_percent.min(100) as f32 / 100.)
            * (1. - ink);
        let channel =
            |text: u8, bg: u8| (text as f32 * ink + bg as f32 * background).round() as u32;
        (((ink + background) * 255.).round() as u32) << 24
            | channel(self.text.0, self.background.0) << 16
            | channel(self.text.1, self.background.1) << 8
            | channel(self.text.2, self.background.2)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrailOptions {
    pub width_px: u32,
    pub duration_ms: u64,
    pub opacity_percent: u32,
    pub glow_percent: u32,
}
impl Default for TrailOptions {
    fn default() -> Self {
        Self {
            width_px: 6,
            duration_ms: 650,
            opacity_percent: 66,
            glow_percent: 50,
        }
    }
}
impl TrailOptions {
    pub fn normalized(self) -> Self {
        Self {
            width_px: self.width_px.clamp(2, 20),
            duration_ms: self.duration_ms.clamp(150, 2000),
            opacity_percent: self.opacity_percent.clamp(10, 100),
            glow_percent: self.glow_percent.min(100),
        }
    }
    pub fn radius(self) -> f32 {
        self.normalized().width_px as f32 / 2.
    }
    pub fn glow_radius(self) -> f32 {
        (self.radius() * (2. + self.glow_percent.min(100) as f32 / 100. * (8. / 3.))).min(32.)
    }
    fn padding(self) -> i32 {
        self.glow_radius().ceil() as i32 + 2
    }
    fn lifetime(self) -> Duration {
        Duration::from_millis(self.normalized().duration_ms)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub color: Rgb,
    pub pressed_color: Rgb,
    pub trail_enabled: bool,
    pub trail_color: Rgb,
    pub trail: TrailOptions,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            color: Rgb(255, 255, 255),
            pressed_color: Rgb(59, 130, 246),
            trail_enabled: true,
            trail_color: Rgb(34, 211, 238),
            trail: TrailOptions::default(),
        }
    }
}
#[derive(Clone, Copy)]
struct Sample {
    point: (i32, i32),
    at: Instant,
}
#[derive(Default)]
pub struct Trail {
    options: TrailOptions,
    points: VecDeque<Sample>,
    last: Option<(i32, i32)>,
}
pub struct Snapshot {
    pub origin: (i32, i32),
    pub size: (i32, i32),
    pub points: Vec<(f32, f32, f32)>,
}
fn distance(a: (i32, i32), b: (i32, i32)) -> f64 {
    (f64::from(a.0) - f64::from(b.0)).hypot(f64::from(a.1) - f64::from(b.1))
}
impl Trail {
    pub fn configure(&mut self, options: TrailOptions) {
        let options = options.normalized();
        if self.options != options {
            self.clear();
            self.options = options;
        }
    }
    pub fn clear(&mut self) {
        self.points.clear();
        self.last = None;
    }
    pub fn update(&mut self, enabled: bool, point: (i32, i32), now: Instant) {
        if !enabled {
            self.clear();
            return;
        }
        while self
            .points
            .front()
            .is_some_and(|p| now.saturating_duration_since(p.at) >= self.options.lifetime())
        {
            self.points.pop_front();
        }
        if self.last.is_some_and(|old| distance(old, point) < 2.) {
            return;
        }
        if self.last.is_some_and(|old| distance(old, point) > 1000.) {
            self.points.clear();
        }
        self.last = Some(point);
        self.points.push_back(Sample { point, at: now });
        // Bound both painting area and total rasterized path, even for rapid
        // zigzags or coordinates crossing monitors with different origins.
        loop {
            let min_x = self.points.iter().map(|p| p.point.0).min().unwrap();
            let max_x = self.points.iter().map(|p| p.point.0).max().unwrap();
            let min_y = self.points.iter().map(|p| p.point.1).min().unwrap();
            let max_y = self.points.iter().map(|p| p.point.1).max().unwrap();
            let length: f64 = self
                .points
                .iter()
                .zip(self.points.iter().skip(1))
                .map(|(a, b)| distance(a.point, b.point))
                .sum();
            if self.points.len() <= 256
                && i64::from(max_x) - i64::from(min_x)
                    <= i64::from(TRAIL_SIDE - 2 * self.options.padding())
                && i64::from(max_y) - i64::from(min_y)
                    <= i64::from(TRAIL_SIDE - 2 * self.options.padding())
                && length <= 2400.
            {
                break;
            }
            self.points.pop_front();
        }
    }
    pub fn snapshot(&self, now: Instant) -> Option<Snapshot> {
        let active: Vec<_> = self
            .points
            .iter()
            .filter(|p| now.saturating_duration_since(p.at) < self.options.lifetime())
            .collect();
        let min_x = active.iter().map(|p| p.point.0).min()?;
        let max_x = active.iter().map(|p| p.point.0).max()?;
        let min_y = active.iter().map(|p| p.point.1).min()?;
        let max_y = active.iter().map(|p| p.point.1).max()?;
        let origin = (
            min_x.saturating_sub(self.options.padding()),
            min_y.saturating_sub(self.options.padding()),
        );
        let size = (
            (max_x - min_x + 2 * self.options.padding()).clamp(1, TRAIL_SIDE),
            (max_y - min_y + 2 * self.options.padding()).clamp(1, TRAIL_SIDE),
        );
        Some(Snapshot {
            origin,
            size,
            points: active
                .iter()
                .map(|p| {
                    let life = 1.
                        - now.saturating_duration_since(p.at).as_secs_f32()
                            / self.options.lifetime().as_secs_f32();
                    (
                        (p.point.0 - origin.0) as f32,
                        (p.point.1 - origin.1) as f32,
                        life * life,
                    )
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notice_background_transparency_keeps_text_visible_including_black() {
        let mut style = NoticeStyle::default();
        assert_eq!(style.pixel(1., 0.) >> 24, 128);
        assert_eq!(style.pixel(1., 1.), 0xffffffff);
        assert_eq!(style.pixel(0., 0.), 0);
        style.transparency_percent = 100;
        style.text = Rgb(0, 0, 0);
        assert_eq!(style.pixel(1., 0.), 0);
        assert_eq!(style.pixel(1., 1.), 0xff000000);
        style.transparency_percent = 0;
        style.background = Rgb(255, 0, 0);
        assert_eq!(style.pixel(1., 0.), 0xffff0000);
    }
    #[test]
    fn color_validation_and_alpha_are_exact() {
        assert_eq!(Rgb::parse("#12aBef"), Some(Rgb(18, 171, 239)));
        for bad in ["fff", "#fff", "#gg0000", "#白000", "#ffffff00"] {
            assert!(Rgb::parse(bad).is_none());
        }
        assert_eq!(Rgb(255, 128, 0).premultiplied(0.5), 0x80804000);
    }
    #[test]
    fn stationary_pointer_fades_instead_of_adding_fresh_samples() {
        let t = Instant::now();
        let mut trail = Trail::default();
        trail.update(true, (100, 100), t);
        trail.update(true, (100, 100), t + Duration::from_millis(400));
        assert!(
            trail
                .snapshot(t + Duration::from_millis(400))
                .unwrap()
                .points[0]
                .2
                < 0.2
        );
        trail.update(true, (100, 100), t + Duration::from_millis(650));
        assert!(trail.snapshot(t + Duration::from_millis(650)).is_none());
    }
    #[test]
    fn disable_and_reentry_never_connect_an_old_path() {
        let t = Instant::now();
        let mut trail = Trail::default();
        trail.update(true, (10, 10), t);
        trail.update(false, (10, 10), t);
        assert!(trail.snapshot(t).is_none());
        trail.update(true, (500, 500), t);
        assert_eq!(trail.snapshot(t).unwrap().points.len(), 1);
        trail.update(true, (3000, 3000), t);
        assert_eq!(trail.snapshot(t).unwrap().points.len(), 1);
    }
    #[test]
    fn custom_lifetime_and_wide_glow_do_not_clip_or_replay() {
        let t = Instant::now();
        let mut trail = Trail::default();
        trail.configure(TrailOptions {
            width_px: 20,
            duration_ms: 1800,
            glow_percent: 100,
            ..Default::default()
        });
        trail.update(true, (20, 30), t);
        let snapshot = trail.snapshot(t + Duration::from_millis(1000)).unwrap();
        assert!(snapshot.size.0 >= 68 && snapshot.size.1 >= 68);
        assert!(trail.snapshot(t + Duration::from_millis(1800)).is_none());
        trail.configure(TrailOptions::default());
        assert!(trail.snapshot(t).is_none());
    }
    #[test]
    fn rapid_zigzags_and_negative_monitor_origins_stay_bounded() {
        let t = Instant::now();
        let mut trail = Trail::default();
        for i in 0..200 {
            trail.update(
                true,
                (-1600 + i * 5, if i % 2 == 0 { -400 } else { 400 }),
                t,
            );
        }
        let snapshot = trail.snapshot(t).unwrap();
        assert!(snapshot.size.0 <= TRAIL_SIDE && snapshot.size.1 <= TRAIL_SIDE);
        assert!(snapshot.points.len() <= 256 && snapshot.points.len() > 1);
        assert!(snapshot.origin.0 < 0);
    }
}
