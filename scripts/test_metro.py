import unittest

from metro import BOUNDS, CITIES, KOREA_REVISION, WORLD_REVISION, report_text, validate, validate_report_set


DIGEST = "a" * 64
MACHINE = {
    "cpu": "Example CPU", "os": "Example OS", "compiler": "rustc -Vv",
    "git_sha": "deadbeef", "git_dirty": False, "rustflags": "default",
    "os_cpu_count": 8, "ram_bytes": 16_000_000_000,
}


def fixture(city="sejong"):
    q, reps, threads = 8, 2, 2
    base = {"city": city, "queries": q, "repeats": reps, "threads": threads, "seed": 20261123}
    if city in ("london-large", "new_york-large", "tokyo-large"):
        source = {"dataset": "https://huggingface.co/datasets/HBNU-NLP/InstructOSM", "revision": WORLD_REVISION,
                  "city": city.removesuffix("-large"), "scale": "large", "native_units": "seconds",
                  "output_sha256": DIGEST, "node_count": 10, "arc_count": 12}
    else:
        source = {"dataset": "CAMUS-LAB/data-road_network_eta", "revision": KOREA_REVISION,
                  "region": city, "weight_units": "seconds", "output_sha256": DIGEST, "node_count": 10, "arc_count": 12}
    methods = {"dijkstra", "bidirectional"} | {f"{model}-{u}" for model in ("Graph", "Hypergraph") for u in BOUNDS}
    records = []
    for scale in (0, 1000):
        records.append({**base, "kind": "graph", "scale": scale, "metric": "native seconds" if scale == 0 else "max(1, round(scale * seconds))",
                        "nodes": 10, "arcs": 9, "certified_trees": 3, "certified_queries": q, "pairs": [[i % 10, (i + 1) % 10] for i in range(q)],
                        "reference": [1.0 if scale == 0 else 1000.0] * q, "reverse_s": .1, "source": source.copy(),
                        "binary_sha256": DIGEST, "machine": MACHINE.copy()})
        expected = methods | ({"fast_paths-CH"} if scale == 1000 else set())
        for j, method in enumerate(sorted(expected)):
            ch = method == "fast_paths-CH"
            if not ch and method not in ("dijkstra", "bidirectional"):
                records.append({"kind": "build", "city": city, "scale": scale, "method": method,
                                "serial_s": .1, "parallel_s": .08, "serial_trials_s": [.1] * reps,
                                "parallel_trials_s": [.08] * reps, "parity_queries": q, "wrong": 0})
            elif ch:
                records.append({"kind": "build", "city": city, "scale": scale, "method": method, "serial_s": .1})
            records.append({"kind": "latency", "city": city, "scale": scale, "method": method,
                            "queries": q, "repeats": reps, "mean_us": j + 1.0, "p50_us": j + 1.0,
                            "p95_us": j + 2.0, "trial_mean_us": [j + 1.0] * reps, "wrong": 0,
                            **({"paired_ratio_to_ch": 1.2} if scale == 1000 else {})})
            for workers in (1, threads):
                if workers not in (1, threads):
                    continue
                records.append({"kind": "throughput", "city": city, "scale": scale, "method": method,
                                "threads": workers, "queries": q, "repeats": reps, "median_qps": 10.0 + workers,
                                "trial_seconds": [.8, .8], "wrong": 0})
    return records


class ValidationTests(unittest.TestCase):
    def test_actual_rust_population_without_baseline_build_rows(self):
        records = fixture()
        self.assertEqual(validate(records, "sejong"), records)
        self.assertFalse(any(r.get("kind") == "build" and r.get("method") in ("dijkstra", "bidirectional") for r in records))

    def test_rejects_result_mutations(self):
        cases = []
        rows = fixture(); rows.pop(); cases.append(("missing result", rows))
        rows = fixture(); next(r for r in rows if r.get("kind") == "build")["method"] = "unknown"; cases.append(("missing build method", rows))
        rows = fixture(); rows = [r for r in rows if not (r.get("kind") == "latency" and r.get("scale") == 0)]; cases.append(("missing latency", rows))
        rows = fixture(); rows = [r for r in rows if not (r.get("kind") == "throughput" and r.get("scale") == 1000)]; cases.append(("missing throughput", rows))
        rows = fixture(); next(r for r in rows if r["kind"] == "latency")["wrong"] = 1; cases.append(("wrong answer", rows))
        rows = fixture(); rows[0]["queries"] = 0; cases.append(("zero query", rows))
        rows = fixture(); rows[0]["source"]["output_sha256"] = "wrong"; cases.append(("hash mismatch", rows))
        rows = fixture(); rows[0]["pairs"][0][0] = 10; cases.append(("bad endpoint", rows))
        rows = fixture(); rows[0]["reference"][0] = float("nan"); cases.append(("nan reference", rows))
        rows = fixture(); next(r for r in rows if r["kind"] == "graph")["source"]["revision"] = "main"; cases.append(("unpinned source", rows))
        for label, rows in cases:
            with self.subTest(label=label), self.assertRaises(ValueError):
                validate(rows, "sejong")
        with self.assertRaises(ValueError):
            validate([], "sejong")

    def test_rejects_cross_metric_pair_mismatch(self):
        rows = fixture()
        graph = [r for r in rows if r["kind"] == "graph"]
        graph[1]["pairs"][0] = [2, 3]
        with self.assertRaises(ValueError):
            validate(rows, "sejong")

    def test_international_source_pin_and_identity(self):
        rows = fixture("london-large")
        source = {"dataset": "https://huggingface.co/datasets/HBNU-NLP/InstructOSM", "revision": WORLD_REVISION,
                  "city": "london", "scale": "large", "native_units": "seconds", "output_sha256": DIGEST, "node_count": 10, "arc_count": 12}
        for row in rows:
            if row["kind"] == "graph":
                row["source"] = source.copy()
        validate(rows, "london-large")


class ReportTests(unittest.TestCase):
    def data(self):
        return {city: fixture(city) for city in CITIES}

    def test_requires_full_inventory_and_clean_matching_runs(self):
        results = self.data()
        results.pop("sejong")
        with self.assertRaises(ValueError):
            validate_report_set(results)
        results = self.data()
        next(r for r in results["seoul"] if r["kind"] == "graph")["machine"]["rustflags"] = "-C opt-level=3"
        with self.assertRaises(ValueError):
            validate_report_set(results)
        results = self.data()
        next(r for r in results["seoul"] if r["kind"] == "graph")["machine"]["git_dirty"] = True
        with self.assertRaises(ValueError):
            validate_report_set(results)

    def test_report_separates_weight_metrics_and_keeps_losses(self):
        text = report_text(self.data())
        self.assertIn("## Native weights", text)
        self.assertIn("## Rounded weights", text)
        self.assertIn("fast_paths-CH", text)
        self.assertIn("## Throughput scaling", text)
        self.assertIn("## Machine and build provenance", text)


if __name__ == "__main__":
    unittest.main()
