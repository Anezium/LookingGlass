//! Vita analog look. Input has already had its radial dead zone rescaled to [0,1].
use macroquad::prelude::*;

const CURVE: f32 = 1.7;

pub fn rates(sensitivity: f32) -> Vec2 {
    // Preserve 1 as the familiar default while making both ends of the UI useful.
    let s = sensitivity.clamp(0.2, 3.);
    if s <= 1. {
        vec2(60., 40.).lerp(vec2(210., 135.), (s - 0.2) / 0.8)
    } else {
        vec2(210., 135.).lerp(vec2(420., 270.), (s - 1.) / 2.)
    }
}

fn seconds(elapsed: f32) -> f32 {
    if elapsed.is_finite() { elapsed.clamp(0., 0.1) } else { 0. }
}

fn delta(axis: Vec2, sensitivity: f32, inverted: bool, elapsed: f32) -> Vec2 {
    let magnitude = axis.length();
    if magnitude == 0. { return Vec2::ZERO; }
    let curved = axis / magnitude * magnitude.min(1.).powf(CURVE);
    curved * rates(sensitivity) * vec2(-1., if inverted { -1. } else { 1. })
        * (std::f32::consts::PI / 180.) * seconds(elapsed)
}

pub fn turn(axis: Vec2, sensitivity: f32, inverted: bool, elapsed: f32, yaw: &mut f32, pitch: &mut f32) {
    let change = delta(axis, sensitivity, inverted, elapsed);
    // No arithmetic at neutral: replay angles (including signed zero) remain bit-identical.
    if change == Vec2::ZERO { return; }
    *yaw = (*yaw + change.x).rem_euclid(std::f32::consts::TAU);
    *pitch = (*pitch + change.y).clamp(-1.5, 1.5);
}

#[derive(Default)]
pub struct RateLog {
    seconds: f32,
    yaw: f32,
    frames: u32,
}
impl RateLog {
    pub fn sample(&mut self, axis: Vec2, elapsed: f32, before: f32, after: f32) {
        // Horizontal end-stop only; diagonals intentionally divide the angular velocity.
        if axis.x.abs() < 0.99 || axis.y.abs() > 0.05 || seconds(elapsed) == 0. {
            *self = Self::default();
            return;
        }
        self.seconds += seconds(elapsed);
        self.yaw += (after - before + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.frames += 1;
        if self.seconds >= 1. {
            eprintln!("[lg/camera-stick] full_deflection frames={} seconds={:.4} yaw_deg_s={:.3}",
                self.frames, self.seconds, self.yaw.to_degrees() / self.seconds);
            *self = Self::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_independent_of_frame_time() {
        for sensitivity in [0.2, 1., 3.] {
            for axis in [Vec2::X, Vec2::Y, vec2(0.3, -0.4), vec2(0.6, 0.8)] {
                let expected = delta(axis, sensitivity, false, 0.1) * 10.;
                for hz in [20, 30, 40, 60, 120] {
                    let mut total = Vec2::ZERO;
                    for _ in 0..hz { total += delta(axis, sensitivity, false, 1. / hz as f32); }
                    assert!(total.distance(expected) < 0.00002, "{hz} {sensitivity} {axis:?}");
                }
                let mut total = Vec2::ZERO;
                for dt in [0.025, 0.038].into_iter().cycle().take(20) {
                    total += delta(axis, sensitivity, false, dt);
                }
                assert!(total.distance(expected * 0.63) < 0.00002);
            }
        }
    }

    #[test]
    fn useful_sensitivity_endpoints_and_continuity() {
        assert_eq!(rates(0.2), vec2(60., 40.));
        assert_eq!(rates(1.), vec2(210., 135.));
        assert_eq!(rates(3.), vec2(420., 270.));
        let mut last = rates(0.2);
        for i in 21..=300 {
            let next = rates(i as f32 / 100.);
            assert!(next.cmpgt(last).all());
            assert!(next.distance(last) < 3.);
            last = next;
        }
    }

    #[test]
    fn response_curve_preserves_direction_and_fine_aim() {
        let full = delta(Vec2::X, 1., false, 0.025).length();
        let half = delta(Vec2::X * 0.5, 1., false, 0.025).length();
        assert!((half / full - 0.5_f32.powf(1.7)).abs() < 1e-6);
        let diagonal = delta(vec2(0.6, 0.8), 1., false, 0.025);
        let unit = delta(Vec2::ONE, 1., false, 0.025);
        assert!((diagonal.x / delta(Vec2::X, 1., false, 0.025).x - 0.6).abs() < 1e-6);
        assert!((unit.x / unit.y + 210. / 135.).abs() < 1e-6);
    }

    #[test]
    fn measured_yaw_rate_includes_wrap_and_matches_default_and_maximum() {
        for hz in [30, 40] {
            for sensitivity in [1., 3.] {
                let mut log = RateLog::default();
                let (mut yaw, mut pitch) = (0.01, 0.);
                for _ in 0..hz / 2 {
                    let before = yaw;
                    turn(Vec2::X, sensitivity, false, 1. / hz as f32, &mut yaw, &mut pitch);
                    log.sample(Vec2::X, 1. / hz as f32, before, yaw);
                }
                let measured = log.yaw.to_degrees() / log.seconds;
                assert!((measured + rates(sensitivity).x).abs() < 0.002);
                log.sample(Vec2::ZERO, 0.025, yaw, yaw);
                assert_eq!(log.frames, 0);
                assert_eq!(log.seconds, 0.);
            }
        }
    }

    #[test]
    fn neutral_inversion_wrap_limits_and_stalls() {
        let (mut yaw, mut pitch) = (-0.0_f32, -0.0_f32);
        turn(Vec2::ZERO, 3., true, 0.038, &mut yaw, &mut pitch);
        assert_eq!(yaw.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(pitch.to_bits(), (-0.0_f32).to_bits());
        for dt in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(delta(Vec2::ONE, 1., false, dt), Vec2::ZERO);
        }
        assert_eq!(delta(Vec2::X, 1., false, 5.), delta(Vec2::X, 1., false, 0.1));
        let normal = delta(Vec2::Y, 1., false, 0.025);
        assert_eq!(delta(Vec2::Y, 1., true, 0.025), -normal);
        turn(Vec2::X, 1., false, 0.025, &mut yaw, &mut pitch);
        assert!((yaw.to_degrees() - 354.75).abs() < 0.0001);
        pitch = 1.49;
        turn(Vec2::Y, 3., false, 0.038, &mut yaw, &mut pitch);
        assert_eq!(pitch, 1.5);
    }
}
