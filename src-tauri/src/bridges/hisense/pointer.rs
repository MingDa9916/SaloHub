#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Default)]
pub struct PointerFilter {
    previous: Option<(f64, f64)>,
    target: (f64, f64),
    smooth: (f64, f64),
    last_output: Option<(i32, i32)>,
    sample: Option<(f64, f64)>,
    speed: f64,
}
impl PointerFilter {
    pub fn reset(&mut self) {
        self.previous = None;
        self.last_output = None;
        self.sample = None;
        self.speed = 0.;
    }
    pub fn update(
        &mut self,
        raw: (u16, u16),
        actual: (i32, i32),
        b: Bounds,
        gain: f64,
        deadzone: f64,
        ms: f64,
        dt: f64,
        precision_ratio: Option<f64>,
    ) -> (i32, i32) {
        let (rx, ry) = (raw.0 as f64, raw.1 as f64);
        let Some((px, py)) = self.previous else {
            self.previous = Some((rx, ry));
            self.target = (actual.0 as f64, actual.1 as f64);
            self.smooth = self.target;
            self.last_output = Some(actual);
            self.sample = Some((rx, ry));
            self.speed = 0.;
            return actual;
        };
        if self
            .last_output
            .is_some_and(|(x, y)| (x - actual.0).abs() > 3 || (y - actual.1).abs() > 3)
        {
            self.target = (actual.0 as f64, actual.1 as f64);
            self.smooth = self.target;
            self.speed = 0.;
        }
        // Estimate hand speed from consecutive sensor samples, independently of
        // the output gain, desktop size and the accumulated deadzone anchor.
        let (sx, sy) = self.sample.replace((rx, ry)).unwrap_or((rx, ry));
        let elapsed = dt.clamp(0.001, 0.2);
        let measured = ((rx - sx) * 1919. / 21240.).hypot((ry - sy) * 1079. / 15980.) / elapsed;
        // Slow down promptly when approaching a target; smooth acceleration so
        // adjacent samples cannot abruptly switch between precision and travel.
        let speed_tau = if measured < self.speed { 0.012 } else { 0.030 };
        self.speed += (1. - (-elapsed / speed_tau).exp()) * (measured - self.speed);
        let gain = gain
            * precision_ratio
                .map(|ratio| speed_multiplier(self.speed, ratio))
                .unwrap_or(1.);
        let (dx, dy) = (
            (rx - px) * (b.width - 1.) / 21240. * gain,
            (ry - py) * (b.height - 1.) / 15980. * gain,
        );
        let distance = dx.hypot(dy);
        if distance >= deadzone {
            self.target.0 = (self.target.0 + dx).clamp(b.left, b.left + b.width - 1.);
            self.target.1 = (self.target.1 + dy).clamp(b.top, b.top + b.height - 1.);
            self.previous = Some((rx, ry));
        }
        let velocity = distance / dt.clamp(0.001, 0.2);
        let tau = (ms / 1000.) / (1. + velocity / 300.);
        let alpha = if tau <= 0. {
            1.
        } else {
            1. - (-dt.clamp(0.001, 0.2) / tau.max(0.002)).exp()
        };
        self.smooth.0 += alpha * (self.target.0 - self.smooth.0);
        self.smooth.1 += alpha * (self.target.1 - self.smooth.1);
        let output = (self.smooth.0.round() as i32, self.smooth.1.round() as i32);
        self.last_output = Some(output);
        output
    }
}

fn speed_multiplier(speed: f64, minimum: f64) -> f64 {
    let t = ((speed - 40.) / (400. - 40.)).clamp(0., 1.);
    let curve = t * t * (3. - 2. * t);
    minimum.clamp(0.1, 0.75) + (1. - minimum.clamp(0.1, 0.75)) * curve
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bounds() -> Bounds {
        Bounds {
            left: 0.,
            top: 0.,
            width: 1920.,
            height: 1080.,
        }
    }
    #[test]
    fn jitter_is_ignored_but_slow_motion_accumulates() {
        let mut f = PointerFilter::default();
        let b = bounds();
        assert_eq!(
            f.update((10000, 7000), (500, 500), b, 2., 2., 0., 0.01, None),
            (500, 500)
        );
        for x in [10001, 10000, 9999, 10000] {
            assert_eq!(
                f.update((x, 7000), (500, 500), b, 2., 2., 0., 0.01, None),
                (500, 500)
            );
        }
        assert!(
            f.update((10030, 7000), (500, 500), b, 2., 2., 0., 0.01, None)
                .0
                > 500
        );
    }
    #[test]
    fn edges_have_no_overshoot_debt() {
        let mut f = PointerFilter::default();
        let b = bounds();
        f.update((10000, 7000), (1919, 500), b, 3.5, 0., 0., 0.01, None);
        f.update((20000, 7000), (1919, 500), b, 3.5, 0., 0., 0.01, None);
        assert!(
            f.update((19900, 7000), (1919, 500), b, 3.5, 0., 0., 0.01, None)
                .0
                < 1919
        );
        f.reset();
        assert_eq!(
            f.update((20000, 100), (300, 400), b, 3.5, 0., 0., 0.01, None),
            (300, 400)
        );
    }
    fn trace(step: u16, count: u16, dt: f64, precision: Option<f64>) -> i32 {
        let mut f = PointerFilter::default();
        let mut point = (500, 500);
        let b = bounds();
        f.update((10000, 7000), point, b, 2.5, 0., 0., dt, precision);
        for n in 1..=count {
            point = f.update(
                (10000 + n * step, 7000),
                point,
                b,
                2.5,
                0.,
                0.,
                dt,
                precision,
            );
        }
        point.0 - 500
    }
    #[test]
    fn slow_motion_is_precise_while_fast_travel_keeps_full_gain() {
        let slow = trace(10, 100, 0.02, Some(0.25));
        let fast = trace(100, 10, 0.008, Some(0.25));
        let baseline = trace(100, 10, 0.008, None);
        assert!(slow > 0 && slow < baseline / 3);
        assert!(fast as f64 > baseline as f64 * 0.90);
        assert_eq!(trace(10, 100, 0.02, None), baseline);
    }
    #[test]
    fn fine_subpixel_motion_accumulates_and_reentry_does_not_jump() {
        assert!(trace(1, 100, 0.02, Some(0.25)) >= 5);
        let mut f = PointerFilter::default();
        let b = bounds();
        f.update((10000, 7000), (500, 500), b, 2.5, 0., 0., 0.008, Some(0.25));
        f.update((12000, 7000), (500, 500), b, 2.5, 0., 0., 0.008, Some(0.25));
        f.reset();
        assert_eq!(
            f.update((20000, 1000), (800, 600), b, 2.5, 0., 0., 0.008, Some(0.25)),
            (800, 600)
        );
        assert_eq!(f.speed, 0.);
    }
    #[test]
    fn sensitivity_drops_quickly_after_fast_motion() {
        let mut f = PointerFilter::default();
        let b = bounds();
        let mut p = (500, 500);
        f.update((10000, 7000), p, b, 2.5, 0., 0., 0.008, Some(0.25));
        for n in 1..=10 {
            p = f.update(
                (10000 + n * 100, 7000),
                p,
                b,
                2.5,
                0.,
                0.,
                0.008,
                Some(0.25),
            );
        }
        for n in 1..=8 {
            p = f.update((11000 + n, 7000), p, b, 2.5, 0., 0., 0.008, Some(0.25));
        }
        assert!(speed_multiplier(f.speed, 0.25) < 0.30);
    }
}
