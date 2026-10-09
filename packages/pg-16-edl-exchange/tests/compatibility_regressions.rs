// SPDX-License-Identifier: MIT OR Apache-2.0

use edl_exchange::{FrameRate, Media, Shot, ShotList, from_edl, to_edl};

#[test]
fn manifest_follows_events_and_cannot_precede_or_interrupt_them() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots.push(Shot::new("ONE", 24));
    let edl = to_edl(&list).unwrap();
    let manifest = edl.lines().last().unwrap();
    assert!(manifest.starts_with("* PG16 TIMELINE: "));
    assert!(!edl.lines().take(3).any(|l| l.starts_with('*')));
    let without = edl.replace(&format!("{manifest}\n"), "");
    let prefixed = format!("{manifest}\n{without}");
    assert!(from_edl(&prefixed, None).is_err());
    assert!(
        from_edl(
            &format!("{edl}002 AX V C 00:00:00:00 00:00:01:00 00:00:01:00 00:00:02:00\n"),
            None
        )
        .is_err()
    );
}

#[test]
fn original_manual_non_drop_spelling_and_native_title_limit_are_supported() {
    let external = "TITLE: INVENTED\nFCM: NON DROP FRAME\n001 CAM V C 00:00:00:00 00:00:01:00 00:00:00:00 00:00:01:00\n";
    assert_eq!(
        from_edl(external, Some(FrameRate::new(24, 1)))
            .unwrap()
            .shots[0]
            .duration_frames,
        24
    );
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.title = Some(format!("{} / 测试", "invented".repeat(20)));
    list.shots.push(Shot::new("ONE", 24));
    let edl = to_edl(&list).unwrap();
    let title = edl.lines().next().unwrap().strip_prefix("TITLE: ").unwrap();
    assert_eq!(title.len(), 70);
    assert!(
        title
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b' ')
    );
    assert_eq!(from_edl(&edl, None).unwrap(), list);
}

#[test]
fn drive_letter_authority_is_rejected_not_whatwg_repaired() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    for bad in ["file://C:/clip.mov", "file://example.invalid:123/clip.mov"] {
        list.shots = vec![Shot {
            media: Some(Media {
                url: Some(bad.into()),
                ..Media::default()
            }),
            ..Shot::new("one", 24)
        }];
        assert!(list.normalized().is_err());
    }
    for valid in [
        "file:///C:/synthetic/clip.mov",
        "file://example.invalid/synthetic/clip.mov",
    ] {
        list.shots = vec![Shot {
            media: Some(Media {
                url: Some(valid.into()),
                ..Media::default()
            }),
            ..Shot::new("one", 24)
        }];
        assert_eq!(from_edl(&to_edl(&list).unwrap(), None).unwrap(), list);
    }
}
