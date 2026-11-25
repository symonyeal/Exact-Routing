import copy
import json
import math
import unittest

from page import ROOT, build_payload, javascript, maze_bench, validate_order_experiment
from test_metro import fixture


class PageTests(unittest.TestCase):
    def query_order_fixture(self):
        experiments = []
        work = [[i % 7, i % 13] for i in range(1000)]
        for city in ("sejong", "london-large"):
            graph = next(row for row in fixture(city) if row["kind"] == "graph" and row["scale"] == 0)
            rows = []
            for condition in ("fixed", "reversed", "precleared"):
                for method in ("dijkstra", "Hypergraph-1024"):
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
        for i, bad in enumerate(mutations):
            with self.subTest(mutation=i), self.assertRaises(ValueError):
                validate_order_experiment(bad, results, "frozen-sha", "producer-sha")

    def test_bench_table_parse(self):
        text = ("| method | exact | optimal | median µs | p95 µs | mazes/s | time / wave (95% CI) | peak heap B |\n"
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n"
                "| wave | 1000 | 1000 | 3.28 | 3.58 | 303140 | 1.00 (1.00–1.00) | 2300 |\n"
                "| queue | 1000 | 1000 | 4.11 | 4.67 | 244382 | 1.24 (1.24–1.25) | 11792 |\n\n"
                "| stage | parse |\n")
        rows = maze_bench(text)
        self.assertEqual([r["method"] for r in rows], ["wave", "queue"])
        self.assertEqual((rows[0]["median_us"], rows[0]["heap_bytes"]), (3.28, 2300))
        with self.assertRaises(ValueError):
            maze_bench(text.replace("| wave |", "| other |"))

    def test_payload_from_retained_results(self):
        payload = build_payload()
        self.assertEqual(len(payload["cities"]), 11)
        for city in payload["cities"]:
            engines = city["engines"]
            self.assertAlmostEqual(engines["dijkstra"]["cost_index"], 100.0)
            for e in engines.values():
                self.assertAlmostEqual(e["cost_index"] / 100,
                                       e["core_hours_per_billion"] / engines["dijkstra"]["core_hours_per_billion"])
        self.assertEqual(sum(payload["summary"]["cheapest_counts"].values()), 11)
        self.assertTrue(all(0 < o["carryover_share"] < 1 for o in payload["order"]))
        self.assertTrue(all(0 < r["boundary"]["median"] < 1 for r in payload["cuts"]))
        script = javascript(payload)
        self.assertEqual(json.loads(script[len("window.DATA = "):-2]), payload)

    def test_published_page_data_is_current(self):
        self.assertEqual((ROOT / "site" / "data.js").read_text(encoding="utf-8"), javascript(build_payload()))


if __name__ == "__main__":
    unittest.main()
