// SPDX-License-Identifier: MIT OR Apache-2.0

//! glTF 2.0 document assembly and `.gltf` / `.glb` encoding.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::geometry::{self, Geometry};
use crate::math::{self, tidy};
use crate::model::{Shape, ShotScene};

const FLOAT: u32 = 5126;
const UNSIGNED_SHORT: u32 = 5123;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const MARKER_COLOR: [u8; 3] = [0xFF, 0xA0, 0x00];
const GENERATOR: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// Output settings that are not part of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteOptions {
    /// Add a wireframe frustum under every camera so shots show up in any viewer.
    pub markers: bool,
    /// Distance from the lens to the far end of the frustum marker, in metres.
    pub marker_depth_m: f64,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            markers: true,
            marker_depth_m: 0.5,
        }
    }
}

/// Writes a `.gltf` document with the geometry embedded as a base64 data URI.
pub fn to_gltf(scene: &ShotScene, options: &WriteOptions) -> Result<String> {
    let (mut root, bin) = build(scene, options)?;
    if let Some(buffer) = root.buffers.first_mut() {
        buffer.uri = Some(format!(
            "data:application/octet-stream;base64,{}",
            base64(&bin)
        ));
    }
    let mut out = serde_json::to_string_pretty(&root)?;
    out.push('\n');
    Ok(out)
}

/// Writes a binary `.glb` container (JSON chunk plus BIN chunk).
pub fn to_glb(scene: &ShotScene, options: &WriteOptions) -> Result<Vec<u8>> {
    let (root, mut bin) = build(scene, options)?;
    let mut json = serde_json::to_vec(&root)?;
    pad(&mut json, b' ');
    pad(&mut bin, 0);
    let bin_chunk = if bin.is_empty() { 0 } else { 8 + bin.len() };
    let too_big = || Error::invalid("output", "asset exceeds the 4 GiB GLB limit");
    let total = u32::try_from(12 + 8 + json.len() + bin_chunk).map_err(|_| too_big())?;

    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

fn build(scene: &ShotScene, options: &WriteOptions) -> Result<(Root, Vec<u8>)> {
    scene.validate()?;
    let depth = options.marker_depth_m;
    if !(depth.is_finite() && depth > 0.0) {
        return Err(Error::invalid(
            "options.marker_depth_m",
            format!("must be greater than 0, got {depth}"),
        ));
    }

    let mut b = Builder::default();
    let mut roots = Vec::new();

    if !scene.shots.is_empty() {
        let group = b.reserve_node();
        roots.push(group);
        let mut shots = Vec::with_capacity(scene.shots.len());
        for (i, shot) in scene.shots.iter().enumerate() {
            let cam = &shot.camera;
            let (focal, sensor) = (cam.focal_length(), cam.sensor());
            b.cameras.push(CameraJson {
                name: shot.id.clone(),
                kind: "perspective",
                perspective: Perspective {
                    aspect_ratio: cam.aspect_ratio(),
                    yfov: cam.yfov(),
                    zfar: cam.far_m,
                    znear: cam.near(),
                },
            });
            let camera = b.cameras.len() - 1;
            let node = b.reserve_node();
            shots.push(node);

            let mut children = Vec::new();
            if options.markers {
                let frustum = geometry::frustum(depth, focal, sensor);
                if !frustum.is_finite() {
                    return Err(Error::invalid(
                        format!("shots[{i}].camera"),
                        "lens and sensor give a frustum marker too large to store",
                    ));
                }
                let material = b.material("camera marker".to_owned(), MARKER_COLOR);
                let key = GeometryKey::Frustum([focal, sensor[0], sensor[1]].map(f64::to_bits));
                let name = format!("frustum {focal} mm {}x{} mm", sensor[0], sensor[1]);
                let mesh = b.mesh(key, material, name, move || frustum);
                children.push(b.push_node(Node {
                    name: Some(format!("{} marker", shot.id)),
                    mesh: Some(mesh),
                    extras: Some(json!({ "kind": "shot_marker", "id": shot.id })),
                    ..Node::default()
                }));
            }

            let mut extras = json!({
                "kind": "shot",
                "id": shot.id,
                "focal_length_mm": focal,
                "sensor_mm": sensor,
            });
            if let Some(name) = &shot.name {
                extras["name"] = json!(name);
            }
            b.nodes[node] = Node {
                name: Some(shot.id.clone()),
                children,
                camera: Some(camera),
                translation: translation(cam.position),
                rotation: rotation(cam.rotation()),
                extras: Some(extras),
                ..Node::default()
            };
        }
        b.nodes[group] = group_node("Shots", shots);
    }

    if !scene.props.is_empty() {
        let group = b.reserve_node();
        roots.push(group);
        let mut props = Vec::with_capacity(scene.props.len());
        for prop in &scene.props {
            let rgb = prop.rgb();
            let material = b.material(format!("prop {}", hex(rgb)), rgb);
            let shape = prop.shape;
            let name = format!("{} {}", shape.as_str(), hex(rgb));
            let mesh = b.mesh(GeometryKey::Shape(shape), material, name, || {
                geometry::shape(shape)
            });
            let mut extras = json!({ "kind": "prop", "id": prop.id, "shape": shape.as_str() });
            if let Some(name) = &prop.name {
                extras["name"] = json!(name);
            }
            props.push(b.push_node(Node {
                name: Some(prop.id.clone()),
                mesh: Some(mesh),
                translation: translation(prop.position),
                rotation: rotation(prop.rotation()),
                scale: (prop.size != [1.0; 3]).then_some(prop.size),
                extras: Some(extras),
                ..Node::default()
            }));
        }
        b.nodes[group] = group_node("Props", props);
    }

    let buffers = if b.bin.is_empty() {
        Vec::new()
    } else {
        vec![Buffer {
            byte_length: b.bin.len(),
            uri: None,
        }]
    };
    let root = Root {
        asset: Asset {
            version: "2.0",
            generator: GENERATOR,
        },
        scene: 0,
        scenes: vec![SceneJson {
            name: scene.scene.clone(),
            nodes: roots,
        }],
        nodes: b.nodes,
        cameras: b.cameras,
        meshes: b.meshes,
        materials: b.materials,
        accessors: b.accessors,
        buffer_views: b.views,
        buffers,
    };
    Ok((root, b.bin))
}

fn group_node(name: &str, children: Vec<usize>) -> Node {
    Node {
        name: Some(name.to_owned()),
        children,
        ..Node::default()
    }
}

fn translation(v: [f64; 3]) -> Option<[f64; 3]> {
    let v = v.map(tidy);
    (v != [0.0; 3]).then_some(v)
}

fn rotation(q: [f64; 4]) -> Option<[f64; 4]> {
    let q = q.map(tidy);
    (q != [0.0, 0.0, 0.0, 1.0]).then_some(q)
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn pad(bytes: &mut Vec<u8>, fill: u8) {
    bytes.resize(bytes.len().next_multiple_of(4), fill);
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GeometryKey {
    Shape(Shape),
    /// Focal length and sensor size, as bits.
    Frustum([u64; 3]),
}

#[derive(Clone, Copy)]
struct GeometryAccessors {
    position: usize,
    normal: Option<usize>,
    indices: usize,
    mode: u32,
}

/// Collects glTF objects; each cache keeps the first index handed out so the
/// output does not depend on hash order.
#[derive(Default)]
struct Builder {
    nodes: Vec<Node>,
    cameras: Vec<CameraJson>,
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
    accessors: Vec<Accessor>,
    views: Vec<BufferView>,
    bin: Vec<u8>,
    geometries: HashMap<GeometryKey, GeometryAccessors>,
    mesh_ids: HashMap<(GeometryKey, usize), usize>,
    /// Keyed by name, which encodes the role and colour.
    material_ids: HashMap<String, usize>,
}

impl Builder {
    fn reserve_node(&mut self) -> usize {
        self.push_node(Node::default())
    }

    fn push_node(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn material(&mut self, name: String, rgb: [u8; 3]) -> usize {
        if let Some(&id) = self.material_ids.get(&name) {
            return id;
        }
        let [r, g, b] = rgb.map(|c| tidy(math::srgb_to_linear(f64::from(c) / 255.0)));
        self.materials.push(Material {
            name: name.clone(),
            pbr_metallic_roughness: Pbr {
                base_color_factor: [r, g, b, 1.0],
                metallic_factor: 0.0,
                roughness_factor: 0.9,
            },
        });
        let id = self.materials.len() - 1;
        self.material_ids.insert(name, id);
        id
    }

    fn mesh(
        &mut self,
        key: GeometryKey,
        material: usize,
        name: String,
        make: impl FnOnce() -> Geometry,
    ) -> usize {
        if let Some(&id) = self.mesh_ids.get(&(key, material)) {
            return id;
        }
        let g = self.geometry(key, make);
        let mut attributes = BTreeMap::new();
        attributes.insert("POSITION", g.position);
        if let Some(normal) = g.normal {
            attributes.insert("NORMAL", normal);
        }
        self.meshes.push(Mesh {
            name,
            primitives: vec![Primitive {
                attributes,
                indices: g.indices,
                material,
                mode: g.mode,
            }],
        });
        let id = self.meshes.len() - 1;
        self.mesh_ids.insert((key, material), id);
        id
    }

    fn geometry(&mut self, key: GeometryKey, make: impl FnOnce() -> Geometry) -> GeometryAccessors {
        if let Some(&found) = self.geometries.get(&key) {
            return found;
        }
        let g = make();
        let position = self.vec3(&g.positions, true);
        let normal = g.normals.as_deref().map(|n| self.vec3(n, false));
        let bytes: Vec<u8> = g.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.view(&bytes, ELEMENT_ARRAY_BUFFER);
        let indices = self.accessor(Accessor {
            buffer_view: view,
            component_type: UNSIGNED_SHORT,
            count: g.indices.len(),
            kind: "SCALAR",
            min: None,
            max: None,
        });
        let found = GeometryAccessors {
            position,
            normal,
            indices,
            mode: g.mode,
        };
        self.geometries.insert(key, found);
        found
    }

    fn vec3(&mut self, data: &[[f32; 3]], bounds: bool) -> usize {
        let bytes: Vec<u8> = data
            .iter()
            .flatten()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let view = self.view(&bytes, ARRAY_BUFFER);
        let (min, max) = if bounds {
            let fold = |pick: fn(f32, f32) -> f32, start: f32| {
                data.iter()
                    .fold([start; 3], |acc, p| [0, 1, 2].map(|k| pick(acc[k], p[k])))
            };
            (
                Some(fold(f32::min, f32::INFINITY)),
                Some(fold(f32::max, f32::NEG_INFINITY)),
            )
        } else {
            (None, None)
        };
        self.accessor(Accessor {
            buffer_view: view,
            component_type: FLOAT,
            count: data.len(),
            kind: "VEC3",
            min,
            max,
        })
    }

    fn accessor(&mut self, accessor: Accessor) -> usize {
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    /// Appends one tightly packed buffer view, keeping every view 4-byte aligned.
    fn view(&mut self, bytes: &[u8], target: u32) -> usize {
        let byte_offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        pad(&mut self.bin, 0);
        self.views.push(BufferView {
            buffer: 0,
            byte_offset,
            byte_length: bytes.len(),
            target,
        });
        self.views.len() - 1
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Root {
    asset: Asset,
    scene: usize,
    scenes: Vec<SceneJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    nodes: Vec<Node>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cameras: Vec<CameraJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    meshes: Vec<Mesh>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    materials: Vec<Material>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    accessors: Vec<Accessor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    buffer_views: Vec<BufferView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    buffers: Vec<Buffer>,
}

#[derive(Serialize)]
struct Asset {
    version: &'static str,
    generator: &'static str,
}

#[derive(Serialize)]
struct SceneJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    nodes: Vec<usize>,
}

#[derive(Serialize, Default)]
struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mesh: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    translation: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rotation: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extras: Option<Value>,
}

#[derive(Serialize)]
struct CameraJson {
    name: String,
    #[serde(rename = "type")]
    kind: &'static str,
    perspective: Perspective,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Perspective {
    aspect_ratio: f64,
    yfov: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    zfar: Option<f64>,
    znear: f64,
}

#[derive(Serialize)]
struct Mesh {
    name: String,
    primitives: Vec<Primitive>,
}

#[derive(Serialize)]
struct Primitive {
    attributes: BTreeMap<&'static str, usize>,
    indices: usize,
    material: usize,
    mode: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    name: String,
    pbr_metallic_roughness: Pbr,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Pbr {
    base_color_factor: [f64; 4],
    metallic_factor: f64,
    roughness_factor: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Accessor {
    buffer_view: usize,
    component_type: u32,
    count: usize,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<[f32; 3]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BufferView {
    buffer: usize,
    byte_offset: usize,
    byte_length: usize,
    target: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Buffer {
    byte_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in cases {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
        assert_eq!(base64(&[0xFB, 0xFF, 0xBF]), "+/+/");
    }
}
