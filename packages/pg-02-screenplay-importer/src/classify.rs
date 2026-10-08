// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::*;
use std::collections::BTreeMap;

pub(crate) fn classify(screenplay: &mut Screenplay, options: &ImportOptions) -> Result<()> {
    let headers = marginal_lines(screenplay);
    let mut scene = None;
    let mut speaker = None;
    let mut cue_x = 0.0;
    let mut parenthetical_open = false;
    for (index, is_header) in headers.into_iter().enumerate() {
        let line = &screenplay.lines[index];
        let text = line.text.trim();
        let width = screenplay.pages[line.page - 1].width;
        let left = line.bounds.left / width;
        let previous = index.checked_sub(1).map(|index| &screenplay.lines[index]);
        let next = screenplay.lines.get(index + 1);
        let page_changed = previous.is_some_and(|previous| previous.page != line.page);
        let separated = previous.is_some_and(|previous| {
            previous.page == line.page
                && line.baseline[1] - previous.baseline[1]
                    > line.font_size.max(previous.font_size) * 2.1
        });
        if page_changed || separated {
            speaker = None;
            parenthetical_open = false;
        }
        let mut reasons = line.issues.clone();
        let kind;
        let mut confidence;
        let scene_heading = scene_marker(text);
        let following_dialogue = next.is_some_and(|next| {
            next.page == line.page
                && next.row > line.row
                && next.baseline[1] - line.baseline[1] <= line.font_size.max(next.font_size) * 3.1
                && next.bounds.left / width >= 0.18
                && next.bounds.left / width <= 0.58
                && !next.text.trim().is_empty()
                && !scene_marker(next.text.trim())
                && !transition(next.text.trim())
                && (!cue(next.text.trim())
                    || next.bounds.left + line.font_size * 3.0 < line.bounds.left)
                && !next.issues.contains(&Reason::MultipleColumns)
        });
        let upper_cue = cue(text);
        let centered = (0.30..=0.68).contains(&left);
        let title_case_cue = !upper_cue
            && centered
            && following_dialogue
            && text.chars().count() <= 40
            && text.split_whitespace().count() <= 4
            && text.chars().next().is_some_and(char::is_uppercase)
            && !text.ends_with(['.', ':', '!', '?']);
        if reasons.iter().any(|reason| {
            matches!(
                reason,
                Reason::MultipleColumns
                    | Reason::UnsupportedTextOrientation
                    | Reason::UnicodeMappingMissing
                    | Reason::BackendUncertainText
                    | Reason::ControlCharacters
            )
        }) {
            kind = ElementKind::Unknown;
            confidence = Confidence::Low;
            speaker = None;
            parenthetical_open = false;
        } else if is_header {
            kind = ElementKind::Unknown;
            confidence = Confidence::Low;
            reasons.push(Reason::PossibleHeaderFooter);
            speaker = None;
            parenthetical_open = false;
            // Page furniture does not become dialogue or a speaker cue.
        } else if text.is_empty() {
            kind = ElementKind::Unknown;
            confidence = Confidence::Low;
            reasons.push(Reason::UnrecognizedText);
            speaker = None;
            parenthetical_open = false;
        } else if scene_heading {
            kind = ElementKind::SceneHeading;
            confidence = Confidence::High;
            speaker = None;
            parenthetical_open = false;
            if text.split_whitespace().count() <= 1 {
                reasons.push(Reason::IncompleteSceneHeading);
                confidence = Confidence::Medium;
            }
        } else if transition(text) {
            kind = ElementKind::Transition;
            confidence = Confidence::High;
            speaker = None;
            parenthetical_open = false;
        } else if parenthetical_open && speaker.is_some() {
            kind = ElementKind::Parenthetical;
            confidence = Confidence::Medium;
            parenthetical_open = !text.contains(')');
        } else if text.starts_with('(') {
            kind = ElementKind::Parenthetical;
            confidence = if speaker.is_some() {
                Confidence::High
            } else {
                Confidence::Low
            };
            if speaker.is_none() {
                reasons.push(Reason::UnexpectedParenthetical);
            }
            parenthetical_open = speaker.is_some() && !text.contains(')');
        } else if (upper_cue && centered) || title_case_cue {
            kind = ElementKind::Character;
            confidence = Confidence::High;
            if !upper_cue || !centered || !following_dialogue || scene.is_none() {
                confidence = Confidence::Medium;
                reasons.push(Reason::UncertainCharacterCue);
            }
            cue_x = line.bounds.left;
            parenthetical_open = false;
        } else if speaker.is_some()
            && (0.17..=0.58).contains(&left)
            && line.bounds.left <= cue_x + line.font_size * 2.0
        {
            kind = ElementKind::Dialogue;
            confidence = Confidence::High;
        } else if uppercase(text) {
            kind = ElementKind::Unknown;
            confidence = Confidence::Low;
            reasons.push(Reason::AmbiguousUppercase);
            speaker = None;
            parenthetical_open = false;
        } else if (0.21..=0.48).contains(&left) && !text.is_empty() {
            kind = ElementKind::Dialogue;
            confidence = Confidence::Low;
            reasons.push(Reason::MissingSpeaker);
            if page_changed {
                reasons.push(Reason::DialogueAcrossPage);
            }
            speaker = None;
        } else if !text.is_empty() {
            kind = ElementKind::Action;
            confidence = Confidence::Medium;
            speaker = None;
            parenthetical_open = false;
        } else {
            kind = ElementKind::Unknown;
            confidence = Confidence::Low;
            reasons.push(Reason::UnrecognizedText);
        }
        // Marginal text and uncertain layout never leak a guessed speaker association.
        let line_speaker = if matches!(kind, ElementKind::Dialogue | ElementKind::Parenthetical) {
            speaker.clone()
        } else {
            None
        };
        let line_scene = if kind == ElementKind::SceneHeading
            || reasons.contains(&Reason::PossibleHeaderFooter)
        {
            None
        } else {
            scene.clone()
        };
        let can_merge = previous.is_some_and(|previous| {
            previous.page == line.page
                && previous.row + 1 == line.row
                && (line.baseline[1] - previous.baseline[1]).abs()
                    <= line.font_size.max(previous.font_size) * 1.6
                && (line.bounds.left - previous.bounds.left).abs() <= line.font_size * 2.0
        }) && matches!(
            kind,
            ElementKind::Action | ElementKind::Dialogue | ElementKind::Parenthetical
        ) && screenplay.blocks.last().is_some_and(|block| {
            block.kind == kind && block.scene == line_scene && block.speaker == line_speaker
        });
        let block_id = if can_merge {
            let block = screenplay
                .blocks
                .last_mut()
                .expect("last block was checked");
            block.text.push('\n');
            block.text.push_str(text);
            block.lines.push(line.id.clone());
            block.confidence = block.confidence.min(confidence);
            block.id.clone()
        } else {
            if screenplay.blocks.len() >= options.max_blocks {
                return Err(Error::Limit("blocks"));
            }
            let id = format!("block-{}", screenplay.blocks.len() + 1);
            screenplay.blocks.push(Block {
                id: id.clone(),
                kind,
                text: text.into(),
                lines: vec![line.id.clone()],
                scene: line_scene,
                speaker: line_speaker,
                confidence,
            });
            id
        };
        if kind == ElementKind::SceneHeading {
            scene = Some(block_id.clone());
        } else if kind == ElementKind::Character {
            speaker = Some(block_id.clone());
        }
        if !reasons.is_empty() {
            if screenplay.doubts.len() >= options.max_doubts {
                return Err(Error::Limit("doubts"));
            }
            screenplay.doubts.push(Doubt {
                line: line.id.clone(),
                block: block_id,
                suggested_kind: kind,
                alternatives: match kind {
                    ElementKind::Character => vec![ElementKind::Action, ElementKind::Unknown],
                    ElementKind::Dialogue => vec![ElementKind::Action, ElementKind::Unknown],
                    ElementKind::Parenthetical => vec![ElementKind::Action, ElementKind::Dialogue],
                    ElementKind::SceneHeading => vec![ElementKind::Action],
                    _ => vec![ElementKind::Action, ElementKind::Character],
                },
                reasons,
            });
        }
        let line = &mut screenplay.lines[index];
        line.kind = kind;
        line.confidence = confidence;
    }
    Ok(())
}

fn scene_marker(text: &str) -> bool {
    let mut text = text.trim();
    if let Some((number, rest)) = text.split_once(char::is_whitespace) {
        if number.len() <= 8
            && number.starts_with(|character: char| character.is_ascii_digit())
            && number
                .chars()
                .all(|character| character.is_alphanumeric() || character == '.')
        {
            text = rest.trim_start();
        }
    }
    let text = text.to_uppercase();
    [
        "INT.",
        "EXT.",
        "INT/EXT.",
        "EXT/INT.",
        "INT./EXT.",
        "EXT./INT.",
        "I/E.",
        "INT. / EXT.",
        "EXT. / INT.",
        "ИНТ.",
        "НАТ.",
        "ЭКСТ.",
        "ИНТ/НАТ.",
        "НАТ/ИНТ.",
        "ИНТ./НАТ.",
        "НАТ./ИНТ.",
        "ИНТ. / НАТ.",
        "ИНТ/ЭКСТ.",
    ]
    .iter()
    .any(|marker| {
        text.strip_prefix(marker).is_some_and(|rest| {
            rest.is_empty()
                || (rest.starts_with(char::is_whitespace) && !rest.trim_start().starts_with('/'))
        })
    })
}

fn transition(text: &str) -> bool {
    let text = text.to_uppercase();
    let text = text.trim_end_matches([':', '.', ' ']);
    matches!(
        text,
        "CUT TO"
            | "SMASH CUT TO"
            | "MATCH CUT TO"
            | "DISSOLVE TO"
            | "FADE IN"
            | "FADE OUT"
            | "FADE TO BLACK"
            | "IRIS IN"
            | "IRIS OUT"
            | "СКЛЕЙКА"
            | "ЗАТЕМНЕНИЕ"
            | "ИЗ ЗАТЕМНЕНИЯ"
            | "НАПЛЫВ"
            | "ПЕРЕХОД К"
            | "КОНЕЦ"
            | "THE END"
    )
}

fn uppercase(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
        && text
            .chars()
            .filter(|character| character.is_alphabetic())
            .all(char::is_uppercase)
}

fn cue(text: &str) -> bool {
    let name = text.split_once('(').map_or(text, |(name, _)| name).trim();
    uppercase(name)
        && name.chars().count() <= 40
        && name.split_whitespace().count() <= 5
        && !name.ends_with(['.', ':', '!', '?'])
        && !name
            .chars()
            .any(|character| character.is_control() || ['[', ']'].contains(&character))
}

fn marginal_lines(screenplay: &Screenplay) -> Vec<bool> {
    let zone = |line: &SourceLine| {
        let height = screenplay.pages[line.page - 1].height;
        if line.bounds.top < height * 0.07 {
            1
        } else if line.bounds.bottom > height * 0.93 {
            2
        } else {
            0
        }
    };
    let mut counts = BTreeMap::<(&str, u8), (usize, usize)>::new();
    for line in &screenplay.lines {
        let zone = zone(line);
        if zone != 0 {
            let value = counts.entry((line.text.trim(), zone)).or_insert((0, 0));
            if value.0 != line.page {
                value.0 = line.page;
                value.1 += 1;
            }
        }
    }
    screenplay
        .lines
        .iter()
        .map(|line| {
            let text = line.text.trim();
            let zone = zone(line);
            zone != 0
                && !scene_marker(text)
                && !transition(text)
                && (counts.get(&(text, zone)).is_some_and(|value| value.1 >= 2)
                    || (!text.is_empty()
                        && text.chars().count() <= 10
                        && text.chars().all(|character| {
                            character.is_ascii_digit() || ".- ".contains(character)
                        })))
        })
        .collect()
}
