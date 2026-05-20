//! Motion — interpolation between layout states.
//!
//! Per prysm/layout.md §3.4:
//!
//! `μ(e, s_0, s_1, t) = s_0 + (s_1 - s_0) · α(t)`
//!
//! With duration `T = 150ms` and easing `α = cubic-bezier(0.25, 0.1, 0.25, 1.0)`
//! (the CSS "ease" curve). Motion is convention, not axiom — the layout
//! protocol outputs the target state `s_1`; the renderer/motion system
//! interpolates between successive computations.
//!
//! One duration, one curve. Uniformity is legibility.

/// Standard motion duration in milliseconds.
pub const MOTION_DURATION_MS: u32 = 150;

/// Standard easing curve control points: cubic-bezier(0.25, 0.1, 0.25, 1.0).
pub const EASE_X1: f32 = 0.25;
pub const EASE_Y1: f32 = 0.10;
pub const EASE_X2: f32 = 0.25;
pub const EASE_Y2: f32 = 1.00;

/// Cubic-bezier easing function. Maps progress `t ∈ [0,1]` through the
/// CSS "ease" curve.
///
/// Implementation: Newton's method finds parametric `u` such that
/// `bezier_x(u) = t`, then evaluates `bezier_y(u)`. Cubic Bezier with
/// P0=(0,0) and P3=(1,1).
pub fn ease(t: f32) -> f32 {
    cubic_bezier(t, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2)
}

/// Generic cubic-bezier evaluation. CSS-style: P1=(x1,y1), P2=(x2,y2).
pub fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    let u = solve_bezier_t(t, x1, x2);
    bezier_axis(u, y1, y2)
}

/// Compute one axis of a cubic Bezier at parameter `u` given control values.
/// P0 = 0, P3 = 1; mid control values are `c1` and `c2`.
fn bezier_axis(u: f32, c1: f32, c2: f32) -> f32 {
    // Polynomial form: a*u^3 + b*u^2 + c*u with
    //   a = 1 - 3*c2 + 3*c1
    //   b = 3*c2 - 6*c1
    //   c = 3*c1
    let a = 1.0 - 3.0 * c2 + 3.0 * c1;
    let b = 3.0 * c2 - 6.0 * c1;
    let c = 3.0 * c1;
    ((a * u + b) * u + c) * u
}

/// Derivative of `bezier_axis` w.r.t. `u`.
fn bezier_axis_deriv(u: f32, c1: f32, c2: f32) -> f32 {
    let a = 1.0 - 3.0 * c2 + 3.0 * c1;
    let b = 3.0 * c2 - 6.0 * c1;
    let c = 3.0 * c1;
    3.0 * a * u * u + 2.0 * b * u + c
}

/// Solve for parameter `u` such that `bezier_axis(u, x1, x2) = target`.
/// Newton's method, ~5–10 iterations until convergence.
fn solve_bezier_t(target: f32, x1: f32, x2: f32) -> f32 {
    let mut u = target;
    for _ in 0..16 {
        let cur = bezier_axis(u, x1, x2);
        let err = cur - target;
        if err.abs() < 1e-6 {
            return u;
        }
        let d = bezier_axis_deriv(u, x1, x2);
        if d.abs() < 1e-9 {
            // Tangent is flat — fall back to bisection.
            return bisect_bezier_t(target, x1, x2);
        }
        u -= err / d;
    }
    u
}

/// Bisection fallback when Newton's tangent vanishes.
fn bisect_bezier_t(target: f32, x1: f32, x2: f32) -> f32 {
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        let cur = bezier_axis(mid, x1, x2);
        if cur < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Types interpolable by motion. Implemented for f32 and integer
/// coordinates by rounding to nearest after linear interpolation.
pub trait Interpolate: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Interpolate for u32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        let a = self as f32;
        let b = other as f32;
        (a + (b - a) * t).round().max(0.0) as u32
    }
}

impl Interpolate for i32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        let a = self as f32;
        let b = other as f32;
        (a + (b - a) * t).round() as i32
    }
}

/// Per-element motion state: the previous (`from`) and target (`to`)
/// values plus elapsed time since the transition began.
#[derive(Debug, Clone, Copy)]
pub struct MotionState<T: Interpolate> {
    pub from: T,
    pub to: T,
    pub elapsed_ms: u32,
}

impl<T: Interpolate> MotionState<T> {
    pub fn new(from: T, to: T) -> Self {
        Self {
            from,
            to,
            elapsed_ms: 0,
        }
    }

    pub fn tick(&mut self, dt_ms: u32) {
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
    }

    /// Current interpolated value at the current elapsed time.
    pub fn current(self) -> T {
        let t = (self.elapsed_ms as f32 / MOTION_DURATION_MS as f32).clamp(0.0, 1.0);
        self.from.lerp(self.to, ease(t))
    }

    /// True once the motion has reached its target.
    pub fn is_done(self) -> bool {
        self.elapsed_ms >= MOTION_DURATION_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_at_endpoints() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
    }

    #[test]
    fn ease_is_monotone() {
        let mut prev = 0.0;
        for k in 0..=100 {
            let t = (k as f32) / 100.0;
            let e = ease(t);
            assert!(e + 1e-5 >= prev, "ease must be monotone non-decreasing");
            prev = e;
        }
    }

    #[test]
    fn ease_midpoint_is_above_linear() {
        // CSS "ease" is faster than linear in the middle.
        assert!(ease(0.5) > 0.5);
    }

    #[test]
    fn f32_lerp() {
        assert_eq!(0.0f32.lerp(10.0, 0.0), 0.0);
        assert_eq!(0.0f32.lerp(10.0, 1.0), 10.0);
        assert!((0.0f32.lerp(10.0, 0.5) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn u32_lerp_rounds() {
        assert_eq!(0u32.lerp(10, 0.5), 5);
        assert_eq!(0u32.lerp(11, 0.5), 6); // 5.5 rounds to 6
    }

    #[test]
    fn motion_state_progresses() {
        let mut m = MotionState::new(0.0f32, 100.0);
        assert_eq!(m.current(), 0.0);
        m.tick(MOTION_DURATION_MS / 2);
        let mid = m.current();
        assert!(mid > 0.0 && mid < 100.0);
        m.tick(MOTION_DURATION_MS / 2);
        assert!(m.is_done());
        assert!((m.current() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn motion_clamps_at_end() {
        let mut m = MotionState::new(0u32, 50);
        m.tick(MOTION_DURATION_MS * 10);
        assert_eq!(m.current(), 50);
    }
}
