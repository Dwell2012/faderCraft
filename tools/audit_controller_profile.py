"""Check a controller profile against an independently transcribed MIDI table.

Run: python tools/audit_controller_profile.py
The reference file records MIDI addresses, not expected app behaviour. This
separation prevents a profile edit from silently changing its own oracle.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "profiles/references/alphatheta-ddj-grv6.json"


def address(kind, channel, *data):
    result = {"kind": kind, "channel": channel}
    if kind == "cc14":
        result.update(msb=data[0], lsb=data[1])
    else:
        result["number"] = data[0]
    return result


def expected_sources(reference):
    expected = {}
    for deck_index, deck in enumerate("abcd"):
        prefix = f"deck_{deck}-"
        for name, number in reference["deck_note"].items():
            expected[prefix + name] = address("note", deck_index, number)
        for name, pair in reference["deck_cc14"].items():
            expected[prefix + name] = address("cc14", deck_index, *pair)
        expected[prefix + "load"] = address(
            "note", 6, reference["load_notes"][deck_index]
        )
        expected[prefix + "quantize-led"] = address("note", deck_index, 53)
        for slot in range(reference["hot_cue_slots"]):
            pad = f"{prefix}hot-cue-{slot + 1}"
            expected[pad] = address("note", 7 + 2 * deck_index, slot)
            expected[pad + "-delete"] = address("note", 8 + 2 * deck_index, slot)
            expected[pad + "-led"] = address("note", 7 + 2 * deck_index, slot)
    for name, pair in reference["mixer_cc14"].items():
        expected[name] = address("cc14", 6, *pair)
    for name, number in reference["browser_cc"].items():
        expected[name] = address("cc", 6, number)
    return expected


def audit(profile, reference):
    errors = []
    expected = expected_sources(reference)
    mappings = {entry["id"]: entry for entry in profile["mappings"]}
    if len(mappings) != len(profile["mappings"]):
        errors.append("duplicate mapping IDs")
    for mapping_id in sorted(expected.keys() | mappings.keys()):
        if mapping_id not in expected:
            errors.append(f"{mapping_id}: no documented reference address")
        elif mapping_id not in mappings:
            errors.append(f"{mapping_id}: missing from profile")
        elif mappings[mapping_id]["source"] != expected[mapping_id]:
            errors.append(
                f"{mapping_id}: profile {mappings[mapping_id]['source']} "
                f"!= document {expected[mapping_id]}"
            )
    # Input and output may legitimately share an address (button and its LED).
    inputs = defaultdict(list)
    for entry in profile["mappings"]:
        if entry["binding"]["type"] == "led":
            continue
        source = entry["source"]
        key = (entry["port"], source["kind"], source["channel"],
               source.get("number"), source.get("msb"), source.get("lsb"),
               json.dumps(entry.get("modifiers", []), sort_keys=True))
        inputs[key].append(entry["id"])
    for ids in inputs.values():
        if len(ids) > 1:
            errors.append(f"ambiguous input address: {', '.join(ids)}")
    return errors, len(expected), len(inputs)


def main():
    reference = json.loads(REFERENCE.read_text(encoding="utf-8"))
    profile = json.loads((ROOT / reference["profile"]).read_text(encoding="utf-8"))
    errors, documented, inputs = audit(profile, reference)
    for error in errors:
        print("ERROR:", error)
    print(f"{documented} documented bindings checked; {inputs} input addresses checked")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
