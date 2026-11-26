"""Growth fits, break-even, partial-update crossover, shard validation, the union cut and the
committed scale rows."""

import json
import unittest
from pathlib import Path

import numpy as np
import pandas as pd

import scale
import traffic

RESULTS = Path(__file__).resolve().parents[1] / "results" / "scale.jsonl"


def raw(q=8, drop=None):
    """Minimal rows of one size: one time shard, the partial study and the memory pass."""
    rows = [{"kind": "graph", "mode": "time", "nodes": 10, "queries": q, "reachable": [q] * 8, "size": "sejong"},
            {"kind": "graph", "mode": "partial", "nodes": 10, "queries": q, "reachable": [q] * 16}]
    rows += [{"kind": "prepare", "method": m, "seconds": 1.0} for m in scale.PREPARED]
    for e in scale.ENGINES:
        for p in range(scale.PASSES):
            for s in range(scale.SLOT_COUNT):
                if (e, p, s) == drop:
                    continue
                mine = [i for i in range(q) if (i + p) % scale.SLOT_COUNT == s]
                rows.append({"kind": "slot", "method": e, "pass": p, "slot": s, "update_s": 0.5, "update_wall_s": 0.5,
                             "queries": len(mine), "query_s": 1e-3 * len(mine), "checked": q, "wrong": 0})
    for p in range(scale.PASSES):
        for seq, n in (("weekday-consecutive", 8), ("weekday-observed", 4), ("weekend-consecutive", 8),
                       ("weekend-observed", 4), ("synthetic", 4 * len(scale.F))):
            rows += [{"kind": "update", "pass": p, "sequence": seq, "identical": True, "wrong": 0} for _ in range(n)]
    rows += [{"kind": "memory", "method": m} for m in ("none", *scale.ENGINES)]
    return rows


class Fits(unittest.TestCase):
    def test_slope_recovers_an_exact_power_law(self):
        n = [1e5, 3e5, 1e6, 3e6, 9e6]
        a, c, se = scale.fit(n, [2.0 * x ** 0.7 for x in n])
        self.assertAlmostEqual(a, 0.7)
        self.assertAlmostEqual(np.exp(c), 2.0)
        self.assertLess(se, 1e-9)

    def test_curve_averages_each_f_and_the_dataset_transitions(self):
        ups = [{"sequence": "synthetic", "target_f": f, "f": f, "changed": 1, "ratio": r, "partial_pass": [r] * 3,
                "full_pass": [1.0] * 3} for f in scale.F for r in (0.5 * f / 0.01, 1.5 * f / 0.01)]
        ups += [{"sequence": "weekday-consecutive", "target_f": None, "f": f, "changed": c, "ratio": 3.0,
                 "partial_pass": [3.0] * 3, "full_pass": [1.0] * 3} for f, c in ((0.98, 1), (0.99, 1), (0.0, 0))]
        f, r = scale.curve(ups)
        self.assertEqual(f[:5], list(scale.F))
        self.assertAlmostEqual(f[5], 0.985)
        self.assertEqual(r[1], 1.0)
        self.assertAlmostEqual(scale.crossing(f, r), 0.01)

    def test_crossing_interpolates_in_log_f(self):
        # Ratio 0.5 at f = 1% and 2 at f = 10%: the crossing is a third of the way in log f.
        f = scale.crossing([0.001, 0.01, 0.1], [0.2, 0.5, 2.0])
        self.assertAlmostEqual(np.log(f), np.log(0.01) + (0.5 / 1.5) * np.log(10.0))
        self.assertEqual(scale.crossing([0.01, 0.1], [1.5, 3.0]), 0.0)
        self.assertIsNone(scale.crossing([0.01, 0.1], [0.2, 0.5]))


class Breakeven(unittest.TestCase):
    def test_kstar_is_the_switch_into_cch_among_the_base_engines(self):
        eng = {e: {"q": 100.0, "u": 0.0} for e in scale.ENGINES}
        eng["fast_paths-CH"] = {"q": 5.0, "u": 3.6}
        eng["CCH"] = {"q": 10.0, "u": 0.036}
        eng["CCH-perfect"] = {"q": 4.0, "u": 0.36}
        first, switches, k = scale.kstar(eng)
        # Base engines: CH costs 0.005 + 0.001 K, CCH 0.01 + 0.00001 K: K* = 0.005 / 0.00099.
        self.assertAlmostEqual(k, 0.005 / 0.00099)
        # All engines: CCH-perfect at K = 0, CCH above 0.006 / 0.00009.
        self.assertEqual(first, "CCH-perfect")
        self.assertEqual([e for _, e in switches][0], "CCH")
        self.assertAlmostEqual(switches[0][0], 0.006 / 0.00009)


class Validation(unittest.TestCase):
    def test_memory_is_absolute_process_peak_even_when_baseline_pages_are_reused(self):
        rows = raw()
        next(r for r in rows if r["kind"] == "memory" and r["method"] == "dijkstra").update(
            baseline_rss_bytes=4096, peak_rss_bytes=4096)
        self.assertEqual(scale.engines(rows)["dijkstra"]["memory_bytes"], 4096)

    def test_update_cost_is_the_mean_over_passes(self):
        rows = raw()
        for row in rows:
            if row["kind"] == "slot":
                row["update_s"] = row["update_wall_s"] = (1.0, 1.0, 7.0)[row["pass"]]
        eng = scale.engines(rows)
        for values in eng.values():
            self.assertEqual(values["u_pass"], [1.0, 1.0, 7.0])
            self.assertEqual(values["u"], 3.0)
            self.assertEqual(values["u_wall"], 3.0)

    def test_shards_cannot_mix_path_and_legacy_contracts(self):
        rows = raw()
        rows.append({**rows[0], "timing_contract": "paths-reverse-update-v1"})
        with self.assertRaisesRegex(ValueError, "timing contracts"):
            scale.validate(rows)

    def test_duplicated_memory_rows_are_rejected(self):
        rows = raw()
        rows.append(next(r for r in rows if r["kind"] == "memory"))
        with self.assertRaisesRegex(ValueError, "duplicated memory"):
            scale.validate(rows)

    def test_routingkit_rejects_a_duplicate_in_place_of_a_missing_slot(self):
        rows = [{"kind": "routingkit_prepare", "commit": "pinned", "order_s": 1.0, "build_s": 2.0}]
        for p in range(scale.PASSES):
            for s in range(scale.SLOT_COUNT):
                common = {"pass": p, "slot": s, "wrong": 0, "queries": 8, "checked": 8, "query_s": 0.001}
                rows.append({"kind": "routingkit_ch_slot", "build_s": 1.0, **common})
                rows.append({"kind": "routingkit_slot", "customize_s": 0.01, "customize4_wall_s": 0.005, **common})
        rows += [{"kind": "routingkit_update", "target_f": f, "partial_s": f, "full_s": 1.0, "wrong": 0}
                 for _ in range(scale.PASSES * 4) for f in scale.F]
        self.assertEqual(len(scale.routingkit(rows)), 1)
        rows.append({"kind": "graph", "mode": "time", "queries": 8,
                     "timing_contract": "paths-reverse-update-v1"})
        with self.assertRaisesRegex(ValueError, "timing contract"):
            scale.routingkit(rows)
        for row in rows:
            if row["kind"] in ("routingkit_slot", "routingkit_ch_slot"):
                row.update(timing_contract="paths-reverse-update-v1", queries=1)
        self.assertEqual(len(scale.routingkit(rows)), 1)
        rows[1]["queries"] = 8
        with self.assertRaisesRegex(ValueError, "query population"):
            scale.routingkit(rows)
        rows[1]["queries"] = 1
        for kind in ("routingkit_ch_slot", "routingkit_slot"):
            broken = [dict(r) for r in rows]
            next(r for r in broken if r["kind"] == kind)["slot"] = 1
            with self.assertRaises(ValueError):
                scale.routingkit(broken)

    def test_complete_rows_pass_and_a_missing_slot_fails(self):
        self.assertEqual(scale.validate(raw()), (8, 1))
        with self.assertRaises(ValueError):
            scale.validate(raw(drop=("CCH-partial", 1, 3)))

    def test_a_wrong_answer_fails(self):
        rows = raw()
        rows[-20]["wrong"] = 1
        with self.assertRaises(ValueError):
            scale.validate(rows)

    def test_plan_covers_every_pass_and_slot_once_per_size(self):
        for size in scale.SIZES:
            cells = [(p, s) for x in scale.plan(size) if "time" in x["modes"]
                     for p in map(int, x["passes"].split(",")) for s in map(int, x["slots"].split(","))]
            self.assertEqual(sorted(cells), [(p, s) for p in range(3) for s in range(8)], size)
            modes = {m for x in scale.plan(size) for m in x["modes"].split(",")}
            self.assertEqual(modes, {"time", "partial", "routingkit", "routingkit-ch", "memory"}, size)

    def test_ch_resume_runs_only_missing_ch_and_preserves_the_workload(self):
        shards = scale.plan("all", "routingkit-ch")
        self.assertEqual([x["size"] for x in shards], list(scale.SIZES))
        self.assertTrue(all(x["modes"] == "routingkit-ch" for x in shards))
        self.assertTrue(all(x["passes"] == "0,1,2" for x in shards))
        self.assertTrue(all(x["queries"] == "1000" for x in shards))
        with self.assertRaises(ValueError):
            scale.plan("all", "missing")


class Union(unittest.TestCase):
    def test_induce_keeps_matched_nodes_and_edges_between_them(self):
        nodes = pd.DataFrame({"node_id": [0, 1, 2, 3], "lat": [1.0, 2.0, 3.0, 4.0], "lon": [5.0, 6.0, 7.0, 8.0]})
        edges = pd.DataFrame({"source": [0, 1, 2, 3], "target": [1, 2, 3, 0], "length_m": [1.0, 2.0, 3.0, 4.0]})
        keys = pd.DataFrame({"lat": [2.0, 3.0, 4.0, 3.0], "lon": [6.0, 7.0, 8.0, 7.0]})
        n, e = scale.induce(nodes, edges, keys)
        self.assertEqual(n.node_id.tolist(), [0, 1, 2])
        self.assertEqual(n.lat.tolist(), [2.0, 3.0, 4.0])
        self.assertEqual(list(zip(e.source, e.target)), [(0, 1), (1, 2)])
        self.assertEqual(e.length_m.tolist(), [2.0, 3.0])


@unittest.skipUnless(RESULTS.exists(), "no committed scale results")
class Committed(unittest.TestCase):
    def setUp(self):
        self.rows = [json.loads(line) for line in RESULTS.read_text(encoding="utf-8").splitlines()]

    def test_every_size_is_present(self):
        self.assertEqual([r["size"] for r in self.rows if r["kind"] == "source"], list(scale.SIZES))

    def test_kstar_recomputes_from_engine_rows(self):
        for b in (r for r in self.rows if r["kind"] == "breakeven"):
            eng = {r["method"]: r for r in self.rows if r["kind"] == "engine" and r["size"] == b["size"]}
            first, switches, k = scale.kstar(eng, b["requests"])
            self.assertEqual(first, b["cheapest_static"])
            self.assertEqual([e for _, e in switches], [s["to"] for s in b["switches"]])
            if k is None:
                self.assertIsNone(b["k_cch"])
            else:
                self.assertAlmostEqual(k, b["k_cch"], delta=1e-9 * max(1.0, k))

    def test_growth_exponents_recompute_from_engine_rows(self):
        src = [r for r in self.rows if r["kind"] == "source"]
        eng = {(r["size"], r["method"]): r for r in self.rows if r["kind"] == "engine"}
        for g in (r for r in self.rows if r["kind"] == "growth" and r["method"] != "K*"):
            for key, name in (("q", "a"), ("u", "b")):
                if name in g:
                    a = scale.fit([s["nodes"] for s in src], [eng[s["size"], g["method"]][key] for s in src])[0]
                    self.assertAlmostEqual(a, g[name], places=9)

    def test_partial_crossover_recomputes_from_update_rows(self):
        for p in (r for r in self.rows if r["kind"] == "partial"):
            ups = [r for r in self.rows if r["kind"] == "update" and r["size"] == p["size"]]
            for u in ups:
                self.assertAlmostEqual(u["ratio"], np.mean(u["partial_pass"]) / np.mean(u["full_pass"]))
            f = scale.crossing(*scale.curve(ups))
            self.assertEqual(f is None, p["f_star"] is None)
            if f is not None:
                self.assertAlmostEqual(f, p["f_star"])

    def test_provenance_and_licence(self):
        for r in (x for x in self.rows if x["kind"] == "source"):
            self.assertEqual(r["source"]["licence"], traffic.LICENCE)
            self.assertTrue(r["source"]["files"] and all(f["sha256"] for f in r["source"]["files"]))


if __name__ == "__main__":
    unittest.main()
