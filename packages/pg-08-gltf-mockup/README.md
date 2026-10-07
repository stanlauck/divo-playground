# gltf-mockup (PG-08)

Turns neutral shot JSON into a glTF 2.0 scene mock-up. Each shot becomes a camera with a wireframe frustum marker. Placeholder props (boxes, cylinders, spheres) are sized and placed from the same file. The output is self-contained (`.gltf` with an embedded buffer, or binary `.glb`) and opens in any glTF viewer or 3D package.

## Usage

```sh
cargo run -- samples/kitchen-blocking.json kitchen.glb
cargo run -- samples/kitchen-blocking.json kitchen.gltf --marker-depth 1.0
cargo run -- samples/kitchen-blocking.json cameras.glb --no-markers
```

The output format follows the file extension. `--no-markers` leaves out the frustum meshes. `--marker-depth` sets their length in metres (default 0.5).

As a library:

```rust
let scene = gltf_mockup::from_json(&std::fs::read_to_string("shots.json")?)?;
let glb = gltf_mockup::to_glb(&scene, &gltf_mockup::WriteOptions::default())?;
```

## Input: neutral shot JSON, version 1

Full example: [`samples/kitchen-blocking.json`](samples/kitchen-blocking.json) (synthetic).

```json
{
  "version": 1,
  "scene": "Kitchen blocking",
  "shots": [
    { "id": "sh010", "name": "Wide", "camera": { "position": [3.2, 1.6, 3.6], "look_at": [0, 0.9, 0], "focal_length_mm": 24 } }
  ],
  "props": [
    { "id": "table", "position": [0, 0, 0], "size": [1.6, 0.75, 0.9], "yaw_deg": 10, "color": "#8B5A2B" }
  ]
}
```

Axes and units are the same as glTF: metres, right-handed, +Y up, +X right. A camera with no orientation fields looks down -Z.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `version` | integer | required | Must be `1`. |
| `scene` | string | none | glTF scene name. |
| `shots[].id` | string | required | Non-empty and unique across shots and props. Used as the node and camera name. |
| `shots[].name` | string | none | Label, kept in `extras`. |
| `camera.position` | `[x, y, z]` | required | Lens position. |
| `camera.look_at` | `[x, y, z]` | none | Point to aim at. Cannot be combined with `pan_deg` / `tilt_deg`. |
| `camera.pan_deg` | number | 0 | Turn about world +Y. Positive turns left. |
| `camera.tilt_deg` | number | 0 | Turn about the camera's X axis. Positive looks up. |
| `camera.roll_deg` | number | 0 | Turn about the view axis. Positive turns the top of the frame left. Also applies with `look_at`. |
| `camera.focal_length_mm` | number | 35 | Focal length. |
| `camera.sensor_mm` | `[w, h]` | `[36, 24]` | Gate size. For cropped formats, give the extraction area. |
| `camera.near_m` | number | 0.1 | Near clip plane. |
| `camera.far_m` | number | none | Far clip plane. When absent, the projection is infinite. |
| `props[].id` | string | required | Same rules as `shots[].id`. |
| `props[].name` | string | none | Label, kept in `extras`. |
| `props[].shape` | `box` \| `cylinder` \| `sphere` | `box` | Placeholder shape, stretched to fill `size`. |
| `props[].position` | `[x, y, z]` | required | Bottom centre of the bounding box, so props on the floor have y = 0. |
| `props[].size` | `[x, y, z]` | required | Bounding box extent before yaw. Every value must be > 0. |
| `props[].yaw_deg` | number | 0 | Turn about +Y, counter-clockwise seen from above. |
| `props[].color` | `"#RRGGBB"` | `#9E9E9E` | sRGB colour. |

Camera orientation is `Ry(pan) · Rx(tilt) · Rz(roll)`. With `look_at`, pan and tilt come from the direction to the target, so the horizon stays level unless `roll_deg` is set. When the camera looks straight up or down, pan is 0 and the top of the frame faces -Z (looking down) or +Z (looking up).

Unknown fields are ignored, so one shot file can also carry editorial data such as durations. Errors name the field at fault, for example `shots[2].camera.focal_length_mm: must be greater than 0, got 0`.

## Output

| Input | glTF |
|---|---|
| file | One scene, named after `scene`, with root nodes `Shots` and `Props`. |
| shot | Node named after `id`, with `translation`, `rotation` and a perspective camera: `yfov = 2·atan(h / 2f)`, `aspectRatio = w / h`, `znear`, optional `zfar`. |
| shot marker | Child node `<id> marker` with a `LINES` mesh: a frustum from the lens to the image rectangle at the marker depth, plus a triangle above the frame that marks "up". |
| prop | Node named after `id`. `translation` is the position, `rotation` is the yaw and `scale` is the size. The mesh is a unit shape (x and z in [-0.5, 0.5], y in [0, 1]). |

Node `extras` keep the data that has no place in glTF, so tools can map nodes back to the input:

- shot: `{ "kind": "shot", "id", "name"?, "focal_length_mm", "sensor_mm" }`
- marker: `{ "kind": "shot_marker", "id" }`
- prop: `{ "kind": "prop", "id", "name"?, "shape" }`

Data is shared where possible:

- one accessor set per shape, and per distinct lens for frustums;
- one mesh per shape and colour;
- one material per colour.

Materials are non-metallic PBR. Colours are converted from sRGB to the linear `baseColorFactor`. The same input always produces the same bytes.

## Tests

```sh
cargo test
```

All tests run offline. The integration tests read the output back with the independent [`gltf`](https://crates.io/crates/gltf) crate, which validates the document, and decode the buffers. They check:

- camera aim and field of view;
- prop placement and colour;
- triangle winding;
- GLB chunk layout;
- id round-trip;
- error reporting.

The sample output was also checked by hand with the Khronos glTF-Validator. It reports no errors, warnings, infos or hints. This check is not part of `cargo test`.

## Limitations

- Pinhole cameras only: no lens distortion, lens shift or focus distance.
- Props rotate about Y only. The shapes are box, cylinder and sphere.
- No animation: one camera pose per shot.

## License

MIT OR Apache-2.0, see the repository root.
