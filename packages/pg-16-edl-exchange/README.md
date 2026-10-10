# PG-16: CMX 3600 EDL exchange

Standalone Rust library and CLI for a neutral JSON v1 shot list and one video
track of cuts/gaps. Reels, source/record timecodes and shot IDs round-trip.
No media probing, URL dereferencing or network calls. MIT OR Apache-2.0.

```rust
use edl_exchange::{from_json, from_edl, to_edl};

let list = from_json(r#"{
  "version":1, "frame_rate":24,
  "shots":[{"id":"synthetic-01","duration_frames":48}]
}"#)?;
assert_eq!(from_edl(&to_edl(&list)?, None)?, list);
# Ok::<(), edl_exchange::Error>(())
```

## JSON v1 and normalization

See `schema.json` and the invented `samples/synthetic.json` and
`samples/drop-frame.json`. This adapts the PG-06 neutral model locally; there
is **no runtime or build dependency on another playground package**.

| Field | Meaning |
|---|---|
| `version` | Required integer `1` |
| `title` | Optional project name |
| `frame_rate` | Integer 24, 25 or 30, or exact `{"num":24000,"den":1001}` / `{"num":30000,"den":1001}` |
| `timecode_mode` | Optional `"non_drop"` (default) or `"drop"` |
| `record_start` | Optional `HH:MM:SS:FF`; drop also accepts `HH:MM:SS;FF`; default zero |
| `resolution` | Optional `[width,height]`, default `[1920,1080]`; carried in comments |
| `shots` | Required ordered array; one video track |
| `shots[].id` | Required unique nonempty string, no surrounding whitespace |
| `shots[].name` | Optional display name; otherwise the ID |
| `shots[].duration_frames` | Positive integer frame count |
| `shots[].gap_before_frames` | Optional nonnegative integer; default zero |
| `shots[].media` | Optional `{reel?,url?,source_in_frame?}`; absence is a placeholder |
| `media.reel` | Optional original reel name; omission means auxiliary reel `AX` |
| `media.url` | Optional absolute `file://` or `https://` reference; never opened |
| `media.source_in_frame` | Optional nonnegative media-relative source-in; default zero |

Rates are reduced before validation, so equivalent integer/rational fractions
are accepted. Floats such as `23.976` and `29.97` are not accepted as neutral
JSON rates. Source media is assumed to use the supplied sequence rate; the
package does not probe or resample it.

`media:{}` is unresolved auxiliary media, not a placeholder.
Normalization removes empty title/name, name equal to ID, zero gap/source-in,
zero record start, default resolution, default non-drop mode and explicit
default reel `AX`. Drop record-start labels normalize to semicolons.
Unknown neutral fields, including extensions, are ignored. Duplicate JSON keys
are rejected even in ignored extensions. Optional fields accept `null` except
`timecode_mode`, whose omission, not `null`, selects the default.

Exact roundtrip means equality of normalized supported model fields for
`from_edl(to_edl(list), None)`. It does not mean byte-identical JSON, preservation
of arbitrary metadata, or persistence after an editor modifies or strips
comments. The schema describes structure; byte limits, supported reduced
rates, URI syntax, unique IDs, event count, timecode consistency and frame sums
are checked authoritatively in Rust. Schema string lengths count characters,
whereas Rust limits count UTF-8 bytes.

URLs must be ASCII absolute references with valid percent escapes, no raw
whitespace/backslashes, credentials or fragments. File references need a
non-root absolute path and no query/port; HTTPS may have a query. URI strings
remain unchanged. A drive-letter file authority such as `file://C:/clip.mov`
is rejected rather than repaired; use `file:///C:/clip.mov`. No relink/download
behavior is promised. All bundled references are invented and unavailable.

## Timecodes

All arithmetic uses integer frame counts. Non-drop uses nominal 24, 25 or
30 labels/second, including at 24000/1001 and 30000/1001. Consequently one
hour of 23.976 non-drop labels is 86,400 frames, not 3600 seconds of media.

Drop is supported **only at 30000/1001**. It skips labels `00` and `01` at
the start of each minute except every tenth minute. It does not drop pictures.
Examples:

| Zero-based frame | Drop label |
|---|---|
| 1799 | `00:00:59;29` |
| 1800 | `00:01:00;02` |
| 17982 | `00:10:00;00` |
| 107892 | `01:00:00;00` |

CMX rows use colon labels for both modes; `FCM: DROP FRAME` or
`FCM: NON-DROP FRAME` defines their interpretation. Import also accepts the
original manual's `FCM: NON DROP FRAME` spelling. External FCM omission means
non-drop. Midstream FCM switches are unsupported.

Hours are 00–23. Source/record out-points are **exclusive**. Midnight rollover,
cross-midnight events and an exclusive out-point at `24:00:00:00` are rejected,
not wrapped. No mixed source/record timecode modes or mixed media rates.

## CMX mapping, aliases and comments

Each event has exactly eight whitespace-separated fields:

```text
001  CAM1     V     C        00:00:01:00 00:00:02:00 01:00:00:00 01:00:01:00
```

The four timecodes are source in/out, then record in/out. Export numbers events
sequentially `001`–`999`. Gaps become separate `BL` video cuts. A `BL` event
carrying a shot ID and shot metadata is a placeholder, not an ordinary gap.

Reel tokens are 1–8 uppercase ASCII letters/digits. Valid original short names
remain unchanged except reserved generator names `BL`, `BLACK` and `BARS`.
Other reels, including long, lowercase and reserved names, get deterministic
`R0000001`-style aliases in first-use order. **All** original short names,
including later ones, are reserved before alias assignment. Repeated originals
reuse one alias; original reel strings live in shot comments. This is a local
reversible alias scheme, not a claim that external editors recover long names.

Export is ASCII with LF line endings. `TITLE:` is an uppercase letter/digit/
space projection limited to 70 characters. The exact title and resolution are
extension data. Comments use the following bounded v1 grammar:

| Comment | Payload / placement |
|---|---|
| `* SHOT ID: ` | Percent-encoded UTF-8 ID, immediately after the shot event |
| `* PG16 SHOT: ` | Percent-encoded JSON `{version,id,name,media}` on that shot |
| `* PG16 GAP: 1` | Ordinary explicit gap event |
| `* PG16 TIMELINE: ` | Percent-encoded JSON `{version,frame_rate,timecode_mode,title,record_start,resolution,event_count}` at EOF |

Encoding leaves only `A–Z`, `0–9`, `-`, `.`, `/`, `:` literal; every other
UTF-8 byte becomes uppercase `%HH`. Decode rejects malformed escapes and
invalid UTF-8; metadata JSON has bounded depth and unique keys. A timeline
manifest is at EOF because some native readers reject comments before the
first event. For an empty list it follows the two headers.

Own-import checks versions, title projection, FCM, event count/order, ID
agreement, source-in, placeholder/gap shape and deterministic reel aliases.
It requires complete own metadata rather than silently falling back to
external import when one comment disappears. This is consistency checking,
**not cryptographic tamper detection**. Exact FPS is not inferable from
ordinary CMX labels; a supplied `from_edl` rate checks metadata instead of
overriding it.

## External import subset

Without the timeline manifest, an exact `FrameRate` argument is mandatory:
`from_edl(text, Some(FrameRate::new(24000,1001)))`.
External cuts must have increasing unique event numbers, `V` channel, `C`
transition, valid short reel tokens, nonnegative timecodes and equal positive
source/record durations. Record overlaps and backwards ranges are errors.
Non-overlapping record holes and un-ID'd `BL`/`BLACK` events become preceding
gaps. A black event with our ID comment becomes a placeholder. Trailing
ordinary gaps, including an all-black no-ID list, cannot be represented and
are rejected.

The reader recognizes `* FROM CLIP NAME:`, `* FROM CLIP:` and `* FROM FILE:`.
The last two must contain supported absolute reference URIs, not bare paths.
Unmapped descriptive comments are ignored. Supplied ID comments are decoded
and preserved. Missing IDs use a trimmed, unused clip name of at most 256
bytes, otherwise `shot-<n>`, reserving all supplied IDs first.

Audio/mixed channels, dissolves/wipes/keys, repeated-event transition pairs,
bars generators, retiming, freeze-frame, speed/motion, split edits, recognized
color-effect declarations (`ASC_SOP`, `ASC_SAT`, CDL/LUT) and OTIO semantic
reference extensions are rejected. Unknown non-comment statements are
rejected. The reader does not implement the full CMX effect/extension ecosystem.
UTF-8 BOM, CRLF, blank lines and ASCII tabs as field separators are accepted.
This is a strict small interchange profile, not general EDL/editor coverage.

## Limits and errors

- JSON and EDL input/output: 8 MiB; JSON depth 32 and two million values.
- At most 999 total events, including explicit preceding gaps.
- EDL: 20,000 lines, 128 KiB per line, including metadata comments.
- Hours 00–23, no midnight wrap; every exclusive source/record end below one day.
- UTF-8 bytes: IDs 256, title/name/reel 4096, URI 8192, canonical text 1 MiB.
  Redundant default names are individually checked but not charged twice.
- Resolution 1–32,768 per side. Text rejects controls, U+2028, U+2029,
  U+FFFE and U+FFFF.

`Error {code,path}`, Display and Debug contain only structural locations,
not input values, URLs, filenames or private parser/I/O causes. `read_input`
reads bounded UTF-8. CLI I/O errors identify `input`/`output` and safe kind
codes such as `not_found` or `already_exists`.

## CLI and offline checks

```text
cargo run --locked -- --from json --to edl --input samples/synthetic.json --output invented.edl
cargo run --locked -- --from edl --to json --input invented.edl --output invented.json
cargo run --locked -- --from edl --to json --input external.edl --output external.json --fps 24000/1001
```

`-` selects stdin/stdout. Output paths must not exist. Input validation and
serialization finish before file creation; writes are not transactional if
the filesystem subsequently fails. CLI `--fps` is only for EDL input.

```text
cargo test --locked
cargo +1.85.0 test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo +1.85.0 clippy --locked --all-targets -- -D warnings
cargo fmt --check
cargo run --locked --example make_samples
```

The offline Rust tests include every frame of a complete drop-frame day.
Optional independent checks require already-installed Python packages:
`opentimelineio==0.18.1`, `otio-cmx3600-adapter==1.0.0`,
`timecode==1.5.1`, `jsonschema==4.25.1`.
Nothing installs packages or uses the network during tests.

```text
cargo build --locked
cargo run --locked --example timecode_vectors > timecode-vectors.tsv
python tests/check_outputs.py --binary target/debug/edl-exchange --vectors timecode-vectors.tsv
```

On Windows append `.exe` to the binary path. Use ASCII/UTF-8 output redirection
in PowerShell 5.1 instead of its default UTF-16 (`Out-File -Encoding ascii`).
The checker validates native NDF reads, official-adapter-authored external
imports, schema/runtime negatives, and 72,456 independent timecode vectors.

**Compatibility limitation:** the pinned official CMX adapter ignores FCM and
interprets raw colon drop labels as non-drop. The checker explicitly records
that raw DF mismatch, then normalizes only separators to semicolons to check
the independent native OTIO DF decoder. The `timecode` oracle also validates
the unmodified DF output using its declared mode. This is not a claim that the
official adapter imports raw DF correctly. Its pre-event comment limitation
also affects metadata-bearing empty exports. No live editor import, relink,
or post-editor ID preservation has been certified.

## Provenance and license

- CMX 3600 manual, revision `907567`, protocol/header/event discussion
  (pages 11–13) and legal character set (page 25):
  <https://xmil.biz/EDL-X/CMX3600.pdf>. Consulted locally, not redistributed.
- Official CMX adapter v1.0.0 (Apache-2.0), independently installed, not copied:
  <https://github.com/OpenTimelineIO/otio-cmx3600-adapter>.
- Independent `timecode` v1.5.1:
  <https://pypi.org/project/timecode/1.5.1/>.
- Local bounded JSON, normalization, URI validation and safe CLI conventions
  adapted from PG-06 at repository merge `c23112b2`; modified for this profile.
  Original MIT OR Apache-2.0 notices retained. No external manual/adapter source
  or real production material is included.

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
