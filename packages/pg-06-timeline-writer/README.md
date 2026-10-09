# PG-06: editorial timeline writer

Standalone Rust library and CLI. A neutral JSON v1 shot list becomes an
OpenTimelineIO JSON timeline or FCPXML 1.9. Readers return the same normalized
shot list, including shot IDs. No media probing, file access through media URLs,
or network calls. MIT OR Apache-2.0.

```rust
use timeline_writer::{from_json, from_otio, from_fcpxml, to_otio, to_fcpxml};

let input = r#"{
  "version": 1, "frame_rate": 24,
  "shots": [{"id":"synthetic-01","duration_frames":48}]
}"#;
let list = from_json(input)?;
let otio = to_otio(&list)?;
let xml = to_fcpxml(&from_otio(&otio)?)?;
assert_eq!(from_fcpxml(&xml)?, list);
# Ok::<(), timeline_writer::Error>(())
```

## JSON v1

See `schema.json` and the invented `samples/synthetic.json`.

| Field | Meaning |
|---|---|
| `version` | Required integer `1` |
| `title` | Optional project name |
| `frame_rate` | Required integer FPS, or `{"num":24000,"den":1001}` |
| `record_start` | Optional **non-drop** `HH:MM:SS:FF`, default `00:00:00:00` |
| `resolution` | Optional `[width,height]`, default `[1920,1080]` |
| `shots` | Required array, one video track in supplied cut order |
| `shots[].id` | Required unique, nonempty string, no surrounding whitespace |
| `shots[].name` | Optional display name, otherwise the ID |
| `shots[].duration_frames` | Required positive integer |
| `shots[].gap_before_frames` | Optional nonnegative integer, default zero |
| `shots[].media` | Optional `{url, source_in_frame?}`; absence is a placeholder |
| `media.source_in_frame` | Nonnegative integer relative to the media beginning |

Unknown neutral JSON fields (including camera/props extensions) are ignored,
not retained. Duplicate keys are rejected even inside ignored extensions.
Optional fields also accept `null`.
`FrameRate` reduces fractions and permits 1–1000 FPS. Timecode counts
`ceil(FPS)` frames per second, without dropped frame numbers. `01:00:00:00`
at 24000/1001 is 86,400 frames, or 3603.6 seconds, **not** 3600 seconds.
Hours are 00–99. Above 100 FPS the final timecode field has three digits.
Drop-frame input is unsupported.

Media URLs must be ASCII absolute `file://` or `https://` URIs with valid
percent escapes, no whitespace/backslashes, user credentials, or fragments.
Characters outside URI syntax (such as raw quotes or angle brackets) need
percent encoding, not just XML escaping.
File URLs need a non-root absolute path and no query; HTTPS may have a query.
URI strings are validated but carried unchanged. Media may be unavailable.
HTTPS is a reference carrier, **not a claim that Final Cut downloads or relinks
HTTPS assets**. The media source is assumed to match sequence FPS/resolution
and be video-only; the package does not inspect it.

Normalization removes empty title/name, names equal to IDs, zero gap/source-in,
zero record start, and default resolution. The exact roundtrip promise compares
normalized supported model fields, not JSON whitespace, unknown extensions,
arbitrary editor metadata, or an editor's later modifications.
`to_json` is compact and bounded, so accepted large lists remain readable.
`schema.json` describes structure; byte budgets, unique IDs, frame sums,
timecode/rate consistency, and complete URI checks are authoritative in Rust.

## Timeline mappings and supported import subset

**OTIO:** `Timeline.1 → Stack.1 → Track.1` (`Video`), containing `Clip.2` and
explicit `Gap.1`. IDs live in `clip.metadata.shot_list.id`. Timeline metadata
`shot_list` stores v1, exact rational FPS and resolution. Missing media uses
`MissingReference.1`; media uses `ExternalReference.1`.

The reader also supports legacy `Clip.1`. Without our metadata it infers integer
or 24000/1001, 30000/1001, 60000/1001, 120000/1001 FPS, from a timed track item
or `global_start_time.rate` when no timed item is available (including an empty
track). With neither source, rate inference is an error. Input RationalTime values
must be whole, nonnegative frame counts. Different supported rates rescale
exactly with integer ratios; fractional frames are errors, never rounded.
Explicit `source_range` is required, and an optional media `available_range`
must contain the clip. When that available range has a nonzero media origin,
the origin is subtracted to obtain neutral media-relative source-in.
Nonzero source-in for MissingReference is unsupported.

**FCPXML:** one project/sequence/spine, one format and one asset per distinct
URI in first-use order. Asset durations are the maximum supplied source-out
for that URI, **not probed physical media length**. Assets have video only,
`media-rep kind="original-media"`, and clips use `srcEnable="video"`.
All times are reduced rational seconds with integer arithmetic. Sequence
`tcStart` is its first local frame; spine offsets begin at that time.
IDs live in `<metadata><md key="shot_list.id" value="…"/></metadata>` on each
asset-clip. A placeholder becomes a gap carrying the same ID and name.
Ordinary preceding gaps have no shot ID.

The reader accepts a single project directly under `fcpxml`, under one event,
or under a library/event. Referenced assets must match the progressive, square
pixel sequence format, use one original media-rep, and contain no audio.
Explicit clip duration is required. A nonzero asset start is subtracted from
the clip's absolute source start. Non-overlapping implicit holes become
`gap_before_frames`. Unused resources still undergo supported-shape checks.

Both readers assign missing external clip IDs from a trimmed, unused clip name
of at most 256 UTF-8 bytes, otherwise `shot-<n>`, reserving supplied IDs first.
They reject duplicate supplied IDs, trailing ordinary gaps (not representable
by this model), multiple/audio tracks, nested storylines, transitions, effects,
markers, disabled clips, trimming compositions, retiming, connected clips,
split edits, image sequences, alternate media references, and unsupported
schema versions. FCPXML namespaces, processing instructions, external/internal
DTDs, and unknown semantic elements/attributes are rejected. Only the customary
bare `<!DOCTYPE fcpxml>` is allowed. Ordinary metadata values not mapped to
neutral fields are ignored. The doctype handler scans only the XML prolog,
skipping complete comments; declarations inside/after the root are not removed.
Neither reader opens referenced media.

## Limits and errors

- Neutral JSON input/output: 8 MiB. OTIO/FCPXML input/output: 64 MiB.
- JSON: depth 32, two million values; XML: depth 32, 300,000 nodes.
- At most 10,000 shots and 20,000 track/spine items.
- Any aggregate timeline end or source-out: at most $2^{30}$ frames.
  This keeps OTIO frame values exact and FCPXML time numerators within signed
  64-bit bounds at every supported rational FPS.
- UTF-8 bytes: IDs 256, names/title 4096, media URLs 8192, all canonical model
  text 1 MiB. Redundant default display names are individually checked but not
  charged twice. Readers enforce field and aggregate budgets before copying
  referenced text into owned shots.
- Resolution: 1–32,768 per side. Single-line text rejects controls, U+2028,
  U+2029, U+FFFE and U+FFFF.

`Error { code, path }`, Display, and Debug contain only structural locations,
never IDs, media URLs, XML/JSON values, filenames, or raw parser/I/O causes.
Errors do not expose chained private causes. `read_input(reader, limit)` is
bounded UTF-8 reading. CLI I/O errors identify `input` or `output` and static
kind codes (`already_exists`, `not_found`, `permission_denied`, `broken_pipe`,
or `io_error`), never the filename or raw cause.
Public timecode/duration helpers return `Result`,
including on invalid directly constructed rates or overflowing frame sums.
Library writers validate before emitting; callers should still treat the
generated URLs as untrusted references when opening files in another tool.

## CLI and offline checks

Rust 1.85 or later. Fetch dependencies once; the tests themselves are offline.

```text
cargo test --locked --offline
cargo fmt --check
cargo clippy --locked --offline --all-targets -- -D warnings
cargo run --locked --offline -- --from json --to otio --input samples/synthetic.json --output timeline.otio
cargo run --locked --offline -- --from otio --to fcpxml --input timeline.otio --output timeline.fcpxml
cargo run --locked --offline -- --from fcpxml --to json --input timeline.fcpxml --output roundtrip.json
```

Use `-` for stdin/stdout. Output files use `create_new`: no overwrite, including
when input/output paths are the same. Invalid input is rejected before output
creation. Disk/stdout writes are not transactional: a later I/O failure may
leave a partial newly created file or output stream. No `--force`.
`cargo run --locked --offline --example make_samples` regenerates only the
package's two synthetic golden exports.

Optional independent checks use OpenTimelineIO **0.18.1**, lxml **6.0.2** and
jsonschema **4.25.1**, installed separately. They make no network calls:

```text
python tests/check_outputs.py --dtd /path/to/FCPXMLv1_9.dtd --work /temporary/output
```

That script checks native OTIO duration, gaps, source ranges, names, references,
IDs, and a native serialization that the Rust CLI reads back; lxml validates
FCPXML against a separately supplied 1.9 DTD and independently checks rational
offsets, asset bounds and IDs. The DTD is **not redistributed**.
Validation used Apple's copyright-noted FCPXML 1.9 DTD preserved in
CommandPost/CommandPost commit `54750e571a427340775028a75e10d17ae0c58f34`,
`src/extensions/cp/apple/fcpxml/dtd/FCPXMLv1_9.dtd`.

References: [OTIO file format](https://opentimelineio.readthedocs.io/en/stable/tutorials/otio-file-format-specification.html),
[Apple FCPXML reference](https://developer.apple.com/documentation/professional-video-applications/fcpxml-reference),
[time/offset explanation](https://fcp.cafe/developers/fcpxml/).
No live Final Cut Pro import or post-editor ID preservation is claimed.
