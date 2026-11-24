import hashlib
import json
import copy
import math
import unittest
from pathlib import Path

from metro import BOUNDS, CITIES, load_cities, validate_report_set
from insights import build_payload, javascript, summarize, validate_order_experiment, workload
from test_metro import fixture


TEST_ROOT = Path(__file__).resolve().parents[1] / "target" / "insights-test"


def frozen_fixture(city):
    records = fixture(city)
    q = 1000
    pairs = [[i % 10, i % 10 if i % 10 == 0 else (i + 1) % 10] for i in range(q)]
    seconds = [None if i % 101 == 0 else (0.0 if pairs[i][0] == pairs[i][1] else 10.0 + i % 5) for i in range(q)]
    for row in records:
        scale = row["scale"]
        if row["kind"] == "graph":
            row.update(queries=q, certified_queries=q, pairs=pairs,
                       reference=[None if d is None else d * (1000 if scale else 1) for d in seconds])
        elif row["kind"] == "build" and row["method"] != "fast_paths-CH":
            bound = int(row["method"].split("-")[1])
            hyper = row["method"].startswith("Hypergraph")
            row.update(parity_queries=q, boundary=50 if hyper else 100, cells=10,
                       arcs=100 if hyper else 200, array_bytes=8000)
        elif row["kind"] == "build":
            row.update(arcs=99)
        elif row["kind"] == "latency":
            method = row["method"]
            row["queries"] = q
            row["mean_us"] = 500.0 if method.startswith("Hypergraph") else 1000.0
            row["p50_us"] = 50.0 if method.startswith("Hypergraph") else 100.0
            row["p95_us"] = 100.0 if method.startswith("Hypergraph") else 200.0
            row["settled"] = 600 if method.startswith("Hypergraph") else 1000
            row["scanned"] = 1200 if method.startswith("Hypergraph") else 2000
            row["walk8_p50_us"], row["walk32_p50_us"], row["uniform_p50_us"] = 1.0, 2.0, 3.0
        elif row["kind"] == "throughput":
            row["queries"] = q
    return records


class InsightTests(unittest.TestCase):
    def query_order_fixture(self):
        experiments = []
        methods = ("dijkstra", "Hypergraph-1024")
        work = [[i % 7, i % 13] for i in range(1000)]
        for city in ("sejong", "london-large"):
            graph = next(row for row in fixture(city) if row["kind"] == "graph" and row["scale"] == 0)
            rows = []
            for condition in ("fixed", "reversed", "precleared"):
                for method in methods:
                    rows.append({
                        "condition": condition, "method": method, "wrong": 0,
                        "mean_query_us": 10.0, "mean_preparation_us": 0.0,
                        "groups": [
                            {"category": "walk8", "count": 250, "median_query_us": 1.0},
                            {"category": "walk32", "count": 250, "median_query_us": 2.0},
                            {"category": "uniform", "count": 500, "median_query_us": 3.0},
                        ],
                        "per_query_mean_us": [1.0] * 1000,
                        "per_query_preparation_us": [0.0] * 1000,
                        "per_query_previous_touched": [0] * 1000,
                        "per_query_work": copy.deepcopy(work),
                    })
            experiments.append({
                "city": city, "scale": 0, "bound": 1024, "queries": 1000, "repeats": 3,
                "same_current_query_work": True, "source_binary_sha256": graph["binary_sha256"],
                "source_benchmark_sha": "producer-sha", "results": rows,
            })
        return {"schema": 1, "provenance": {"frozen_results_sha256": "frozen-sha"}, "experiments": experiments}, {
            city: [row for row in fixture(city) if row["kind"] == "graph" and row["scale"] == 0]
            for city in ("sejong", "london-large")
        }

    def test_query_order_diagnostic_requires_registered_complete_population(self):
        experiment, results = self.query_order_fixture()
        self.assertIs(validate_order_experiment(experiment, results, "frozen-sha", "producer-sha"), experiment)
        mutations = []
        bad = copy.deepcopy(experiment); bad["provenance"]["frozen_results_sha256"] = "other"; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["source_benchmark_sha"] = "other"; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["source_binary_sha256"] = "other"; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["results"].pop(); mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["results"][0]["wrong"] = 1; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["same_current_query_work"] = False; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["results"][0]["mean_query_us"] = math.nan; mutations.append(bad)
        bad = copy.deepcopy(experiment); bad["experiments"][0]["results"][1]["per_query_work"][0][0] += 1; mutations.append(bad)
        for bad in mutations:
            with self.subTest(mutation=len(mutations)), self.assertRaises(ValueError):
                validate_order_experiment(bad, results, "frozen-sha", "producer-sha")

    def test_workload_groups_and_presets(self):
        graph = next(row for row in frozen_fixture("sejong") if row["kind"] == "graph" and row["scale"] == 1000)
        result = workload(graph, 1000)
        groups = {group["id"]: group for group in result["groups"]}
        self.assertEqual([groups[name]["count"] for name in ("walk8", "walk32", "uniform")], [250, 250, 500])
        self.assertEqual([p["index"] for p in result["presets"]], [0, 1, 2, 3, 125, 126, 501, 502, 750, 999])
        self.assertEqual(result["presets"][0]["category"], "walk8")
        self.assertIsNone(groups["walk8"]["distances_seconds"][0])
        self.assertTrue(all(value is None or 0 <= value < 20 for group in groups.values() for value in group["distances_seconds"]))

    def test_summary_arithmetic(self):
        rows = [{"bound": bound, "boundary_reduction": .5, "mean_speedup": 2.0,
                 "p50_speedup": 1.5} for bound in BOUNDS for _ in range(11)]
        result = summarize(rows)
        self.assertEqual(result["count"], 33)
        self.assertEqual(result["total"], 33)
        self.assertEqual(result["mean_wins"], 33)
        self.assertEqual(result["p50_wins"], 33)
        self.assertEqual(result["median_boundary_reduction"], .5)
        self.assertEqual([r["count"] for r in result["by_bound"]], [11, 11, 11])

    def test_payload_from_full_frozen_population_and_canonical_js(self):
        TEST_ROOT.mkdir(parents=True, exist_ok=True)
        source = TEST_ROOT / "metro.jsonl"
        orderfile = TEST_ROOT / "query-order.json"
        try:
            rows = [row for city in sorted(CITIES) for row in frozen_fixture(city)]
            raw = "".join(json.dumps(row, allow_nan=False) + "\n" for row in rows).encode("utf-8")
            source.write_bytes(raw)
            payload = build_payload(source)
            self.assertEqual(payload["source_sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual(payload["schema"], 1)
            self.assertEqual(len(payload["cities"]), 11)
            city = next(row for row in payload["cities"] if row["id"] == "sejong")
            self.assertEqual(city["metrics"]["0"]["comparisons"][0]["mean_speedup"], 2.0)
            self.assertEqual(payload["summary"]["0"]["mean_wins"], 33)
            script = javascript(payload)
            self.assertTrue(script.startswith("window.ROUTING_INSIGHTS = {"))
            self.assertTrue(script.endswith(";\n"))
            self.assertEqual(json.loads(script[len("window.ROUTING_INSIGHTS = "):-2]), payload)
            self.assertIsNone(payload["order_experiment"])
            order, _ = self.query_order_fixture()
            order["provenance"]["frozen_results_sha256"] = hashlib.sha256(raw).hexdigest()
            _q, _r, _t, _seed, producer_sha, _flags = validate_report_set(load_cities(source))
            for experiment in order["experiments"]:
                experiment["source_benchmark_sha"] = producer_sha
            orderfile.write_text(json.dumps(order), encoding="utf-8")
            with_order = build_payload(source, orderfile)
            self.assertEqual(with_order["order_experiment"], order)
            self.assertEqual(with_order["summary"], payload["summary"])

            missing = [row for row in rows if row["city"] != "seoul"]
            source.write_text("".join(json.dumps(row) + "\n" for row in missing), encoding="utf-8")
            with self.assertRaises(ValueError):
                build_payload(source)
        finally:
            if source.exists():
                source.unlink()
            if orderfile.exists():
                orderfile.unlink()
            try:
                TEST_ROOT.rmdir()
            except OSError:
                pass


if __name__ == "__main__":
    unittest.main()
