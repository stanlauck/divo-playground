# SPDX-License-Identifier: MIT OR Apache-2.0
"""Independent stdlib XML/CSV check of the invented checked-in output fixtures."""
import csv
import io
import json
from pathlib import Path
import uuid
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]


def main():
    source = json.loads((ROOT / "samples/synthetic.json").read_text(encoding="utf-8"))
    raw_xml = (ROOT / "samples/synthetic.fdx").read_bytes()
    assert b"<!DOCTYPE" not in raw_xml and b"<!ENTITY" not in raw_xml
    root = ET.fromstring(raw_xml)
    assert root.tag == "FinalDraft" and root.attrib["DocumentType"] == "Script"
    headings = root.findall("./Content/Paragraph[@Type='Scene Heading']")
    assert len(headings) == len(source["scenes"])
    cursor = 0
    for heading, scene in zip(headings, source["scenes"]):
        assert heading.attrib["Number"] == scene["number"]
        props = heading.find("SceneProperties")
        assert props.attrib["Length"] == f"{scene['pages_eighths']}/8"
        assert props.attrib["Page"] == str(cursor // 8 + 1)
        synopsis = props.find("./Summary/Paragraph/Text")
        assert (synopsis.text if synopsis is not None else "") == scene.get("synopsis", "")
        cursor += scene["pages_eighths"]
    categories = {n.attrib["Id"]: n.attrib for n in root.findall("./TagData/TagCategories/TagCategory")}
    assert categories["01fc9642-84ff-4366-b37c-a3068dee57e8"]["Name"] == "Cast Members"
    assert categories["05c556eb-6bc1-4a3a-b09f-f8b5ba1b6afa"]["Name"] == "Props"
    definitions = {n.attrib["Id"]: n.attrib for n in root.findall("./TagData/TagDefinitions/TagDefinition")}
    assert len(definitions) == len(root.findall("./TagData/TagDefinitions/TagDefinition"))
    for key, definition in definitions.items():
        assert uuid.UUID(key).version == 5
        assert definition["CatId"] in categories
    tags = {n.attrib["Number"]: n.findtext("DefId") for n in root.findall("./TagData/Tags/Tag")}
    assert len(tags) == len(definitions)
    tagged = root.findall(".//Text[@TagNumber]")
    assert tagged
    for node in tagged:
        assert definitions[tags[node.attrib["TagNumber"]]]["Label"] == (node.text or "")
    raw_csv = (ROOT / "samples/synthetic.csv").read_bytes()
    assert raw_csv.endswith(b"\r\n") and not raw_csv.startswith(b"\xef\xbb\xbf")
    rows = list(csv.DictReader(io.StringIO(raw_csv.decode("utf-8"), newline="")))
    assert [r["scene_id"] for r in rows] == ["yard", "workshop", "doorway"]
    assert rows[1]["pages_eighths"] == "13" and rows[1]["pages"] == "1 5/8"
    assert rows[1]["notes"] == source["scenes"][0]["notes"]
    assert not rows[2]["shooting_day_id"]
    elements = json.loads(rows[1]["elements_json"])
    assert elements[1] == {"id": "cup", "category": "props",
                           "name": "Copper cup & key", "quantity": 2}
    print(f"Independent Python XML/CSV: {len(headings)} scenes, {len(tags)} tag definitions, {len(rows)} schedule rows pass")


if __name__ == "__main__":
    main()
