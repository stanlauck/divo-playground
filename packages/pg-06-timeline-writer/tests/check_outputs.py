# SPDX-License-Identifier: MIT OR Apache-2.0
"""Offline, independent OTIO/DTD/JSON-schema oracles. No media or network I/O."""

import argparse
import copy
import json
import subprocess
import tempfile
from fractions import Fraction
from pathlib import Path

import jsonschema
import opentimelineio as otio
from lxml import etree


def seconds(text):
    assert text.endswith("s")
    return Fraction(text[:-1])


def expected_neutral(data):
    out = {key: data[key] for key in ("version", "title", "frame_rate", "record_start", "resolution")}
    out["shots"] = []
    for source in data["shots"]:
        shot = {key: value for key, value in source.items() if key != "camera"}
        out["shots"].append(shot)
    return out


def check_schema(root, data):
    validator = jsonschema.Draft202012Validator(json.loads((root / "schema.json").read_text("utf-8")))
    validator.check_schema(validator.schema)
    validator.validate(data)
    bad = []
    for ch in ["\n", "\r", "\t", "\x00", "\x85", "\u2028", "\u2029", "\ufffe", "\uffff"]:
        for field in ["title", "name", "id"]:
            for value in [f"synthetic{ch}text", f"synthetic{ch}"]:
                changed = copy.deepcopy(data)
                target = changed if field == "title" else changed["shots"][0]
                target[field] = value
                bad.append(changed)
    for value in [0, -1, 1.5]:
        changed = copy.deepcopy(data)
        changed["shots"][0]["duration_frames"] = value
        bad.append(changed)
    for changed in bad:
        assert not validator.is_valid(changed)
    return len(bad)


def check_otio(root, data, rate, work):
    timeline = otio.adapters.read_from_file(str(root / "samples" / "synthetic.otio"), "otio_json")
    assert isinstance(timeline, otio.schema.Timeline)
    assert timeline.name == data["title"]
    assert timeline.global_start_time.value == 86400
    assert timeline.global_start_time.rate == float(rate)
    assert len(timeline.tracks) == 1
    track = timeline.tracks[0]
    assert track.kind == otio.schema.TrackKind.Video
    assert timeline.duration().value == sum(s["duration_frames"] + s.get("gap_before_frames", 0) for s in data["shots"])
    index = 0
    cursor = 0
    for shot in data["shots"]:
        gap = shot.get("gap_before_frames", 0)
        if gap:
            item = track[index]
            assert isinstance(item, otio.schema.Gap) and item.duration().value == gap
            index += 1
            cursor += gap
        clip = track[index]
        assert isinstance(clip, otio.schema.Clip)
        assert clip.metadata["shot_list"]["id"] == shot["id"]
        assert clip.name == shot.get("name", shot["id"])
        assert clip.duration().value == shot["duration_frames"]
        assert clip.range_in_parent().start_time.value == cursor
        assert clip.source_range.start_time.value == shot.get("media", {}).get("source_in_frame", 0)
        if "media" in shot:
            assert isinstance(clip.media_reference, otio.schema.ExternalReference)
            assert clip.media_reference.target_url == shot["media"]["url"]
        else:
            assert isinstance(clip.media_reference, otio.schema.MissingReference)
        cursor += shot["duration_frames"]
        index += 1
    assert index == len(track)
    native = work / "native.otio"
    otio.adapters.write_to_file(timeline, str(native), "otio_json")
    neutral = work / "native.json"
    subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "--offline", "--", "--from", "otio", "--to", "json",
         "--input", str(native), "--output", str(neutral)],
        cwd=root, check=True, capture_output=True,
    )
    assert json.loads(neutral.read_text("utf-8")) == expected_neutral(data)
    # An external native timeline with nonzero media timecode origin, not made
    # by our writer: neutral source-in must remain relative to media beginning.
    rational = otio.opentime.RationalTime
    time_range = otio.opentime.TimeRange
    reference = otio.schema.ExternalReference(
        target_url="file:///synthetic/origin.mov",
        available_range=time_range(rational(86400, 24), rational(96, 24)),
    )
    clip = otio.schema.Clip(
        name="Synthetic origin clip", media_reference=reference,
        source_range=time_range(rational(86424, 24), rational(24, 24)),
        metadata={"shot_list": {"id": "external-origin"}},
    )
    external = otio.schema.Timeline(name="Synthetic external origin")
    external.tracks.append(otio.schema.Track(children=[clip], kind=otio.schema.TrackKind.Video))
    assert clip.available_range().start_time.value == 86400
    native = work / "external-origin.otio"
    neutral = work / "external-origin.json"
    otio.adapters.write_to_file(external, str(native), "otio_json")
    subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "--offline", "--", "--from", "otio", "--to", "json",
         "--input", str(native), "--output", str(neutral)],
        cwd=root, check=True, capture_output=True,
    )
    result = json.loads(neutral.read_text("utf-8"))
    assert result == {
        "version": 1, "title": "Synthetic external origin", "frame_rate": 24,
        "shots": [{"id": "external-origin", "name": "Synthetic origin clip", "duration_frames": 24,
                   "media": {"url": "file:///synthetic/origin.mov", "source_in_frame": 24}}],
    }
    empty = otio.schema.Timeline(name="Synthetic native empty")
    empty.global_start_time = rational(86400, 24)
    empty.tracks.append(otio.schema.Track(kind=otio.schema.TrackKind.Video))
    native = work / "external-empty.otio"
    neutral = work / "external-empty.json"
    otio.adapters.write_to_file(empty, str(native), "otio_json")
    subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "--offline", "--", "--from", "otio", "--to", "json",
         "--input", str(native), "--output", str(neutral)],
        cwd=root, check=True, capture_output=True,
    )
    assert json.loads(neutral.read_text("utf-8")) == {
        "version": 1, "title": "Synthetic native empty", "frame_rate": 24,
        "record_start": "01:00:00:00", "shots": [],
    }


def check_xml(root, data, rate, dtd_path):
    parser = etree.XMLParser(resolve_entities=False, no_network=True, load_dtd=False)
    document = etree.parse(str(root / "samples" / "synthetic.fcpxml"), parser)
    with dtd_path.open("rb") as file:
        dtd = etree.DTD(file)
    assert dtd.validate(document), str(dtd.error_log)
    formats = {node.get("id"): node for node in document.findall("./resources/format")}
    assets = {node.get("id"): node for node in document.findall("./resources/asset")}
    assert len(formats) == 1 and len(assets) == 2
    project = document.find("./library/event/project")
    assert project.get("name") == data["title"]
    sequence = project.find("sequence")
    fmt = formats[sequence.get("format")]
    assert seconds(fmt.get("frameDuration")) == 1 / rate
    assert [int(fmt.get("width")), int(fmt.get("height"))] == data["resolution"]
    start = 86400 / rate
    assert seconds(sequence.get("tcStart")) == start and sequence.get("tcFormat") == "NDF"
    items = list(sequence.find("spine"))
    cursor = start
    index = 0
    refs = []
    for shot in data["shots"]:
        gap = shot.get("gap_before_frames", 0)
        if gap:
            item = items[index]
            assert item.tag == "gap" and item.find("metadata") is None
            assert seconds(item.get("offset")) == cursor and seconds(item.get("duration")) == gap / rate
            cursor += gap / rate
            index += 1
        item = items[index]
        assert seconds(item.get("offset")) == cursor
        assert seconds(item.get("duration")) == shot["duration_frames"] / rate
        assert item.get("name") == shot.get("name", shot["id"])
        assert item.find("./metadata/md[@key='shot_list.id']").get("value") == shot["id"]
        if "media" in shot:
            assert item.tag == "asset-clip" and item.get("srcEnable") == "video"
            asset = assets[item.get("ref")]
            refs.append(item.get("ref"))
            assert asset.find("media-rep").get("src") == shot["media"]["url"]
            source_in = shot["media"].get("source_in_frame", 0)
            assert seconds(item.get("start")) == source_in / rate
            assert seconds(item.get("start")) + seconds(item.get("duration")) <= seconds(asset.get("duration"))
        else:
            assert item.tag == "gap" and seconds(item.get("start")) == 0
        cursor += shot["duration_frames"] / rate
        index += 1
    assert index == len(items) and refs[0] == refs[1] != refs[2]
    assert seconds(sequence.get("duration")) == cursor - start


def main():
    args = argparse.ArgumentParser()
    args.add_argument("--dtd", type=Path, required=True)
    args.add_argument("--work", type=Path, required=True)
    options = args.parse_args()
    root = Path(__file__).resolve().parents[1]
    data = json.loads((root / "samples" / "synthetic.json").read_text("utf-8"))
    rate = Fraction(data["frame_rate"]["num"], data["frame_rate"]["den"])
    negatives = check_schema(root, data)
    options.work.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="pg06-check-", dir=options.work) as tmp:
        check_otio(root, data, rate, Path(tmp))
    check_xml(root, data, rate, options.dtd)
    print(f"PASS: native OTIO 0.18.1 read/reserialize/CLI, FCPXML 1.9 DTD/time/IDs, schema and {negatives} negatives")


if __name__ == "__main__":
    main()
