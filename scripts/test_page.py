"""Page evidence excludes old objectives and checks retained cost and partial boundaries."""

import copy
import json
import tempfile
import unittest
from pathlib import Path

import page
import scale


class PageTests(unittest.TestCase):
    def test_page_data_is_current_and_has_no_archived_tracks(self):
        payload = page.build_payload()
        self.assertEqual((page.ROOT / "site/data.js").read_text(encoding="utf-8"), page.javascript(payload))
        self.assertEqual(set(payload), {"schema", "labels", "scale", "traffic", "industry"})
        self.assertTrue(payload["traffic"]["regions"])
        self.assertEqual(json.loads(page.javascript(payload)[len("window.DATA = "):-2]), payload)

    def test_missing_scale_is_pending_without_borrowing_traffic_timings(self):
        folder = page.ROOT / "data/page-tests"
        folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=folder) as name:
            payload = page.build_payload(Path(name))
        self.assertIsNone(payload["scale"])
        self.assertIsNone(payload["traffic"])

    def test_legacy_mixed_answer_timings_do_not_enter_the_ranking(self):
        folder = page.ROOT / "data/page-tests"
        folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=folder) as name:
            path = Path(name) / "scale.jsonl"
            path.write_text(json.dumps({"kind": "source", "size": "sejong", "nodes": 10,
                                        "timing_contract": "mixed distance/path"}) + "\n", encoding="utf-8")
            payload = page.scale_payload(path)
        self.assertEqual(payload["sizes"], [])
        self.assertIn("sejong", payload["rejected"])
        self.assertIn("union", payload["missing"])

    @unittest.skipUnless((page.ROOT / "results/scale.jsonl").exists(), "complete path benchmark not retained")
    def test_committed_boundaries_recompute_and_a_changed_number_fails(self):
        path = page.ROOT / "results/scale.jsonl"
        payload = page.scale_payload(path)
        self.assertTrue(payload["sizes"])
        rows, _ = page.read_rows(path)
        for s in payload["sizes"]:
            eng = {r["method"]: r for r in rows if r.get("size") == s["id"] and r["kind"] == "engine"}
            first, switches, k = scale.kstar(eng)
            self.assertEqual((first, k), (s["static"], s["k"]))
            self.assertEqual([e for _, e in switches], [x["to"] for x in s["switches"]])
        bad = copy.deepcopy(rows)
        next(r for r in bad if r["kind"] == "breakeven" and r["requests"] == 10**6)["k_cch"] = -1
        folder = page.ROOT / "data/page-tests"
        folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=folder) as name:
            changed = Path(name) / "scale.jsonl"
            changed.write_text("".join(json.dumps(r) + "\n" for r in bad), encoding="utf-8")
            with self.assertRaises(ValueError):
                page.scale_payload(changed)


if __name__ == "__main__":
    unittest.main()
