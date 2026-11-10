import copy
import json
import unittest
from pathlib import Path

from audit_controller_profile import ROOT, REFERENCE, audit


class ControllerProfileAuditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.reference = json.loads(REFERENCE.read_text(encoding="utf-8"))
        cls.profile = json.loads(
            (ROOT / cls.reference["profile"]).read_text(encoding="utf-8")
        )

    def test_bundled_grv6_matches_document(self):
        errors, documented, inputs = audit(self.profile, self.reference)
        self.assertEqual(errors, [])
        self.assertEqual(documented, 1321)
        self.assertEqual(inputs, 715)

    def test_swapped_mixer_control_is_detected(self):
        profile = copy.deepcopy(self.profile)
        master = next(m for m in profile["mappings"] if m["id"] == "mixer-master")
        master["source"] = {"kind": "cc14", "channel": 6, "msb": 9, "lsb": 41}
        errors, _, _ = audit(profile, self.reference)
        self.assertTrue(any("mixer-master" in error for error in errors))

    def test_ambiguous_input_is_detected(self):
        profile = copy.deepcopy(self.profile)
        cue = next(m for m in profile["mappings"] if m["id"] == "deck_a-cue")
        cue["source"] = {"kind": "note", "channel": 0, "number": 11}
        errors, _, _ = audit(profile, self.reference)
        self.assertTrue(any("ambiguous input address" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
