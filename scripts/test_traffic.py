"""Break-even envelope, regret invariants and committed traffic rows."""

import json
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np

import traffic

RESULTS = Path(__file__).resolve().parents[1] / "results" / "traffic.jsonl"


def raw(costs, pairs, repeats=1):
    """Minimal raw rows: a graph row with the given cost matrices and a complete timing population."""
    k = len(traffic.METRICS)
    q = len(pairs)
    rows = [{"kind": "graph", "metrics": k, "queries": q, "repeats": repeats, "certified_trees": q * k,
             "pairs": pairs, "costs": costs, "region": "sejong"}]
    rows += [{"kind": "prepare", "method": m, "seconds": 1.0, "edges": 1, "tree_height": 1}
             for m in ("overlay-1024", "overlay-4096", "fast_paths-CH-fixed-order", "CCH")]
    for e in traffic.ENGINES:
        for p in range(repeats):
            for s in range(len(traffic.SLOTS)):
                mine = [i for i in range(q) if (i + p) % len(traffic.SLOTS) == s]
                rows.append({"kind": "slot", "method": e, "pass": p, "slot": s, "update_s": 0.5,
                             "queries": len(mine), "query_s": 1e-3 * len(mine), "checked": q, "wrong": 0})
    return rows


class Envelope(unittest.TestCase):
    def test_switches_follow_the_lower_envelope(self):
        lines = {"a": (10.0, 0.0), "b": (1.0, 1.0), "c": (4.0, 0.25)}
        # b is cheapest at K=0; c overtakes at K=4; a at K=24.
        self.assertEqual(traffic.envelope(lines), [(4.0, "c"), (24.0, "a")])
        for k in (0.0, 3.9, 4.1, 23.9, 24.1, 1e6):
            best = min(lines, key=lambda e: lines[e][0] + lines[e][1] * k)
            current = "b"
            for at, e in traffic.envelope(lines):
                if k > at:
                    current = e
            self.assertEqual(current, best, k)

    def test_dominated_engine_never_appears(self):
        lines = {"a": (1.0, 0.0), "b": (2.0, 1.0)}
        self.assertEqual(traffic.envelope(lines), [])

    def test_breakeven_uses_daily_core_hours(self):
        eng = {"x": {"q": 100.0, "u": 0.0}, "y": {"q": 10.0, "u": 36.0}}
        first, switches = traffic.breakeven(eng, 10**6)
        # Core-hours per day at R = 1e6: x costs 0.1, y costs 0.01 + 0.01 K, so K* = 9.
        self.assertEqual(first, "y")
        self.assertEqual([e for _, e in switches], ["x"])
        self.assertAlmostEqual(switches[0][0], 9.0)


class Regret(unittest.TestCase):
    def matrix(self, scale):
        k = len(traffic.METRICS)
        c = np.full((k, k), 100.0)
        c[0, 1] = 110.0 * scale  # the free-flow path costs 110 on am_peak
        return list((c).ravel())

    def test_zero_on_the_diagonal_and_positive_when_stale(self):
        k = len(traffic.METRICS)
        same = list(np.full(k * k, 100.0))
        rows = raw([same, self.matrix(1.0), None, list(np.zeros(k * k))], [[0, 1], [2, 3], [4, 5], [6, 6]])
        out = {(r["from"], r["to"], r["group"]): r for r in traffic.regret(rows)}
        self.assertEqual(out["free", "am_peak", "all"]["pairs"], 2)
        self.assertAlmostEqual(out["free", "am_peak", "all"]["mean"], 0.05)
        self.assertEqual(out["free", "am_peak", "all"]["share_changed"], 0.5)
        self.assertEqual(out["am_peak", "am_shoulder", "all"]["mean"], 0.0)
        self.assertEqual([(r["from"], r["to"]) for r in traffic.regret(rows)
                          if r["series"] == "observed" and r["group"] == "all"],
                         [("am_peak", "midday"), ("midday", "pm_peak"), ("pm_peak", "offpeak"), ("offpeak", "am_peak")])

    def test_validate_rejects_a_stale_path_below_the_optimum(self):
        rows = raw([self.matrix(0.5)], [[0, 1]])
        with self.assertRaises(ValueError):
            traffic.validate(rows)
        traffic.validate(raw([self.matrix(1.0)], [[0, 1]]))

    def test_validate_rejects_a_missing_slot(self):
        rows = raw([self.matrix(1.0)], [[0, 1]])
        with self.assertRaises(ValueError):
            traffic.validate([r for r in rows if not (r["kind"] == "slot" and r["slot"] == 3)])

    def test_summarize_keeps_only_source_and_all_request_regret(self):
        rows = raw([self.matrix(1.0)], [[0, 1]])
        rows[0].update(nodes=2, arcs=1, reachable=1, identical_endpoints=0,
                       reference_s=0.1, source={}, machine={})
        data = "".join(json.dumps(r) + "\n" for r in rows).encode()
        with patch.object(Path, "read_bytes", return_value=data):
            out = traffic.summarize(Path("fixture.jsonl"))
        self.assertEqual([r["kind"] for r in out], ["source"] + ["regret"] * 20)
        self.assertEqual([r for r in out if r["kind"] == "regret"],
                         [{"kind": "regret", "region": "sejong", **r} for r in traffic.regret(rows)])
        self.assertTrue(all(r["group"] == "all" for r in out[1:]))
        am_peak = next(r for r in out[1:] if r["from"] == "free" and r["to"] == "am_peak")
        self.assertAlmostEqual(am_peak["mean"], 0.1)


@unittest.skipUnless(RESULTS.exists(), "no committed traffic results")
class Committed(unittest.TestCase):
    def setUp(self):
        self.rows = [json.loads(line) for line in RESULTS.read_text(encoding="utf-8").splitlines()]

    def test_nine_regions_have_only_sources_and_all_request_regret(self):
        regions = {r["region"] for r in self.rows if r["kind"] == "source"}
        self.assertEqual(regions, set(traffic.REGIONS))
        self.assertEqual({r["kind"] for r in self.rows}, {"source", "regret"})
        for region in regions:
            source = [r for r in self.rows if r["kind"] == "source" and r["region"] == region]
            self.assertEqual(len(source), 1)
            self.assertEqual(source[0]["repeats"], 1 if region == "korea" else 3)
            self.assertEqual(source[0]["queries"], 100 if region == "korea" else 1000)
            rows = [r for r in self.rows if r["kind"] == "regret" and r["region"] == region]
            self.assertEqual(len({(r["series"], r["from"], r["to"]) for r in rows}), 20)
            self.assertEqual(len(rows), 20)
            self.assertTrue(all(r["group"] == "all" and r["pairs"] > 0 for r in rows))

    def test_regret_is_nonnegative_and_rows_carry_provenance(self):
        for r in self.rows:
            if r["kind"] == "regret":
                self.assertGreaterEqual(r["mean"], 0.0)
                self.assertGreaterEqual(r["share_ge5"], 0.0)
                self.assertLessEqual(r["share_ge5"], r["share_changed"])
            if r["kind"] == "source":
                self.assertEqual(r["certified_trees"], r["queries"] * len(traffic.METRICS))
                self.assertLessEqual(r["reachable"], r["queries"])
                self.assertRegex(r["raw_sha256"], r"^[0-9a-f]{64}$")
                src = r["source"]
                self.assertEqual(src["licence"], "CC BY-NC 4.0")
                for key in ("url", "revision", "nodes_sha256", "edges_sha256", "output_sha256", "metrics_sha256"):
                    self.assertTrue(src[key])


if __name__ == "__main__":
    unittest.main()
