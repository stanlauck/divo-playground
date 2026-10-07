// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end checks. Output is read back with the independent `gltf` crate,
//! which validates the document, and compared with the input.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use gltf::Semantic;
use gltf::buffer::Source;
use gltf::camera::Projection;
use gltf::mesh::Mode;
use gltf_mockup::{Error, ShotScene, WriteOptions, from_json, to_glb, to_gltf};
use serde_json::Value;

const SAMPLE: &str = include_str!("../samples/kitchen-blocking.json");
const DATA_URI: &str = "data:application/octet-stream;base64,";

struct Asset {
    doc: gltf::Document,
    buffers: Vec<Vec<u8>>,
}

fn sample() -> ShotScene {
    from_json(SAMPLE).expect("sample is valid")
}

fn read_gltf(text: &str) -> Asset {
    let gltf::Gltf { document, .. } =
        gltf::Gltf::from_slice(text.as_bytes()).expect("output passes gltf validation");
    let buffers = document
        .buffers()
        .map(|buffer| {
            let Source::Uri(uri) = buffer.source() else {
                panic!(".gltf buffers must be embedded")
            };
            let data = STANDARD
                .decode(uri.strip_prefix(DATA_URI).expect("base64 data URI"))
                .unwrap();
            assert_eq!(data.len(), buffer.length());
            data
        })
        .collect();
    Asset {
        doc: document,
        buffers,
    }
}

fn read_glb(bytes: &[u8]) -> Asset {
    let gltf::Gltf { document, blob } =
        gltf::Gltf::from_slice(bytes).expect("output passes gltf validation");
    let buffers = document
        .buffers()
        .map(|buffer| {
            assert!(matches!(buffer.source(), Source::Bin));
            let data = blob.clone().expect("BIN chunk");
            assert!(data.len() >= buffer.length());
            data
        })
        .collect();
    Asset {
        doc: document,
        buffers,
    }
}

fn both(scene: &ShotScene, options: &WriteOptions) -> [Asset; 2] {
    [
        read_gltf(&to_gltf(scene, options).unwrap()),
        read_glb(&to_glb(scene, options).unwrap()),
    ]
}

fn extras(node: &gltf::Node) -> Value {
    node.extras()
        .as_ref()
        .map_or(Value::Null, |raw| serde_json::from_str(raw.get()).unwrap())
}

fn node<'a>(doc: &'a gltf::Document, name: &str) -> gltf::Node<'a> {
    doc.nodes()
        .find(|n| n.name() == Some(name))
        .unwrap_or_else(|| panic!("no node named {name}"))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn rotate(q: [f32; 4], v: [f64; 3]) -> [f64; 3] {
    let [x, y, z, w] = q.map(f64::from);
    let t = cross([x, y, z], v).map(|c| 2.0 * c);
    let u = cross([x, y, z], t);
    [0, 1, 2].map(|k| v[k] + w * t[k] + u[k])
}

fn assert_close(actual: [f64; 3], expected: [f64; 3], tolerance: f64, what: &str) {
    assert!(
        (0..3).all(|k| (actual[k] - expected[k]).abs() < tolerance),
        "{what}: {actual:?} != {expected:?}"
    );
}

fn srgb_to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn sample_writes_valid_gltf_and_glb() {
    let scene = sample();
    for asset in both(&scene, &WriteOptions::default()) {
        let doc = &asset.doc;
        assert_eq!(doc.cameras().count(), scene.shots.len());
        assert_eq!(
            doc.nodes().count(),
            2 + 2 * scene.shots.len() + scene.props.len()
        );
        let root = doc.default_scene().expect("default scene");
        assert_eq!(root.name(), scene.scene.as_deref());
        let roots: Vec<_> = root.nodes().map(|n| n.name().unwrap().to_owned()).collect();
        assert_eq!(roots, ["Shots", "Props"]);
        assert_eq!(asset.buffers.len(), 1);
    }
}

#[test]
fn ids_round_trip_through_node_extras() {
    let scene = sample();
    let mut expected = Vec::new();
    for shot in &scene.shots {
        expected.push(("shot".to_owned(), shot.id.clone()));
        expected.push(("shot_marker".to_owned(), shot.id.clone()));
    }
    for prop in &scene.props {
        expected.push(("prop".to_owned(), prop.id.clone()));
    }
    for asset in both(&scene, &WriteOptions::default()) {
        let tagged: Vec<(String, String)> = asset
            .doc
            .nodes()
            .filter_map(|n| {
                let e = extras(&n);
                Some((e["kind"].as_str()?.to_owned(), e["id"].as_str()?.to_owned()))
            })
            .collect();
        assert_eq!(tagged, expected);

        for shot in &scene.shots {
            let n = node(&asset.doc, &shot.id);
            let e = extras(&n);
            assert_eq!(e["name"].as_str(), shot.name.as_deref());
            assert_eq!(e["focal_length_mm"], shot.camera.focal_length());
            assert_eq!(e["sensor_mm"], serde_json::json!(shot.camera.sensor()));
            let marker = n.children().next().expect("marker child");
            assert_eq!(extras(&marker)["id"], shot.id.as_str());
        }
        for prop in &scene.props {
            let e = extras(&node(&asset.doc, &prop.id));
            assert_eq!(e["shape"], prop.shape.as_str());
            assert_eq!(e["name"].as_str(), prop.name.as_deref());
        }
    }
}

#[test]
fn cameras_aim_and_match_the_lens() {
    let scene = sample();
    for asset in both(&scene, &WriteOptions::default()) {
        for shot in &scene.shots {
            let cam = &shot.camera;
            let n = node(&asset.doc, &shot.id);
            let (translation, q, scale) = n.transform().decomposed();
            assert_close(translation.map(f64::from), cam.position, 1e-5, &shot.id);
            assert_eq!(scale, [1.0; 3]);

            let forward = rotate(q, [0.0, 0.0, -1.0]);
            let right = rotate(q, [1.0, 0.0, 0.0]);
            let tilt = match cam.look_at {
                Some(target) => {
                    let d = [0, 1, 2].map(|k| target[k] - cam.position[k]);
                    let len = d.iter().map(|c| c * c).sum::<f64>().sqrt();
                    assert_close(forward, d.map(|c| c / len), 1e-6, &shot.id);
                    (d[1] / len).asin()
                }
                None => {
                    let (pan, tilt) = (
                        cam.pan_deg.unwrap().to_radians(),
                        cam.tilt_deg.unwrap().to_radians(),
                    );
                    let expected = [-pan.sin() * tilt.cos(), tilt.sin(), -pan.cos() * tilt.cos()];
                    assert_close(forward, expected, 1e-6, &shot.id);
                    tilt
                }
            };
            let roll = cam.roll_deg.unwrap_or(0.0).to_radians();
            assert!(
                (right[1] - roll.sin() * tilt.cos()).abs() < 1e-6,
                "{}: roll",
                shot.id
            );

            let camera = n.camera().expect("camera");
            let Projection::Perspective(p) = camera.projection() else {
                panic!("{}: expected a perspective camera", shot.id)
            };
            let ([width, height], focal) = (cam.sensor(), cam.focal_length());
            assert!((f64::from(p.yfov()) - 2.0 * (height / (2.0 * focal)).atan()).abs() < 1e-6);
            assert!((f64::from(p.aspect_ratio().unwrap()) - width / height).abs() < 1e-6);
            assert!((f64::from(p.znear()) - cam.near()).abs() < 1e-6);
            match (p.zfar(), cam.far_m) {
                (Some(actual), Some(expected)) => {
                    assert!((f64::from(actual) - expected).abs() < 1e-4)
                }
                (None, None) => {}
                other => panic!("{}: zfar {other:?}", shot.id),
            }
        }
    }
}

#[test]
fn props_sit_on_their_bottom_centre() {
    let scene = sample();
    for asset in both(&scene, &WriteOptions::default()) {
        for prop in &scene.props {
            let n = node(&asset.doc, &prop.id);
            let (translation, q, scale) = n.transform().decomposed();
            assert_close(translation.map(f64::from), prop.position, 1e-5, &prop.id);
            assert_close(scale.map(f64::from), prop.size, 1e-5, &prop.id);
            let yaw = prop.yaw_deg.unwrap_or(0.0).to_radians();
            assert_close(
                rotate(q, [1.0, 0.0, 0.0]),
                [yaw.cos(), 0.0, -yaw.sin()],
                1e-6,
                &prop.id,
            );

            let primitive = n.mesh().unwrap().primitives().next().unwrap();
            let bounds = primitive.bounding_box();
            assert_close(bounds.min.map(f64::from), [-0.5, 0.0, -0.5], 1e-6, &prop.id);
            assert_close(bounds.max.map(f64::from), [0.5, 1.0, 0.5], 1e-6, &prop.id);

            let color = primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_factor();
            let rgb = prop.rgb();
            for k in 0..3 {
                assert!(
                    (f64::from(color[k]) - srgb_to_linear(rgb[k])).abs() < 1e-6,
                    "{}",
                    prop.id
                );
            }
            assert_eq!(color[3], 1.0);
        }
    }
}

#[test]
fn geometry_reads_back_through_accessors() {
    let scene = sample();
    for asset in both(&scene, &WriteOptions::default()) {
        let mut modes = Vec::new();
        for primitive in asset.doc.meshes().flat_map(|m| m.primitives()) {
            let reader = primitive.reader(|b| asset.buffers.get(b.index()).map(Vec::as_slice));
            let positions: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
            let indices: Vec<usize> = reader
                .read_indices()
                .unwrap()
                .into_u32()
                .map(|i| i as usize)
                .collect();
            assert!(indices.iter().all(|&i| i < positions.len()));
            let bounds = primitive.bounding_box();
            for p in &positions {
                assert!((0..3).all(|k| bounds.min[k] <= p[k] && p[k] <= bounds.max[k]));
            }
            match primitive.mode() {
                Mode::Triangles => {
                    let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
                    assert_eq!(normals.len(), positions.len());
                    assert_eq!(indices.len() % 3, 0);
                    let at = |i: usize| positions[i].map(f64::from);
                    for tri in indices.chunks(3) {
                        let [a, b, c] = [at(tri[0]), at(tri[1]), at(tri[2])];
                        let face = cross(
                            [0, 1, 2].map(|k| b[k] - a[k]),
                            [0, 1, 2].map(|k| c[k] - a[k]),
                        );
                        let dot: f64 = (0..3)
                            .map(|k| face[k] * f64::from(normals[tri[0]][k]))
                            .sum();
                        assert!(dot > 0.0, "triangle {tri:?} winds inwards");
                    }
                }
                Mode::Lines => {
                    assert!(reader.read_normals().is_none());
                    assert_eq!(indices.len() % 2, 0);
                }
                other => panic!("unexpected primitive mode {other:?}"),
            }
            modes.push(primitive.mode());
        }
        assert!(modes.contains(&Mode::Lines) && modes.contains(&Mode::Triangles));
    }
}

#[test]
fn meshes_and_geometry_are_shared() {
    let doc = read_gltf(&to_gltf(&sample(), &WriteOptions::default()).unwrap()).doc;
    let mesh = |name: &str| node(&doc, name).mesh().unwrap();
    let positions = |name: &str| {
        mesh(name)
            .primitives()
            .next()
            .unwrap()
            .get(&Semantic::Positions)
            .unwrap()
            .index()
    };
    assert_eq!(mesh("chair-1").index(), mesh("chair-2").index());
    assert_ne!(mesh("stand-in-a").index(), mesh("stand-in-b").index());
    assert_eq!(positions("stand-in-a"), positions("stand-in-b"));
    assert_eq!(positions("table"), positions("crate"));
    assert_eq!(
        doc.meshes().count(),
        doc.meshes()
            .map(|m| m.name().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
}

#[test]
fn glb_layout_follows_the_spec() {
    let glb = to_glb(&sample(), &WriteOptions::default()).unwrap();
    let word = |at: usize| u32::from_le_bytes(glb[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(&glb[0..4], b"glTF");
    assert_eq!(word(4), 2);
    assert_eq!(word(8), glb.len());

    let json_len = word(12);
    assert_eq!(&glb[16..20], b"JSON");
    assert_eq!(json_len % 4, 0);
    let json: Value = serde_json::from_slice(&glb[20..20 + json_len]).unwrap();

    let bin_at = 20 + json_len;
    let bin_len = word(bin_at);
    assert_eq!(&glb[bin_at + 4..bin_at + 8], b"BIN\0");
    assert_eq!(bin_len % 4, 0);
    assert_eq!(bin_at + 8 + bin_len, glb.len());
    assert_eq!(json["buffers"][0]["byteLength"], bin_len);
    assert!(json["buffers"][0].get("uri").is_none());
}

#[test]
fn output_is_deterministic() {
    let (scene, options) = (sample(), WriteOptions::default());
    assert_eq!(
        to_gltf(&scene, &options).unwrap(),
        to_gltf(&scene, &options).unwrap()
    );
    assert_eq!(
        to_glb(&scene, &options).unwrap(),
        to_glb(&scene, &options).unwrap()
    );
}

#[test]
fn markers_follow_the_options() {
    let scene = sample();
    let deep = WriteOptions {
        marker_depth_m: 2.0,
        ..WriteOptions::default()
    };
    let doc = read_gltf(&to_gltf(&scene, &deep).unwrap()).doc;
    let marker = node(&doc, "sh010 marker").mesh().unwrap();
    assert_eq!(
        marker.primitives().next().unwrap().bounding_box().min[2],
        -2.0
    );

    let off = WriteOptions {
        markers: false,
        ..WriteOptions::default()
    };
    for asset in both(&scene, &off) {
        assert!(
            asset
                .doc
                .nodes()
                .all(|n| extras(&n)["kind"] != "shot_marker")
        );
        assert!(
            asset
                .doc
                .meshes()
                .flat_map(|m| m.primitives())
                .all(|p| p.mode() == Mode::Triangles)
        );
    }

    let cameras_only = from_json(
        r#"{ "version": 1, "shots": [ { "id": "a", "camera": { "position": [0, 1, 0] } } ] }"#,
    )
    .unwrap();
    for asset in both(&cameras_only, &off) {
        assert_eq!(asset.doc.buffers().count(), 0);
        assert_eq!(asset.doc.cameras().count(), 1);
        assert_eq!(asset.doc.meshes().count(), 0);
    }
}

#[test]
fn ignores_unknown_fields() {
    let scene = from_json(
        r#"{ "version": 1, "fps": 24, "shots": [ { "id": "a", "duration_frames": 48,
             "camera": { "position": [0, 1, 0], "lens_notes": "zoom" } } ] }"#,
    )
    .unwrap();
    assert_eq!(scene.shots[0].id, "a");
}

fn camera(fields: &str) -> String {
    format!(
        r#"{{ "version": 1, "shots": [ {{ "id": "a", "camera": {{ "position": [0, 1, 2]{fields} }} }} ] }}"#
    )
}

fn prop(fields: &str) -> String {
    format!(r#"{{ "version": 1, "props": [ {{ "id": "p", "position": [0, 0, 0]{fields} }} ] }}"#)
}

#[test]
fn rejects_invalid_scenes() {
    let cases = [
        (r#"{ "version": 2, "props": [] }"#.to_owned(), "version"),
        (r#"{ "version": 1 }"#.to_owned(), "shots"),
        (
            camera(r#", "focal_length_mm": 0"#),
            "shots[0].camera.focal_length_mm",
        ),
        (
            camera(r#", "sensor_mm": [36, -1]"#),
            "shots[0].camera.sensor_mm[1]",
        ),
        (
            camera(r#", "look_at": [0, 1, 2]"#),
            "shots[0].camera.look_at",
        ),
        (
            camera(r#", "look_at": [0, 0, 0], "pan_deg": 10"#),
            "shots[0].camera.look_at",
        ),
        (camera(r#", "near_m": 0"#), "shots[0].camera.near_m"),
        (
            camera(r#", "near_m": 1, "far_m": 1"#),
            "shots[0].camera.far_m",
        ),
        (camera(r#", "focal_length_mm": 1e-300"#), "shots[0].camera"),
        (prop(r#", "size": [1, 0, 1]"#), "props[0].size[1]"),
        (
            prop(r##", "size": [1, 1, 1], "color": "#12345""##),
            "props[0].color",
        ),
        (
            r#"{ "version": 1, "shots": [ { "id": " ", "camera": { "position": [0, 0, 0] } } ] }"#
                .to_owned(),
            "shots[0].id",
        ),
        (
            r#"{ "version": 1, "shots": [ { "id": "a", "camera": { "position": [0, 0, 0] } } ],
                 "props": [ { "id": "a", "position": [0, 0, 0], "size": [1, 1, 1] } ] }"#
                .to_owned(),
            "props[0].id",
        ),
    ];
    for (json, expected_path) in cases {
        match from_json(&json) {
            Err(Error::Invalid { path, message }) => {
                assert_eq!(path, expected_path, "{message}");
                assert!(!message.is_empty());
            }
            other => panic!("{expected_path}: expected a validation error, got {other:?}"),
        }
    }
}

#[test]
fn reports_schema_shape_errors_from_serde() {
    for (json, needle) in [
        (prop(r#", "size": [1, 1, 1], "shape": "cone""#), "cone"),
        (prop(""), "size"),
        (
            r#"{ "version": 1, "shots": [ { "id": "a", "camera": {} } ] }"#.to_owned(),
            "position",
        ),
        ("not json".to_owned(), "expected"),
    ] {
        match from_json(&json) {
            Err(e @ Error::Json(_)) => assert!(e.to_string().contains(needle), "{e}"),
            other => panic!("expected a JSON error, got {other:?}"),
        }
    }
}

#[test]
fn rejects_unusable_options() {
    let scene = sample();
    let zero = WriteOptions {
        marker_depth_m: 0.0,
        ..WriteOptions::default()
    };
    let err = to_glb(&scene, &zero).unwrap_err();
    assert!(
        err.to_string().starts_with("options.marker_depth_m:"),
        "{err}"
    );

    let huge = WriteOptions {
        marker_depth_m: 1e39,
        ..WriteOptions::default()
    };
    let err = to_gltf(&scene, &huge).unwrap_err();
    assert!(err.to_string().starts_with("shots[0].camera:"), "{err}");
}
