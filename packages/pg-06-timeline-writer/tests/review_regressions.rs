// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::{Value, json};
use timeline_writer::*;

fn one_shot() -> ShotList {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    let mut s = Shot::new("external", 24);
    s.media = Some(Media {
        url: "file:///synthetic/media.mov".into(),
        source_in_frame: Some(24),
    });
    list.shots.push(s);
    list
}

#[test]
fn default_display_names_and_ordinary_gaps_do_not_double_text_budget() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots = (0..MAX_SHOTS)
        .map(|i| {
            let mut s = Shot::new(format!("s{i:04}{}", "x".repeat(98)), 1);
            s.gap_before_frames = Some(1);
            s
        })
        .collect();
    let list = list.normalized().unwrap(); // 1,030,000 canonical ID bytes
    assert_eq!(from_otio(&to_otio(&list).unwrap()).unwrap(), list);
    assert_eq!(from_fcpxml(&to_fcpxml(&list).unwrap()).unwrap(), list);
    // Raw redundant default names are individually checked, canonically uncharged.
    let mut redundant = list.clone();
    for s in &mut redundant.shots {
        s.name = Some(s.id.clone());
    }
    assert_eq!(redundant.normalized().unwrap(), list);
}

#[test]
fn available_range_origin_is_subtracted_from_otio_source_in() {
    let list = one_shot();
    let mut v: Value = serde_json::from_str(&to_otio(&list).unwrap()).unwrap();
    let clip = &mut v["tracks"]["children"][0]["children"][0];
    clip["source_range"]["start_time"]["value"] = json!(86424);
    clip["media_references"]["DEFAULT_MEDIA"]["available_range"] = json!({
        "OTIO_SCHEMA":"TimeRange.1",
        "start_time":{"OTIO_SCHEMA":"RationalTime.1","value":86400,"rate":24},
        "duration":{"OTIO_SCHEMA":"RationalTime.1","value":100,"rate":24}
    });
    let parsed = from_otio(&v.to_string()).unwrap();
    assert_eq!(parsed, list);
    assert_eq!(from_fcpxml(&to_fcpxml(&parsed).unwrap()).unwrap(), list);
}

#[test]
fn non_uri_ascii_is_rejected_instead_of_whatwg_repaired() {
    for character in ['"', '<', '>', '`', '{', '}', '^', '|', '\\', ' ', '\u{7f}'] {
        for scheme in ["https://example.invalid/", "file:///synthetic/"] {
            let mut list = one_shot();
            list.shots[0].media.as_mut().unwrap().url = format!("{scheme}a{character}b.mov");
            assert!(list.normalized().is_err());
        }
    }
    let mut list = one_shot();
    list.shots[0].media.as_mut().unwrap().url = "https://example.invalid/a%22b.mov".into();
    assert!(list.normalized().is_ok());
    for url in [
        "https:///example.invalid/a",
        "https://@example.invalid/a",
        "https://example.invalid/a[b].mov",
        "file:///synthetic/a[b].mov",
    ] {
        let mut list = one_shot();
        list.shots[0].media.as_mut().unwrap().url = url.into();
        assert!(list.normalized().is_err());
    }
    let mut list = one_shot();
    list.shots[0].media.as_mut().unwrap().url = "https://[2001:db8::1]/synthetic.mov".into();
    assert!(list.normalized().is_ok());
}

fn shared_xml(url: &str, count: usize) -> String {
    let mut clips = String::new();
    for i in 0..count {
        clips.push_str(&format!("<asset-clip ref=\"a\" name=\"s{i}\" offset=\"{i}s\" start=\"0s\" duration=\"1s\"><metadata><md key=\"shot_list.id\" value=\"s{i}\"/></metadata></asset-clip>"));
    }
    format!(
        "<fcpxml version=\"1.9\"><resources><format id=\"f\" frameDuration=\"1/24s\" width=\"1920\" height=\"1080\"/><asset id=\"a\" hasVideo=\"1\" hasAudio=\"0\" duration=\"1s\"><media-rep src=\"{url}\"/></asset></resources><project><sequence format=\"f\" duration=\"{count}s\"><spine>{clips}</spine></sequence></project></fcpxml>"
    )
}

#[test]
fn repeated_shared_urls_are_limited_before_owned_shot_copies() {
    let large = format!("https://example.invalid/{}", "a".repeat(1024 * 1024));
    // The source is small, not the approximately 10GiB that naive expansion could allocate.
    let input = shared_xml(&large, MAX_SHOTS);
    assert!(input.len() < MAX_JSON_BYTES);
    let error = from_fcpxml(&input).unwrap_err();
    assert_eq!(error.code, "text_limit");
    assert_eq!(error.path, "asset.media-rep.src"); // rejected before expanding clips
    let valid_but_repeated = format!("https://example.invalid/{}", "a".repeat(8000));
    let input = shared_xml(&valid_but_repeated, MAX_SHOTS);
    let error = from_fcpxml(&input).unwrap_err();
    assert_eq!(error.code, "text_limit");
    assert!(error.path.starts_with("spine[") && error.path.ends_with(".media.url"));
}

#[test]
fn fallback_reserved_ids_use_monotonic_search_without_changing_ids() {
    let mut list = ShotList::new(FrameRate::new(24, 1));
    list.shots = (0..MAX_SHOTS)
        .map(|i| Shot::new(format!("s{i}"), 1))
        .collect();
    let mut v: Value = serde_json::from_str(&to_otio(&list).unwrap()).unwrap();
    let items = v["tracks"]["children"][0]["children"]
        .as_array_mut()
        .unwrap();
    for (i, clip) in items.iter_mut().enumerate() {
        clip["name"] = json!("");
        clip["metadata"] = if i < 5000 {
            json!({})
        } else {
            json!({"shot_list":{"id":format!("shot-{}",i-4999)}})
        };
    }
    let parsed = from_otio(&v.to_string()).unwrap();
    assert_eq!(parsed.shots[0].id, "shot-5001");
    assert_eq!(parsed.shots[4999].id, "shot-10000");
    assert_eq!(parsed.shots[5000].id, "shot-1");
    assert_eq!(parsed.shots[9999].id, "shot-5000");
}
