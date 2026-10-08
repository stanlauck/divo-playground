# Task board

Status: `open` → `taken` (handle, date) → `review` (PR #) → `done`.

| ID | Task | Stack | Status | Taken | PR |
|---|---|---|---|---|---|
| PG-01 | Prose importer: FB2 / DOCX / TXT → chapters + paragraphs JSON; streaming; benchmark on 200k words | Rust crate | review | stanlauck, 2026-10-07 | #21 |
| PG-02 | Screenplay PDF importer: text extraction, element classification (scene heading, action, character, dialogue, parenthetical, transition) + "doubtful lines" report | Rust crate | review | stanlauck, 2026-10-07 | #23 |
| PG-03 | articy:draft JSON export reader → neutral dialogue-graph JSON (nodes, choices, conditions, variables) | Rust crate | review | stanlauck, 2026-10-07 | #22 |
| PG-04 | Godot 4 importer add-on for neutral scene/dialogue JSON (locations, exits, items, dialogue nodes) | GDScript add-on | taken | stanlauck, 2026-10-08 | |
| PG-05 | Production breakdown export: neutral breakdown JSON → Movie Magic Scheduling–compatible file + CSV schedule | Rust crate | open | | |
| PG-06 | Editorial timeline writer: neutral shot list JSON → OpenTimelineIO → FCPXML; round-trip ids | Rust crate | open | | |
| PG-07 | ComfyUI client: submit workflow, poll progress, cancel, fetch outputs, retries; mock server for tests | Rust crate | review | stanlauck, 2026-10-07 | #20 |
| PG-08 | glTF scene mock-up writer: camera markers + placeholder props from neutral shot JSON | Rust crate | review | Droid, 2026-10-07 | #19 |
| PG-09 | Unicode test corpus + checks: RTL (Arabic/Hebrew), Devanagari, CJK, combining marks — line breaking, cursor movement, grapheme counting | TS package + corpus | review | stanlauck, 2026-10-08 | #24 |
| PG-10 | Minimap component for large node graphs (pan, zoom, viewport rect, 10k nodes at 60 fps) | TS / React package | open | | |
| PG-11 | Theme engine: presets, accent from cover image (auto palette), custom HSL, density; light/dark | TS package | open | | |
| PG-12 | Report renderer: neutral report JSON → Markdown and PDF (Typst) with optional sections | Rust crate | open | | |
| PG-13 | GEXF 1.3 writer for typed graphs (node/edge attributes, time slices) | Rust crate | review | stanlauck, 2026-10-07 | #18 |
| PG-14 | Localization exchange: string table JSON ↔ XLIFF 2.1 / CSV, with per-line context and character name | Rust crate | review | Mwapi, 2026-10-07 | #17 |
