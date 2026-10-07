// SPDX-License-Identifier: MIT OR Apache-2.0

//! Small vector and quaternion helpers. Quaternions are `[x, y, z, w]`, as in glTF.

pub type Vec3 = [f64; 3];
pub type Quat = [f64; 4];

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn length(v: Vec3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[cfg(test)]
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn axis_angle(axis: usize, angle: f64) -> Quat {
    let mut q = [0.0, 0.0, 0.0, (angle / 2.0).cos()];
    q[axis] = (angle / 2.0).sin();
    q
}

pub fn yaw(angle: f64) -> Quat {
    axis_angle(1, angle)
}

/// Camera orientation `Ry(pan) * Rx(tilt) * Rz(roll)`, all in radians.
///
/// At zero the camera looks down -Z with +Y up. Positive pan turns left,
/// positive tilt looks up, positive roll turns the top of the frame left.
pub fn pan_tilt_roll(pan: f64, tilt: f64, roll: f64) -> Quat {
    quat_mul(
        quat_mul(axis_angle(1, pan), axis_angle(0, tilt)),
        axis_angle(2, roll),
    )
}

/// Pan and tilt (radians) that point the camera's -Z axis along `dir`.
///
/// Straight up or down the pan is undefined; 0 is used so the top of the
/// frame faces -Z (looking down) or +Z (looking up).
pub fn pan_tilt_towards(dir: Vec3) -> (f64, f64) {
    let horizontal = dir[0].hypot(dir[2]);
    let pan = if horizontal > 1e-12 * length(dir) {
        (-dir[0]).atan2(-dir[2])
    } else {
        0.0
    };
    (pan, dir[1].atan2(horizontal))
}

#[cfg(test)]
pub fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = [q[0], q[1], q[2]];
    let t = cross(u, v).map(|c| 2.0 * c);
    let ut = cross(u, t);
    [
        v[0] + q[3] * t[0] + ut[0],
        v[1] + q[3] * t[1] + ut[1],
        v[2] + q[3] * t[2] + ut[2],
    ]
}

/// Drops floating-point noise (and negative zero) so output stays readable.
pub fn tidy(v: f64) -> f64 {
    if v.abs() < 1e-12 { 0.0 } else { v }
}

pub fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn close(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn identity_looks_down_negative_z() {
        let q = pan_tilt_roll(0.0, 0.0, 0.0);
        assert!(close(rotate(q, [0.0, 0.0, -1.0]), [0.0, 0.0, -1.0]));
    }

    #[test]
    fn sign_conventions() {
        let fwd = [0.0, 0.0, -1.0];
        assert!(close(
            rotate(pan_tilt_roll(FRAC_PI_2, 0.0, 0.0), fwd),
            [-1.0, 0.0, 0.0]
        ));
        assert!(close(
            rotate(pan_tilt_roll(0.0, FRAC_PI_2, 0.0), fwd),
            [0.0, 1.0, 0.0]
        ));
        let up = rotate(pan_tilt_roll(0.0, 0.0, FRAC_PI_2), [0.0, 1.0, 0.0]);
        assert!(close(up, [-1.0, 0.0, 0.0]));
    }

    #[test]
    fn towards_round_trips_and_keeps_horizon_level() {
        for dir in [
            [1.0, 0.2, 0.0],
            [-0.3, -0.5, 2.0],
            [0.0, 0.0, 1.0],
            [0.4, 0.9, -0.1],
        ] {
            let n = dir.map(|c| c / length(dir));
            let (pan, tilt) = pan_tilt_towards(dir);
            let q = pan_tilt_roll(pan, tilt, 0.0);
            assert!(close(rotate(q, [0.0, 0.0, -1.0]), n), "{dir:?}");
            assert!(
                rotate(q, [1.0, 0.0, 0.0])[1].abs() < 1e-9,
                "horizon {dir:?}"
            );
        }
    }

    #[test]
    fn straight_down_is_defined() {
        let (pan, tilt) = pan_tilt_towards([0.0, -3.0, 0.0]);
        assert_eq!(pan, 0.0);
        let q = pan_tilt_roll(pan, tilt, 0.0);
        assert!(close(rotate(q, [0.0, 0.0, -1.0]), [0.0, -1.0, 0.0]));
        assert!(close(rotate(q, [0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]));
    }

    #[test]
    fn srgb_conversion() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-12);
        assert!((srgb_to_linear(0.5) - 0.214_041_14).abs() < 1e-6);
    }
}
