# SPDX-License-Identifier: MIT OR Apache-2.0
"""Optional, offline native-format/timecode/schema checks. See README."""

import argparse
import copy
import json
import subprocess
from pathlib import Path
from fractions import Fraction
from urllib.parse import unquote

import jsonschema
import opentimelineio as otio
from timecode import Timecode


ROOT = Path(__file__).resolve().parents[1]


def rate(value):
    if isinstance(value, int):
        return Fraction(value, 1)
    return Fraction(value["num"], value["den"])


def oracle(label, fps, drop):
    if drop:
        label = label[:8] + ";" + label[9:]
    return Timecode(str(fps), start_timecode=label, force_non_drop_frame=not drop).frame_number


def rows(text):
    return [line.split() for line in text.splitlines() if line[:1].isdigit()]


def native_check(text, fps, expected_rows):
    timeline = otio.adapters.read_from_string(text, "cmx_3600", rate=float(fps))
    assert len(timeline.tracks) == 1
    track = timeline.tracks[0]
    assert len(track) == len(expected_rows)
    for item, expected in zip(track, expected_rows):
        source, duration, _record = expected
        assert isinstance(item, otio.schema.Clip)
        assert item.source_range.start_time.value == source
        assert item.source_range.duration.value == duration
    return timeline


def convert(binary, source, target, text, fps=None, success=True):
    args = [str(binary), "--from", source, "--to", target, "--input", "-", "--output", "-"]
    if fps is not None:
        args += ["--fps", str(fps)]
    result = subprocess.run(args, input=text.encode("utf-8"), capture_output=True, check=False)
    assert (result.returncode == 0) == success, result.stderr.decode("utf-8")
    if success:
        assert not result.stderr
    else:
        assert not result.stdout
    return result.stdout.decode("utf-8")


def samples(binary, schema):
    total_events = 0
    for name in ("synthetic", "drop-frame"):
        data = json.loads((ROOT / "samples" / f"{name}.json").read_text(encoding="utf-8"))
        jsonschema.validate(data, schema)
        raw_json = json.dumps(data, ensure_ascii=False)
        normalized = json.loads(convert(binary, "json", "json", raw_json))
        edl = convert(binary, "json", "edl", raw_json)
        assert edl == (ROOT / "samples" / f"{name}.edl").read_text(encoding="utf-8")
        assert json.loads(convert(binary, "edl", "json", edl)) == normalized
        fps = rate(data["frame_rate"])
        drop = data.get("timecode_mode") == "drop"
        expected = []
        cursor = oracle(data.get("record_start", "00:00:00:00"), fps, drop)
        for shot in data["shots"]:
            gap = shot.get("gap_before_frames", 0)
            if gap:
                expected.append((0, gap, cursor))
                cursor += gap
            expected.append((shot.get("media", {}).get("source_in_frame", 0),
                             shot["duration_frames"], cursor))
            cursor += shot["duration_frames"]
        event_rows = rows(edl)
        assert len(event_rows) == len(expected)
        for event, (source, duration, record) in zip(event_rows, expected):
            si, so, ri, ro = [oracle(tc, fps, drop) for tc in event[4:]]
            assert (si, so - si, ri, ro - ri) == (source, duration, record, duration)
        ids = [unquote(line.removeprefix("* SHOT ID: "))
               for line in edl.splitlines() if line.startswith("* SHOT ID: ")]
        assert ids == [shot["id"] for shot in data["shots"]]
        if not drop:
            native_check(edl, fps, expected)
        else:
            # The pinned official adapter ignores FCM and treats colon labels
            # as non-drop. Record the limitation, do not silently claim raw
            # DF compatibility. Semicolon normalization exercises OTIO's
            # independent native DF decoder, without changing frame labels.
            mismatch = False
            try:
                native_check(edl, fps, expected)
            except (AssertionError, otio.exceptions.OTIOError, ValueError):
                mismatch = True
            assert mismatch, "Update compatibility evidence if native FCM support changes."
            fixed = []
            for line in edl.splitlines():
                if line[:1].isdigit():
                    fields = line.split()
                    fields[4:] = [tc[:8] + ";" + tc[9:] for tc in fields[4:]]
                    line = " ".join(fields)
                fixed.append(line)
            native_check("\n".join(fixed) + "\n", fps, expected)
        total_events += len(expected)
    return total_events


def external_checks(binary):
    # Independently authored native OTIO clips -> official CMX writer -> Rust.
    timeline = otio.schema.Timeline(name="Invented native external")
    track = otio.schema.Track(kind=otio.schema.TrackKind.Video)
    for i, duration in enumerate((24, 36)):
        clip = otio.schema.Clip(
            name=f"Native {i + 1}",
            media_reference=otio.schema.ExternalReference(
                target_url=f"file:///synthetic/native-{i + 1}.mov"),
            source_range=otio.opentime.TimeRange(
                otio.opentime.RationalTime(12, 24),
                otio.opentime.RationalTime(duration, 24)),
        )
        clip.metadata["cmx_3600"] = {"reel": f"CAM{i + 1}"}
        track.append(clip)
    timeline.tracks.append(track)
    text = otio.adapters.write_to_string(timeline, "cmx_3600")
    # The native writer emits no FCM; the external importer defaults to NDF.
    imported = json.loads(convert(binary, "edl", "json", text, fps="24"))
    assert [s["duration_frames"] for s in imported["shots"]] == [24, 36]
    assert [s["media"]["source_in_frame"] for s in imported["shots"]] == [12, 12]
    assert [s["media"]["reel"] for s in imported["shots"]] == ["CAM1", "CAM2"]
    assert [s["media"]["url"] for s in imported["shots"]] == [
        "file:///synthetic/native-1.mov", "file:///synthetic/native-2.mov"]


def schema_checks(binary, schema):
    jsonschema.Draft202012Validator.check_schema(schema)
    valid = {"version": 1, "frame_rate": 24, "shots": [{"id": "one", "duration_frames": 24}]}
    negatives = []
    for key, values in {
        "version": [0, 2, None], "frame_rate": [23.976, 60, {"num": 24, "den": 0}],
        "timecode_mode": ["unknown", None], "record_start": ["bad", "24:00:00:00", "00:00:00:00\n"],
        "resolution": [[0, 1], [1], [1, 2, 3], [1, 40000]], "shots": [None, {}],
    }.items():
        for value in values:
            data = copy.deepcopy(valid)
            data[key] = value
            negatives.append(data)
    for key, values in {
        "id": ["", " spaced ", 1, "bad\nline", "bad\n", "\u0007"], "duration_frames": [0, -1, 1.5, None],
        "name": ["x" * 4097, "bad\u2028line", "bad\n"], "gap_before_frames": [-1, 1.5],
        "media": [{"reel": ""}, {"source_in_frame": -1}, {"url": "http://example.invalid/a"},
                  {"url": "https://example.invalid/a\n"}],
    }.items():
        for value in values:
            data = copy.deepcopy(valid)
            data["shots"][0][key] = value
            negatives.append(data)
    for field in ("version", "frame_rate", "shots"):
        data = copy.deepcopy(valid)
        del data[field]
        negatives.append(data)
    for data in negatives:
        assert list(jsonschema.Draft202012Validator(schema).iter_errors(data))
        convert(binary, "json", "json", json.dumps(data), success=False)
    return len(negatives)


def timecodes(vector_path):
    total = 0
    for line in vector_path.read_text(encoding="utf-8-sig").splitlines():
        fps, mode, frame, label = line.split("\t")
        drop = mode == "drop"
        expected = Timecode(fps, frames=int(frame) + 1, force_non_drop_frame=not drop)
        assert str(expected) == label, (fps, mode, frame, str(expected), label)
        assert Timecode(fps, start_timecode=label, force_non_drop_frame=not drop).frame_number == int(frame)
        total += 1
    assert total > 70_000
    return total


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--vectors", required=True, type=Path)
    args = parser.parse_args()
    schema = json.loads((ROOT / "schema.json").read_text(encoding="utf-8"))
    events = samples(args.binary, schema)
    external_checks(args.binary)
    negatives = schema_checks(args.binary, schema)
    vectors = timecodes(args.vectors)
    print(f"PASS: {events} sample CMX events, native NDF and explicit DF normalization; "
          f"native external import; {negatives} schema/runtime negatives; {vectors} independent timecode vectors")
    print("LIMITATION: pinned official CMX adapter ignores FCM on raw colon DF EDL; no live editor certification.")


if __name__ == "__main__":
    main()
