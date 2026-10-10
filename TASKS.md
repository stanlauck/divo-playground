# Task board

Status: `open` → `taken` (handle, date) → `review` (PR #) → `done`.

| ID | Task | Stack | Status | Taken | PR |
|---|---|---|---|---|---|
| PG-01 | Prose importer: FB2 / DOCX / TXT → chapters + paragraphs JSON; streaming; benchmark on 200k words | Rust crate | done | stanlauck, 2026-10-07 | #21 |
| PG-02 | Screenplay PDF importer: text extraction, element classification (scene heading, action, character, dialogue, parenthetical, transition) + "doubtful lines" report | Rust crate | done | stanlauck, 2026-10-07 | #23 |
| PG-03 | articy:draft JSON export reader → neutral dialogue-graph JSON (nodes, choices, conditions, variables) | Rust crate | done | stanlauck, 2026-10-07 | #22 |
| PG-04 | Godot 4 importer add-on for neutral scene/dialogue JSON (locations, exits, items, dialogue nodes) | GDScript add-on | done | stanlauck, 2026-10-08 | #25, #26 |
| PG-05 | Production breakdown export: neutral breakdown JSON → Movie Magic Scheduling–compatible file + CSV schedule | Rust crate | done | stanlauck, 2026-10-09 | #29 |
| PG-06 | Editorial timeline writer: neutral shot list JSON → OpenTimelineIO → FCPXML; round-trip ids | Rust crate | done | stanlauck, 2026-10-09 | #39 |
| PG-06a | Follow-up PG-06: empty OTIO timeline without shot_list must take frame rate from `global_start_time` (bot 🟡 on #39) | Rust crate | open | | (issue #40) |
| PG-07 | ComfyUI client: submit workflow, poll progress, cancel, fetch outputs, retries; mock server for tests | Rust crate | done | stanlauck, 2026-10-07 | #20 |
| PG-08 | glTF scene mock-up writer: camera markers + placeholder props from neutral shot JSON | Rust crate | done | Droid, 2026-10-07 | #19 |
| PG-09 | Unicode test corpus + checks: RTL (Arabic/Hebrew), Devanagari, CJK, combining marks — line breaking, cursor movement, grapheme counting | TS package + corpus | done | stanlauck, 2026-10-08 | #24 |
| PG-10 | Minimap component for large node graphs (pan, zoom, viewport rect, 10k nodes at 60 fps) | TS / React package | done | stanlauck, 2026-10-08 | #27 |
| PG-11 | Theme engine: presets, accent from cover image (auto palette), custom HSL, density; light/dark | TS package | done | stanlauck, 2026-10-09 | #28 |
| PG-12 | Report renderer: neutral report JSON → Markdown and PDF (Typst) with optional sections | Rust crate | review |  | #48 |
| PG-13 | GEXF 1.3 writer for typed graphs (node/edge attributes, time slices) | Rust crate | done | stanlauck, 2026-10-07 | #18 |
| PG-14 | Localization exchange: string table JSON ↔ XLIFF 2.1 / CSV, with per-line context and character name | Rust crate | done | Mwapi, 2026-10-07 | #17 |
| PG-15 | Unreal Engine 5 importer for neutral scene/dialogue JSON (same input as PG-04): places, exits, items, actor placeholders; re-import by id without duplicates; import report | UE5 plugin (Python or C++) | review |  | #49 |
| PG-16 | EDL CMX 3600 writer + reader for a neutral shot list JSON: reels, timecodes at 23.976/24/25/30, comments carry shot ids; round-trip test | Rust crate | review | stanlauck, 2026-10-09 | #42 |
| PG-17 | Subtitle writer: dialogue timing JSON → SRT / WebVTT / TTML with speaker labels; reading-speed (CPS), line-length and duration checks | Rust crate | review |  | #46 |
| PG-18 | Screenplay length estimator: Fountain → per-scene length in eighths of a page (Courier 12, US Letter/A4); deterministic | Rust crate | review |  | #47 |
| PG-19 | Fountain conformance corpus: reference .fountain files + expected element JSON for spec edge cases + CLI runner for any parser | Corpus + TS runner | review |  | #50 |
| PG-20 | Ink compiled JSON reader → neutral dialogue-graph JSON (same target as PG-03); unsupported features report | Rust crate | review |  | #51 |
| PG-21 | Yarn Spinner 2 .yarn parser → neutral dialogue-graph JSON (same target as PG-03); line ids preserved; error report | Rust crate | review |  | #43 |
| PG-22 | Storyboard sheet renderer: shot list JSON + frames → printable PDF grids via Typst; placeholder for missing images | Rust crate | review |  | #52 |
| PG-23 | Twine / Twee 3 story reader → PG-03 DialogueGraph v1 (passages, links, macros as opaque conditions) + round-trip test on a public-domain sample | Rust crate | open | | |
| PG-24 | DialogueGraph v1 → Yarn Spinner 2 writer; round-trip with PG-21 parser (parse → write → parse = same graph) | Rust crate | open | | |
| PG-25 | Unity importer for neutral scene JSON (same input as PG-04 Godot / PG-15 UE5): editor-only package, locations as GameObjects, exits as markers, import report imported/warned/rejected | C# Unity package | open | | |
| PG-26 | Screenplay statistics CLI: Fountain → per-character lines/words/scenes, scene lengths, dialogue/action ratio; JSON + Markdown | Rust crate | open | | |
| PG-27 | Draft-to-draft change report: two Fountain files → word-level aligned diff JSON (added/removed/moved scenes and lines, stable ids) | Rust crate | open | | |
| PG-28 | WCAG contrast checker for theme presets (pairs with PG-11): every text/surface token pair → ratio, AA/AAA verdict, failing pairs list | TS package | open | | |
| PG-29 | SVG icon set linter: 24px grid, stroke width, round caps/joins, `currentColor` only, no raster, viewBox checks; report per icon | TS package | open | | |
| PG-30 | Follow-up quality pass: collect all open 🟡 bot findings on merged PG PRs into one checklist issue and fix them in one PR per crate | any | open | | |
