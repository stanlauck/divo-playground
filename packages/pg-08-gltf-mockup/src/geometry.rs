// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unit meshes for placeholder props and the wireframe camera marker.
//!
//! Prop shapes fill the box x, z in [-0.5, 0.5], y in [0, 1]: a node scale
//! equal to the prop size and a translation equal to its bottom centre place
//! them exactly. Triangles wind counter-clockwise seen from outside, as glTF
//! expects for front faces.

use std::f64::consts::{PI, TAU};

use crate::model::Shape;

pub const LINES: u32 = 1;
pub const TRIANGLES: u32 = 4;

const CYLINDER_SEGMENTS: u16 = 32;
const SPHERE_RINGS: u16 = 12;
const SPHERE_SEGMENTS: u16 = 24;

pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    /// Absent for line primitives.
    pub normals: Option<Vec<[f32; 3]>>,
    pub indices: Vec<u16>,
    pub mode: u32,
}

impl Geometry {
    pub fn is_finite(&self) -> bool {
        self.positions.iter().flatten().all(|c| c.is_finite())
    }
}

pub fn shape(shape: Shape) -> Geometry {
    match shape {
        Shape::Box => unit_box(),
        Shape::Cylinder => unit_cylinder(CYLINDER_SEGMENTS),
        Shape::Sphere => unit_sphere(SPHERE_RINGS, SPHERE_SEGMENTS),
    }
}

/// Wireframe camera marker in camera space: apex at the lens, the image
/// rectangle `depth` metres down -Z, and a triangle above it marking "up".
pub fn frustum(depth: f64, focal_length_mm: f64, sensor_mm: [f64; 2]) -> Geometry {
    let half_w = depth * sensor_mm[0] / (2.0 * focal_length_mm);
    let half_h = depth * sensor_mm[1] / (2.0 * focal_length_mm);
    let tri = 0.35 * half_w;
    let base = half_h + 0.1 * half_h;
    let z = -depth;
    let positions = [
        [0.0, 0.0, 0.0],
        [-half_w, -half_h, z],
        [half_w, -half_h, z],
        [half_w, half_h, z],
        [-half_w, half_h, z],
        [-tri, base, z],
        [tri, base, z],
        [0.0, base + tri, z],
    ];
    Geometry {
        positions: positions.iter().map(|p| p.map(|c| c as f32)).collect(),
        normals: None,
        indices: vec![
            0, 1, 0, 2, 0, 3, 0, 4, 1, 2, 2, 3, 3, 4, 4, 1, 5, 6, 6, 7, 7, 5,
        ],
        mode: LINES,
    }
}

#[derive(Default)]
struct Triangles {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u16>,
}

impl Triangles {
    fn vertex(&mut self, position: [f64; 3], normal: [f64; 3]) -> u16 {
        self.positions.push(position.map(|c| c as f32));
        self.normals.push(normal.map(|c| c as f32));
        u16::try_from(self.positions.len() - 1).expect("unit meshes stay below 65536 vertices")
    }

    fn finish(self) -> Geometry {
        Geometry {
            positions: self.positions,
            normals: Some(self.normals),
            indices: self.indices,
            mode: TRIANGLES,
        }
    }
}

fn unit_box() -> Geometry {
    // (normal, u, v) with u x v = normal, so corners taken in (u, v) order wind CCW.
    const FACES: [[[f64; 3]; 3]; 6] = [
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [[0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    ];
    let mut mesh = Triangles::default();
    for [n, u, v] in FACES {
        let corner = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(a, b)| {
            let p = [0, 1, 2]
                .map(|k| 0.5 * n[k] + a * u[k] + b * v[k] + if k == 1 { 0.5 } else { 0.0 });
            mesh.vertex(p, n)
        });
        mesh.indices.extend([
            corner[0], corner[1], corner[2], corner[0], corner[2], corner[3],
        ]);
    }
    mesh.finish()
}

fn unit_cylinder(segments: u16) -> Geometry {
    let ring: Vec<(f64, f64)> = (0..segments)
        .map(|i| {
            let t = TAU * f64::from(i) / f64::from(segments);
            (t.cos(), t.sin())
        })
        .collect();
    let mut mesh = Triangles::default();

    let side: Vec<(u16, u16)> = ring
        .iter()
        .map(|&(c, s)| {
            let n = [c, 0.0, s];
            (
                mesh.vertex([0.5 * c, 0.0, 0.5 * s], n),
                mesh.vertex([0.5 * c, 1.0, 0.5 * s], n),
            )
        })
        .collect();
    for (i, &(b0, t0)) in side.iter().enumerate() {
        let (b1, t1) = side[(i + 1) % side.len()];
        mesh.indices.extend([b0, t0, t1, b0, t1, b1]);
    }

    for (y, normal) in [(1.0, [0.0, 1.0, 0.0]), (0.0, [0.0, -1.0, 0.0])] {
        let centre = mesh.vertex([0.0, y, 0.0], normal);
        let rim: Vec<u16> = ring
            .iter()
            .map(|&(c, s)| mesh.vertex([0.5 * c, y, 0.5 * s], normal))
            .collect();
        for (i, &a) in rim.iter().enumerate() {
            let b = rim[(i + 1) % rim.len()];
            if normal[1] > 0.0 {
                mesh.indices.extend([centre, b, a]);
            } else {
                mesh.indices.extend([centre, a, b]);
            }
        }
    }
    mesh.finish()
}

fn unit_sphere(rings: u16, segments: u16) -> Geometry {
    let mut mesh = Triangles::default();
    let north = mesh.vertex([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
    let bands: Vec<Vec<u16>> = (1..rings)
        .map(|r| {
            let phi = PI * f64::from(r) / f64::from(rings);
            (0..segments)
                .map(|s| {
                    let theta = TAU * f64::from(s) / f64::from(segments);
                    let n = [phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin()];
                    mesh.vertex([0.5 * n[0], 0.5 + 0.5 * n[1], 0.5 * n[2]], n)
                })
                .collect()
        })
        .collect();
    let south = mesh.vertex([0.0, 0.0, 0.0], [0.0, -1.0, 0.0]);

    let count = usize::from(segments);
    let (first, last) = (&bands[0], &bands[bands.len() - 1]);
    for s in 0..count {
        let t = (s + 1) % count;
        mesh.indices.extend([north, first[t], first[s]]);
        mesh.indices.extend([south, last[s], last[t]]);
    }
    for pair in bands.windows(2) {
        let (upper, lower) = (&pair[0], &pair[1]);
        for s in 0..count {
            let t = (s + 1) % count;
            mesh.indices
                .extend([lower[s], upper[s], upper[t], lower[s], upper[t], lower[t]]);
        }
    }
    mesh.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f64; 3] {
        [0, 1, 2].map(|k| f64::from(a[k]) - f64::from(b[k]))
    }

    #[test]
    fn shapes_fill_the_unit_box_and_face_outwards() {
        for shape_kind in [Shape::Box, Shape::Cylinder, Shape::Sphere] {
            let g = shape(shape_kind);
            let normals = g.normals.as_ref().unwrap();
            assert_eq!(g.mode, TRIANGLES);
            assert_eq!(g.indices.len() % 3, 0);
            for k in 0..3 {
                let lo = g
                    .positions
                    .iter()
                    .map(|p| p[k])
                    .fold(f32::INFINITY, f32::min);
                let hi = g
                    .positions
                    .iter()
                    .map(|p| p[k])
                    .fold(f32::NEG_INFINITY, f32::max);
                let expected = if k == 1 { (0.0, 1.0) } else { (-0.5, 0.5) };
                assert!(
                    (lo - expected.0).abs() < 1e-6 && (hi - expected.1).abs() < 1e-6,
                    "{shape_kind:?} axis {k}"
                );
            }
            for n in normals {
                let len = n.iter().map(|c| c * c).sum::<f32>().sqrt();
                assert!((len - 1.0).abs() < 1e-6);
            }
            for tri in g.indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|i| usize::from(tri[i]));
                assert!(
                    a != b && b != c && a != c,
                    "degenerate triangle in {shape_kind:?}"
                );
                let face = crate::math::cross(
                    sub(g.positions[b], g.positions[a]),
                    sub(g.positions[c], g.positions[a]),
                );
                let dot: f64 = (0..3).map(|k| face[k] * f64::from(normals[a][k])).sum();
                assert!(dot > 0.0, "{shape_kind:?} triangle {tri:?} winds inwards");
            }
        }
    }

    #[test]
    fn frustum_matches_the_lens() {
        let g = frustum(2.0, 50.0, [36.0, 24.0]);
        assert_eq!(g.mode, LINES);
        assert!(g.normals.is_none());
        assert_eq!(g.indices.len(), 22);
        assert!(
            g.indices
                .iter()
                .all(|&i| usize::from(i) < g.positions.len())
        );
        assert_eq!(g.positions[3], [0.72, 0.48, -2.0]);
        assert!(g.positions[7][1] > g.positions[3][1]);
    }
}
