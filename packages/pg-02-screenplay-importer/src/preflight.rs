// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::*;
use hayro_interpret::hayro_syntax::{
    content::{ops::TypedInstruction, TypedIter},
    object::{Dict, Name, ObjectIdentifier},
    page::Resources,
};
use std::collections::BTreeSet;

pub(crate) struct Budget<'o> {
    options: &'o ImportOptions,
    decoded: usize,
    operations: usize,
}

impl<'o> Budget<'o> {
    pub(crate) fn new(options: &'o ImportOptions) -> Self {
        Self {
            options,
            decoded: 0,
            operations: 0,
        }
    }

    pub(crate) fn check(
        &mut self,
        content: &[u8],
        resources: &Resources<'_>,
    ) -> Result<Vec<WarningKind>> {
        let mut warnings = Vec::new();
        self.walk(content, resources, 0, &mut BTreeSet::new(), &mut warnings)?;
        Ok(warnings)
    }

    fn walk(
        &mut self,
        content: &[u8],
        resources: &Resources<'_>,
        depth: usize,
        active: &mut BTreeSet<ObjectIdentifier>,
        warnings: &mut Vec<WarningKind>,
    ) -> Result<()> {
        if content.len() > self.options.max_decoded_page_bytes {
            return Err(Error::Limit("decoded page or Form bytes"));
        }
        self.decoded = self.decoded.saturating_add(content.len());
        if self.decoded > self.options.max_decoded_content_bytes {
            return Err(Error::Limit("expanded decoded content bytes"));
        }
        let mut operations = TypedIter::new(content);
        while let Some(operation) = operations.next() {
            self.operations = self.operations.saturating_add(1);
            if self.operations > self.options.max_operations {
                return Err(Error::Limit("expanded content operations"));
            }
            match operation {
                TypedInstruction::XObject(object) => {
                    let Some(stream) = resources.get_x_object(object.0) else {
                        add(warnings, WarningKind::BackendInvalidResource);
                        continue;
                    };
                    let Some(subtype) = stream.dict().get::<Name<'_>>(b"Subtype") else {
                        add(warnings, WarningKind::BackendInvalidResource);
                        continue;
                    };
                    if subtype.as_ref() != b"Form" {
                        continue;
                    }
                    if depth >= self.options.max_form_depth {
                        return Err(Error::Limit("Form depth"));
                    }
                    let id = stream.obj_id();
                    if !active.insert(id) {
                        return Err(Error::InvalidPdf);
                    }
                    let decoded = stream.decoded().map_err(|_| Error::InvalidPdf)?;
                    let local = stream
                        .dict()
                        .get::<Dict<'_>>(b"Resources")
                        .unwrap_or_default();
                    let resources = Resources::from_parent(local, resources.clone());
                    self.walk(decoded.as_ref(), &resources, depth + 1, active, warnings)?;
                    active.remove(&id);
                }
                TypedInstruction::TextFont(font) => {
                    let supported = resources
                        .get_font(font.0)
                        .and_then(|font| font.get::<Name<'_>>(b"Subtype"))
                        .is_some_and(|subtype| {
                            matches!(
                                subtype.as_ref(),
                                b"Type1"
                                    | b"MMType1"
                                    | b"Type3"
                                    | b"Type0"
                                    | b"TrueType"
                                    | b"OpenType"
                            )
                        });
                    if !supported {
                        add(warnings, WarningKind::BackendInvalidResource);
                    }
                }
                TypedInstruction::Fallback(_) => add(warnings, WarningKind::BackendUnknownOperator),
                _ => {}
            }
        }
        Ok(())
    }
}

fn add(warnings: &mut Vec<WarningKind>, kind: WarningKind) {
    if !warnings.contains(&kind) {
        warnings.push(kind);
    }
}
