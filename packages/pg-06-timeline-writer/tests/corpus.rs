// SPDX-License-Identifier: MIT OR Apache-2.0

use timeline_writer::*;

#[test]
fn synthetic_goldens_are_exact_and_roundtrip_complete_document() {
    let list = from_json(include_str!("../samples/synthetic.json")).unwrap();
    assert_eq!(
        to_otio(&list).unwrap(),
        include_str!("../samples/synthetic.otio")
    );
    assert_eq!(
        to_fcpxml(&list).unwrap(),
        include_str!("../samples/synthetic.fcpxml")
    );
    assert_eq!(
        from_otio(include_str!("../samples/synthetic.otio")).unwrap(),
        list
    );
    assert_eq!(
        from_fcpxml(include_str!("../samples/synthetic.fcpxml")).unwrap(),
        list
    );
}
