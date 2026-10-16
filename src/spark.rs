//! Sparkline data: a request's recent response times, kept for this
//! session only. Pure: no clock, no drawing. The view passes in how far the
//! newest point's animation has run.

/// How many responses a sparkline keeps.
pub const LEN: usize = 20;

/// One response: how long it took and whether its status was a success.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub ms: f32,
    pub ok: bool,
}

/// A request's recent samples, oldest first, at most `LEN`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Series {
    samples: Vec<Sample>,
}

impl Series {
    pub fn push(&mut self, sample: Sample) {
        self.samples.push(sample);
        if self.samples.len() > LEN {
            self.samples.remove(0);
        }
    }

    pub fn last(&self) -> Option<Sample> {
        self.samples.last().copied()
    }

    /// The points of the line in a `width` × `height` box, y growing down
    /// as on screen. Slow responses sit high. `t` (0 to 1) is how far the
    /// newest point has eased in from the one before it; 1 means at rest.
    pub fn points(&self, width: f32, height: f32, t: f32) -> Vec<(f32, f32)> {
        let n = self.samples.len();
        if n == 0 {
            return Vec::new();
        }
        let min = self
            .samples
            .iter()
            .map(|s| s.ms)
            .fold(f32::INFINITY, f32::min);
        let max = self
            .samples
            .iter()
            .map(|s| s.ms)
            .fold(f32::NEG_INFINITY, f32::max);
        let span = max - min;
        // Equal values draw a flat line through the middle.
        let y = |ms: f32| {
            if span <= f32::EPSILON {
                height / 2.0
            } else {
                height - (ms - min) / span * height
            }
        };
        // Points sit at fixed slots from the right, so the line grows leftward
        // and every row's newest point lines up.
        let step = width / (LEN - 1) as f32;
        let x = |i: usize| width - (n - 1 - i) as f32 * step;
        let mut points: Vec<(f32, f32)> = self
            .samples
            .iter()
            .enumerate()
            .map(|(i, s)| (x(i), y(s.ms)))
            .collect();
        if n >= 2 {
            let t = ease(t.clamp(0.0, 1.0));
            let (px, py) = points[n - 2];
            let (lx, ly) = points[n - 1];
            points[n - 1] = (px + (lx - px) * t, py + (ly - py) * t);
        }
        points
    }
}

/// Ease-out: fast start, gentle stop.
fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(ms: f32) -> Sample {
        Sample { ms, ok: true }
    }

    #[test]
    fn keeps_only_the_newest_len_samples() {
        let mut s = Series::default();
        for i in 0..(LEN + 5) {
            s.push(ok(i as f32));
        }
        assert_eq!(s.points(40.0, 10.0, 1.0).len(), LEN);
        assert_eq!(s.last(), Some(ok((LEN + 4) as f32)));
    }

    #[test]
    fn empty_series_has_no_points() {
        assert!(Series::default().points(40.0, 10.0, 1.0).is_empty());
    }

    #[test]
    fn single_point_sits_right_and_mid_height() {
        let mut s = Series::default();
        s.push(ok(100.0));
        assert_eq!(s.points(40.0, 10.0, 1.0), vec![(40.0, 5.0)]);
    }

    #[test]
    fn slow_is_high_and_fast_is_low() {
        let mut s = Series::default();
        s.push(ok(100.0));
        s.push(ok(300.0));
        let p = s.points(38.0, 10.0, 1.0);
        assert_eq!(p[0].1, 10.0, "fastest at the bottom");
        assert_eq!(p[1].1, 0.0, "slowest at the top");
        assert_eq!(p[1].0, 38.0, "newest at the right edge");
        assert_eq!(p[0].0, 36.0, "one slot to the left");
    }

    #[test]
    fn newest_point_eases_in_from_the_previous_one() {
        let mut s = Series::default();
        s.push(ok(100.0));
        s.push(ok(300.0));
        let start = s.points(38.0, 10.0, 0.0);
        assert_eq!(start[1], start[0], "starts on the previous point");
        let end = s.points(38.0, 10.0, 1.0);
        assert_eq!(end[1], (38.0, 0.0));
    }
}
