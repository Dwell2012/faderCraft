"""Check the DDJ-1000 profile against the independently transcribed MIDI table.

Run: python tools/audit_ddj1000_profile.py
profiles/references/pioneer-ddj-1000.json records addresses from Pioneer's
"DDJ-1000 List of MIDI Messages", not expected app behaviour. A passing audit
means the addresses match that document; it does not verify firmware behaviour,
LED colours or audio routing without hardware.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "profiles/references/pioneer-ddj-1000.json"
PROFILE = ROOT / "profiles/pioneer-ddj-1000.json"


def note(channel, number):
    return {"kind": "note", "channel": channel, "number": number}


def cc(channel, number):
    return {"kind": "cc", "channel": channel, "number": number}


def cc14(channel, msb, lsb):
    return {"kind": "cc14", "channel": channel, "msb": msb, "lsb": lsb}


def expected_sources(ref):
    exp = {}
    for d, name in enumerate("abcd"):
        p = f"deck_{name}-"
        for key, n in ref["deck_note"].items():
            exp[p + key] = note(d, n)
        for key, n in ref["deck_led_note"].items():
            exp[p + key + "-led"] = note(d, n)
        for key in ref["deck_input_with_led"]:
            exp[p + key + "-led"] = note(d, ref["deck_note"][key])
        for key, (msb, lsb) in ref["deck_cc14"].items():
            exp[p + key] = cc14(d, msb, lsb)
        exp[p + "level-meter"] = cc(d, ref["channel_meter_cc"])
        exp[p + "load"] = note(6, ref["load_notes"][d])
        exp[p + "jog"] = cc(d, ref["jog"]["platter_vinyl_on_cc"])
        for slot in range(ref["hot_cue_slots"]):
            pad = f"{p}hot-cue-{slot + 1}"
            exp[pad] = note(7 + 2 * d, slot)
            exp[pad + "-led"] = note(7 + 2 * d, slot)
            exp[pad + "-delete"] = note(8 + 2 * d, slot)
            exp[pad + "-delete-led"] = note(8 + 2 * d, slot)
    for key, n in ref["mixer_note"].items():
        exp[key] = note(6, n)
    for key, (msb, lsb) in ref["mixer_cc14"].items():
        exp[key] = cc14(6, msb, lsb)
    for key, n in ref["browser_cc"].items():
        exp[key] = cc(6, n)
    for key, n in ref["browser_note"].items():
        exp[key] = note(6, n)
    return exp


def audit(profile, ref):
    errors = []
    exp = expected_sources(ref)
    seen = {}
    for m in profile["mappings"]:
        seen[m["id"]] = m
        if m["port"] != ref["port"]:
            errors.append(f"{m['id']}: port {m['port']!r}")
        want = exp.get(m["id"])
        if want is None:
            errors.append(f"{m['id']}: not in the reference")
        elif m["source"] != want:
            errors.append(f"{m['id']}: source {m['source']} != reference {want}")
    for id_ in exp:
        if id_ not in seen:
            errors.append(f"{id_}: in the reference but missing from the profile")
    jog = ref["jog"]
    for d, name in enumerate("abcd"):
        config = seen.get(f"deck_{name}-jog", {}).get("binding", {}).get("config")
        if not config:
            continue
        if config["touch"] != note(d, jog["touch_note"]):
            errors.append(f"deck_{name}-jog: touch {config['touch']}")
        if config["alternate_motion"] != [cc(d, jog["rim_cc"]), cc(d, jog["platter_vinyl_off_cc"])]:
            errors.append(f"deck_{name}-jog: alternate motion {config['alternate_motion']}")
    # A button and its LED may share an address; two inputs may not.
    inputs = defaultdict(list)
    for m in profile["mappings"]:
        if m["binding"]["type"] == "led":
            continue
        s = m["source"]
        key = (s["kind"], s["channel"], s.get("number", s.get("msb")))
        inputs[key].append(m["id"])
    for key, ids in inputs.items():
        if len(ids) > 1:
            errors.append(f"ambiguous input {key}: {ids}")
    return errors


def main():
    errors = audit(json.loads(PROFILE.read_text()), json.loads(REFERENCE.read_text()))
    for error in errors:
        print(error)
    if errors:
        return 1
    print(f"DDJ-1000 profile matches the reference ({len(expected_sources(json.loads(REFERENCE.read_text())))} addresses)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
