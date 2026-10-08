// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::*;
use hayro_interpret::{
    font::Glyph,
    hayro_cmap::BfString,
    hayro_syntax::{page::Rotation, LoadPdfError, Pdf},
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache,
    InterpreterSettings, InterpreterWarning, Paint, PathDrawMode, SoftMask, TransformExt,
};
use kurbo::{Affine, BezPath, Point, Rect};
use std::{
    io::Read,
    sync::{Arc, Mutex},
};

struct PlacedGlyph {
    text: String,
    bounds: Bounds,
    baseline: Point,
    font_size: f64,
    draw_order: usize,
    issues: Vec<Reason>,
}

struct Extractor<'o> {
    options: &'o ImportOptions,
    glyphs: Vec<PlacedGlyph>,
    callbacks: usize,
    text_bytes: usize,
    images: usize,
    error: Option<Error>,
    last: Option<(String, [f64; 6], bool)>,
}

impl Device<'_> for Extractor<'_> {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {
        self.images = self.images.saturating_add(1);
    }
    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        glyph_transform: Affine,
        _: &Paint<'_>,
        mode: &GlyphDrawMode,
    ) {
        if self.error.is_some() {
            return;
        }
        self.callbacks = self.callbacks.saturating_add(1);
        if self.callbacks > self.options.max_glyphs {
            self.error = Some(Error::Limit("glyph callbacks"));
            return;
        }
        let mut issues = Vec::new();
        let text = match glyph.as_unicode() {
            Some(BfString::Char(character)) => character.to_string(),
            Some(BfString::String(text)) => text,
            None => {
                issues.push(Reason::UnicodeMappingMissing);
                "\u{fffd}".into()
            }
        };
        let matrix = transform * glyph_transform;
        let coefficients = matrix.as_coeffs();
        // Fill+stroke makes two callbacks for one text glyph. Do not suppress arbitrary overprints.
        let stroke = matches!(mode, GlyphDrawMode::Stroke(_));
        if stroke
            && self
                .last
                .as_ref()
                .is_some_and(|(previous, affine, was_stroke)| {
                    previous == &text && affine == &coefficients && !was_stroke
                })
        {
            self.last = Some((text, coefficients, true));
            return;
        }
        if self.text_bytes.saturating_add(text.len()) > self.options.max_text_bytes {
            self.error = Some(Error::Limit("decoded text bytes"));
            return;
        }
        let baseline = matrix * Point::ORIGIN;
        let advance = match glyph {
            Glyph::Outline(glyph) => glyph.advance_width().map(f64::from).unwrap_or(600.0),
            Glyph::Type3(_) => 600.0,
        };
        let corners = [
            matrix * Point::new(0.0, -200.0),
            matrix * Point::new(advance, -200.0),
            matrix * Point::new(0.0, 800.0),
            matrix * Point::new(advance, 800.0),
        ];
        let em = matrix * Point::new(0.0, 1000.0);
        let font_size = em.distance(baseline);
        if !font_size.is_finite()
            || font_size <= 0.0
            || corners
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            || !baseline.x.is_finite()
            || !baseline.y.is_finite()
        {
            self.error = Some(Error::InvalidGeometry);
            return;
        }
        let axis = matrix * Point::new(1000.0, 0.0) - baseline;
        if axis.x <= 0.0 || axis.y.abs() > axis.x.abs() * 0.02 {
            issues.push(Reason::UnsupportedTextOrientation);
        }
        if text.chars().any(char::is_control) {
            issues.push(Reason::ControlCharacters);
        }
        self.text_bytes += text.len();
        self.last = Some((text.clone(), coefficients, stroke));
        self.glyphs.push(PlacedGlyph {
            text,
            bounds: Bounds {
                left: corners
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::INFINITY, f64::min),
                top: corners
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::INFINITY, f64::min),
                right: corners
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                bottom: corners
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::NEG_INFINITY, f64::max),
            },
            baseline,
            font_size,
            draw_order: self.callbacks,
            issues,
        });
    }
}

pub(crate) fn extract<R: Read>(reader: R, options: &ImportOptions) -> Result<Screenplay> {
    let mut bytes = Vec::new();
    reader
        .take(options.max_input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > options.max_input_bytes {
        return Err(Error::Limit("input bytes"));
    }
    if !bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|window| window == b"%PDF-")
    {
        return Err(Error::InvalidPdf);
    }
    let pdf = Pdf::new(bytes).map_err(|error| match error {
        LoadPdfError::Decryption(_) => Error::PasswordProtected,
        LoadPdfError::Invalid => Error::InvalidPdf,
    })?;
    if pdf.len() > options.max_objects {
        return Err(Error::Limit("PDF objects"));
    }
    if pdf.pages().len() > options.max_pages {
        return Err(Error::Limit("pages"));
    }
    let mut result = Screenplay {
        version: 1,
        coordinate_system: "crop_rotated_top_left_points".into(),
        pages: Vec::new(),
        lines: Vec::new(),
        blocks: Vec::new(),
        doubts: Vec::new(),
        warnings: Vec::new(),
    };
    let cache = InterpreterCache::new();
    let mut budget = crate::preflight::Budget::new(options);
    let mut callbacks = 0usize;
    let mut text_bytes = 0usize;
    for (index, page) in pdf.pages().iter().enumerate() {
        let number = index + 1;
        let warning_start = result.warnings.len();
        let content = page.page_stream();
        let inspection = budget.check(content.unwrap_or_default(), page.resources())?;
        for kind in inspection.warnings {
            warning(&mut result, options, kind, number)?;
        }
        let (width, height) = page.render_dimensions();
        let user_unit = if page.raw().contains_key(b"UserUnit") {
            page.raw()
                .get::<f64>(b"UserUnit")
                .ok_or(Error::InvalidGeometry)?
        } else {
            1.0
        };
        if !user_unit.is_finite() || user_unit <= 0.0 || user_unit > 75_000.0 {
            return Err(Error::InvalidGeometry);
        }
        let (width, height) = (f64::from(width) * user_unit, f64::from(height) * user_unit);
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(Error::InvalidGeometry);
        }
        let backend_warnings = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&backend_warnings);
        let settings = InterpreterSettings {
            render_annotations: false,
            warning_sink: Arc::new(move |warning| {
                let kind = match warning {
                    InterpreterWarning::UnsupportedFont => WarningKind::BackendUnsupportedFont,
                    InterpreterWarning::ImageDecodeFailure => {
                        WarningKind::BackendImageDecodeFailure
                    }
                };
                let mut warnings = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if !warnings.contains(&kind) {
                    warnings.push(kind);
                }
            }),
            ..InterpreterSettings::default()
        };
        let mut context = Context::new(
            Affine::scale(user_unit) * page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, width, height),
            &cache,
            pdf.xref(),
            settings,
        );
        let mut extractor = Extractor {
            options,
            glyphs: Vec::new(),
            callbacks,
            text_bytes,
            images: 0,
            error: None,
            last: None,
        };
        interpret_page(page, &mut context, &mut extractor);
        if let Some(error) = extractor.error {
            return Err(error);
        }
        callbacks = extractor.callbacks;
        text_bytes = extractor.text_bytes;
        for kind in backend_warnings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
        {
            warning(&mut result, options, *kind, number)?;
        }
        let image_failed = result.warnings[warning_start..]
            .iter()
            .any(|warning| warning.kind == WarningKind::BackendImageDecodeFailure);
        let images = extractor
            .images
            .max(inspection.images)
            .max(usize::from(image_failed));
        let status = if !extractor.glyphs.is_empty() {
            PageStatus::Text
        } else if images == 0
            && content.is_none_or(|content| content.iter().all(u8::is_ascii_whitespace))
        {
            PageStatus::Blank
        } else {
            PageStatus::UnsupportedNoTextLayer
        };
        if status == PageStatus::Blank {
            warning(&mut result, options, WarningKind::BlankPage, number)?;
        } else if status == PageStatus::UnsupportedNoTextLayer {
            warning(&mut result, options, WarningKind::NoExtractableText, number)?;
        }
        if images > 0 {
            warning(
                &mut result,
                options,
                WarningKind::ImageContentNotImported,
                number,
            )?;
        }
        let mut lines = assemble_lines(extractor.glyphs, number, result.lines.len(), options)?;
        if result.warnings[warning_start..].iter().any(|warning| {
            matches!(
                warning.kind,
                WarningKind::BackendUnsupportedFont
                    | WarningKind::BackendInvalidResource
                    | WarningKind::BackendUnknownOperator
            )
        }) {
            for line in &mut lines {
                line.issues.push(Reason::BackendUncertainText);
            }
        }
        let line_ids = lines.iter().map(|line| line.id.clone()).collect();
        result.lines.extend(lines);
        result.pages.push(Page {
            number,
            width,
            height,
            user_unit,
            rotation: match page.rotation() {
                Rotation::None => 0,
                Rotation::Horizontal => 90,
                Rotation::Flipped => 180,
                Rotation::FlippedHorizontal => 270,
            },
            status,
            images,
            lines: line_ids,
        });
    }
    Ok(result)
}

fn warning(
    result: &mut Screenplay,
    options: &ImportOptions,
    kind: WarningKind,
    page: usize,
) -> Result<()> {
    if result.warnings.len() >= options.max_warnings {
        return Err(Error::Limit("warnings"));
    }
    result.warnings.push(Warning { kind, page });
    Ok(())
}

fn assemble_lines(
    mut glyphs: Vec<PlacedGlyph>,
    page: usize,
    existing: usize,
    options: &ImportOptions,
) -> Result<Vec<SourceLine>> {
    glyphs.sort_by(|a, b| {
        a.baseline
            .y
            .total_cmp(&b.baseline.y)
            .then(a.baseline.x.total_cmp(&b.baseline.x))
            .then(a.draw_order.cmp(&b.draw_order))
    });
    let mut rows: Vec<Vec<PlacedGlyph>> = Vec::new();
    let mut row_y = 0.0;
    let mut row_size = 0.0;
    for glyph in glyphs {
        if rows.is_empty() || (glyph.baseline.y - row_y).abs() > glyph.font_size.min(row_size) * 0.3
        {
            row_y = glyph.baseline.y;
            row_size = glyph.font_size;
            rows.push(Vec::new());
        }
        rows.last_mut().expect("row was inserted").push(glyph);
    }
    let mut lines = Vec::new();
    for (row_index, mut row) in rows.into_iter().enumerate() {
        row.sort_by(|a, b| {
            a.baseline
                .x
                .total_cmp(&b.baseline.x)
                .then(a.draw_order.cmp(&b.draw_order))
        });
        let mut segments: Vec<Vec<PlacedGlyph>> = Vec::new();
        let mut previous_right = 0.0;
        for glyph in row {
            if segments.is_empty() || glyph.bounds.left - previous_right > glyph.font_size * 4.0 {
                segments.push(Vec::new());
            }
            previous_right = glyph.bounds.right;
            segments
                .last_mut()
                .expect("segment was inserted")
                .push(glyph);
        }
        let columns = segments.len() > 1;
        for segment in segments {
            if existing.saturating_add(lines.len()) >= options.max_lines {
                return Err(Error::Limit("source lines"));
            }
            let first = &segment[0];
            let mut line = SourceLine {
                id: format!("line-{}", existing + lines.len() + 1),
                page,
                index: lines.len() + 1,
                row: row_index + 1,
                draw_order: segment.iter().map(|glyph| glyph.draw_order).min().unwrap(),
                text: String::new(),
                raw_text: String::new(),
                bounds: first.bounds,
                baseline: [first.baseline.x, first.baseline.y],
                font_size: first.font_size,
                inferred_spaces: 0,
                issues: Vec::new(),
                kind: ElementKind::Unknown,
                confidence: Confidence::Low,
            };
            let mut previous: Option<&PlacedGlyph> = None;
            for glyph in &segment {
                if let Some(previous) = previous {
                    let gap = glyph.bounds.left - previous.bounds.right;
                    if gap > glyph.font_size.min(previous.font_size) * 0.25
                        && !line.text.ends_with(char::is_whitespace)
                        && !glyph.text.starts_with(char::is_whitespace)
                    {
                        line.text.push(' ');
                        line.inferred_spaces += 1;
                    }
                }
                line.text.push_str(&glyph.text);
                line.raw_text.push_str(&glyph.text);
                line.bounds = line.bounds.union(glyph.bounds);
                for reason in &glyph.issues {
                    if !line.issues.contains(reason) {
                        line.issues.push(*reason);
                    }
                }
                previous = Some(glyph);
            }
            if columns {
                line.issues.push(Reason::MultipleColumns);
            }
            lines.push(line);
        }
    }
    Ok(lines)
}
