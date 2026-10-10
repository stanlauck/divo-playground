<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Fountain rules covered

These notes summarize the public [Fountain syntax reference](https://fountain.io/syntax) in original wording. They are a test map, not a copy of the specification.

- **01 title page:** A title page is the opening block of `Key: value` lines. A key may contain spaces; an indented continuation belongs to the preceding key. `Title:` with no value is valid.
- **02 scene headings:** `INT.`, `EXT.`, `EST.`, `INT./EXT.`, and `I/E.` at the start identify headings. A leading dot forces a heading; a line beginning with multiple dots is ordinary text. A trailing `#...#` is a scene number. Heading matching is case insensitive, so lowercase `int.` is still a scene heading.
- **03 action:** Unclassified text is action. `!` forces action and is removed from the element text. Action preserves leading spaces and line breaks, including a two-space blank line used to keep paragraphs connected.
- **04 character cues:** An uppercase line before dialogue is a character cue. A parenthesized extension can follow the name. `@` forces a cue and permits mixed case. An uppercase line followed by a blank line remains action; digits may occur in names.
- **05 dialogue:** A parenthetical line follows a character cue. Dialogue paragraphs can remain one element when a blank line contains two spaces.
- **06 dual dialogue:** A caret on a character cue marks the second side of a dual-dialogue block. The neutral representation adds explicit begin/end delimiters and `dual: true` to that second cue.
- **07 transitions:** Uppercase lines ending in `TO:` are transitions. `>` forces a transition and is removed from its text.
- **08 centered:** A line enclosed by `>` and `<` is centered text; both markers are removed.
- **09 lyrics:** Each line beginning with `~` is a lyric line; the marker is removed.
- **10 page breaks:** Three or more equals signs alone form a page break. The neutral element has empty text.
- **11 sections and synopses:** One or more leading `#` characters create a section; their count is its depth. A line beginning with `=` is a synopsis.
- **12 notes:** `[[...]]` surrounds an author note and may span lines. The corpus reports its inner text as a note element.
- **13 boneyard:** `/* ... */` comments out text and may span lines. The corpus reports its inner text as a boneyard element.
- **14 emphasis:** Single, double, triple asterisks and underscores mark emphasis styles. The neutral contract preserves those markers in `text`; a backslash escapes a marker.
- **15 line endings:** Fountain is line-oriented. The runner removes a UTF-8 BOM and normalizes CRLF/CR to LF before comparison.
- **16 Unicode:** Element classification is independent of script, so Cyrillic, CJK, and combining marks remain unchanged.
- **17 edge files:** Empty input, blank-only input, and trailing spaces are retained as regression cases.
- **18 kitchen sink:** Combines title metadata, sections, synopsis, heading numbers, action, note, forced action, dialogue, dual dialogue, centered text, lyric, page break, boneyard, and transition.
