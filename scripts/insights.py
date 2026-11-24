"""Derive descriptive partition and workload insights from frozen metro results."""

import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

from metro import BOUNDS, CITIES, load_cities, validate_report_set
from serve_maps import LABELS

PRESETS = (0, 1, 2, 3, 125, 126, 501, 502, 750, 999)
GROUPS = {"walk8": lambda i: i % 4 == 0, "walk32": lambda i: i % 4 == 1,
          "uniform": lambda i: i % 4 >= 2}


def percentile(values, p):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[max(0, math.ceil(p * len(ordered)) - 1)]


def workload(graph, scale):
    pairs, reference = graph["pairs"], graph["reference"]
    factor = 1.0 if scale == 0 else 1000.0
    groups = []
    for label, includes in GROUPS.items():
        indices = [i for i in range(len(pairs)) if includes(i)]
        distances = [None if reference[i] is None else reference[i] / factor for i in indices]
        reachable = [value for value in distances if value is not None]
        groups.append({
            "id": label,
            "count": len(indices),
            "reachable": len(reachable),
            "identical": sum(pairs[i][0] == pairs[i][1] for i in indices),
            "median_seconds": statistics.median(reachable) if reachable else None,
            "p95_seconds": percentile(reachable, .95),
            "distances_seconds": distances,
        })
    presets = []
    for index in PRESETS:
        source, target = pairs[index]
        distance = reference[index]
        presets.append({
            "index": index,
            "source": source,
            "target": target,
            "category": next(label for label, includes in GROUPS.items() if includes(index)),
            "distance_seconds": None if distance is None else distance / factor,
        })
    return {"groups": groups, "presets": presets}


def _latency_row(rows, method):
    return next(row for row in rows if row["method"] == method)


def city_insight(city, records):
    graph_rows = {row["scale"]: row for row in records if row["kind"] == "graph"}
    metrics = {}
    comparisons_by_scale = {}
    for scale in (0, 1000):
        builds = {row["method"]: row for row in records if row["kind"] == "build" and row["scale"] == scale}
        latencies = [row for row in records if row["kind"] == "latency" and row["scale"] == scale]
        comparisons, ratios = [], []
        for bound in BOUNDS:
            graph_method, hyper_method = f"Graph-{bound}", f"Hypergraph-{bound}"
            gb, hb = builds[graph_method], builds[hyper_method]
            gl, hl = _latency_row(latencies, graph_method), _latency_row(latencies, hyper_method)
            graph = {"boundary": gb["boundary"], "cells": gb["cells"], "overlay_arcs": gb["arcs"],
                     "array_bytes": gb["array_bytes"], "mean_us": gl["mean_us"], "p50_us": gl["p50_us"],
                     "p95_us": gl["p95_us"], "settled": gl["settled"], "scanned": gl["scanned"],
                     "serial_s": gb["serial_s"], "parallel_s": gb["parallel_s"],
                     "walk8_p50_us": gl["walk8_p50_us"], "walk32_p50_us": gl["walk32_p50_us"],
                     "uniform_p50_us": gl["uniform_p50_us"]}
            hypergraph = {"boundary": hb["boundary"], "cells": hb["cells"], "overlay_arcs": hb["arcs"],
                          "array_bytes": hb["array_bytes"], "mean_us": hl["mean_us"], "p50_us": hl["p50_us"],
                          "p95_us": hl["p95_us"], "settled": hl["settled"], "scanned": hl["scanned"],
                          "serial_s": hb["serial_s"], "parallel_s": hb["parallel_s"],
                          "walk8_p50_us": hl["walk8_p50_us"], "walk32_p50_us": hl["walk32_p50_us"],
                          "uniform_p50_us": hl["uniform_p50_us"]}
            comparison = {
                "bound": bound,
                "graph": graph,
                "hypergraph": hypergraph,
                "boundary_reduction": 1 - hypergraph["boundary"] / graph["boundary"],
                "overlay_reduction": 1 - hypergraph["overlay_arcs"] / graph["overlay_arcs"],
                "mean_speedup": graph["mean_us"] / hypergraph["mean_us"],
                "p50_speedup": graph["p50_us"] / hypergraph["p50_us"],
                "settled_reduction": 1 - hypergraph["settled"] / graph["settled"],
            }
            comparisons.append(comparison)
            ratios.append(comparison)
        baseline_methods = ["dijkstra", "bidirectional"] + (["fast_paths-CH"] if scale == 1000 else [])
        baselines = []
        for method in baseline_methods:
            row = _latency_row(latencies, method)
            baselines.append({key: row[key] for key in (
                "method", "mean_us", "p50_us", "p95_us", "settled", "scanned",
                "walk8_p50_us", "walk32_p50_us", "uniform_p50_us",
            )})
        metrics[str(scale)] = {
            "comparisons": comparisons,
            "baselines": baselines,
            "workload": workload(graph_rows[scale], scale),
        }
        comparisons_by_scale[scale] = ratios

    label, country = LABELS[city]
    source = graph_rows[0]["source"]
    if city in {"london-large", "new_york-large", "tokyo-large"}:
        source_url = f"https://huggingface.co/datasets/HBNU-NLP/InstructOSM/tree/{source['revision']}"
    else:
        source_url = f"https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta/tree/{source['revision']}"
    return {
        "id": city,
        "label": label,
        "country": country,
        "nodes": graph_rows[0]["nodes"],
        "arcs": graph_rows[0]["arcs"],
        "source_url": source_url,
        "metrics": metrics,
    }, comparisons_by_scale


def summarize(comparisons):
    by_bound = []
    for bound in BOUNDS:
        rows = [row for row in comparisons if row["bound"] == bound]
        by_bound.append({
            "bound": bound,
            "count": len(rows),
            "mean_wins": sum(row["mean_speedup"] > 1 for row in rows),
            "p50_wins": sum(row["p50_speedup"] > 1 for row in rows),
            "median_boundary_reduction": statistics.median(row["boundary_reduction"] for row in rows),
            "median_mean_speedup": statistics.median(row["mean_speedup"] for row in rows),
            "median_p50_speedup": statistics.median(row["p50_speedup"] for row in rows),
        })
    return {
        "count": len(comparisons),
        "median_boundary_reduction": statistics.median(row["boundary_reduction"] for row in comparisons),
        "min_boundary_reduction": min(row["boundary_reduction"] for row in comparisons),
        "max_boundary_reduction": max(row["boundary_reduction"] for row in comparisons),
        "median_mean_speedup": statistics.median(row["mean_speedup"] for row in comparisons),
        "median_p50_speedup": statistics.median(row["p50_speedup"] for row in comparisons),
        "mean_wins": sum(row["mean_speedup"] > 1 for row in comparisons),
        "p50_wins": sum(row["p50_speedup"] > 1 for row in comparisons),
        "total": len(comparisons),
        "by_bound": by_bound,
    }


def validate_order_experiment(experiment, results, source_sha, producer_sha):
    """Check the registered query-order diagnostic against its frozen inputs."""
    provenance = experiment.get("provenance") if isinstance(experiment, dict) else None
    if (not isinstance(experiment, dict) or experiment.get("schema") != 1
            or not isinstance(provenance, dict) or provenance.get("frozen_results_sha256") != source_sha):
        raise ValueError("query-order experiment does not identify these frozen results")
    experiments = experiment.get("experiments")
    required_cities = {"sejong", "london-large"}
    if not isinstance(experiments, list) or {e.get("city") for e in experiments if isinstance(e, dict)} != required_cities or len(experiments) != 2:
        raise ValueError("query-order experiment city population is invalid")
    conditions = {"fixed", "reversed", "precleared"}
    methods = {"dijkstra", "Hypergraph-1024"}
    expected_pairs = {(condition, method) for condition in conditions for method in methods}
    for item in experiments:
        city = item.get("city")
        graph_rows = [row for row in results[city] if row.get("kind") == "graph" and row.get("scale") == 0]
        if len(graph_rows) != 1:
            raise ValueError("query-order city lacks one frozen native graph")
        graph = graph_rows[0]
        if (item.get("scale") != 0 or item.get("bound") != 1024 or item.get("queries") != 1000
                or item.get("repeats") != 3 or item.get("same_current_query_work") is not True
                or item.get("source_binary_sha256") != graph.get("binary_sha256")
                or item.get("source_benchmark_sha") != producer_sha):
            raise ValueError("query-order experiment settings or source hashes mismatch")
        rows = item.get("results")
        if not isinstance(rows, list) or len(rows) != 6:
            raise ValueError("query-order result population is incomplete")
        work_by_method = {}
        seen = set()
        for row in rows:
            if not isinstance(row, dict):
                raise ValueError("query-order result row is invalid")
            key = (row.get("condition"), row.get("method"))
            if key not in expected_pairs or key in seen:
                raise ValueError("query-order condition/method population is invalid")
            seen.add(key)
            if row.get("wrong") != 0:
                raise ValueError("query-order diagnostic contains incorrect answers")
            for field in ("mean_query_us", "mean_preparation_us"):
                value = row.get(field)
                if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                    raise ValueError("query-order timing metric is invalid")
            groups = row.get("groups")
            expected_groups = {"walk8": 250, "walk32": 250, "uniform": 500}
            if not isinstance(groups, list) or len(groups) != 3 or {g.get("category"): g.get("count") for g in groups if isinstance(g, dict)} != expected_groups:
                raise ValueError("query-order group population is invalid")
            for group in groups:
                for field, value in group.items():
                    if field == "category":
                        continue
                    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                        raise ValueError("query-order group metric is invalid")
            for field in ("per_query_mean_us", "per_query_preparation_us", "per_query_previous_touched", "per_query_work"):
                values = row.get(field)
                if not isinstance(values, list) or len(values) != 1000:
                    raise ValueError("query-order per-query population is invalid")
                if field == "per_query_work":
                    if any(not isinstance(pair, list) or len(pair) != 2 or any(isinstance(v, bool) or not isinstance(v, int) or v < 0 for v in pair) for pair in values):
                        raise ValueError("query-order work counters are invalid")
                elif any(isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or v < 0 for v in values):
                    raise ValueError("query-order per-query metric is invalid")
            prior = work_by_method.setdefault(key[1], row["per_query_work"])
            if row["per_query_work"] != prior:
                raise ValueError("query-order work counters differ across conditions")
        if seen != expected_pairs:
            raise ValueError("query-order condition/method population is incomplete")
    return experiment


def build_payload(source, orderfile=None):
    source = Path(source)
    raw = source.read_bytes()
    results = load_cities(source)
    query_count, repeats, _threads, seed, producer_sha, _flags = validate_report_set(results)
    if query_count < PRESETS[-1] + 1:
        raise ValueError("frozen results do not contain all ten requested query presets")
    cities, aggregate = [], {0: [], 1000: []}
    for city in sorted(CITIES):
        entry, ratios = city_insight(city, results[city])
        cities.append(entry)
        for scale in aggregate:
            aggregate[scale].extend(ratios[scale])
    payload = {
        "schema": 1,
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "producer_sha": producer_sha,
        "queries_per_graph": query_count,
        "repeats": repeats,
        "seed": seed,
        "analysis_note": "Post-hoc descriptive analysis of the frozen benchmark; not a new algorithm or causal claim. Query-group latency follows the fixed walk8, walk32, uniform order and is confounded by preceding-query cleanup; do not interpret group contrasts causally.",
        "cities": cities,
        "summary": {str(scale): summarize(comparisons) for scale, comparisons in aggregate.items()},
    }
    if orderfile is not None and Path(orderfile).is_file():
        experiment = json.loads(Path(orderfile).read_text(encoding="utf-8"))
        payload["order_experiment"] = validate_order_experiment(experiment, results, payload["source_sha256"], producer_sha)
    else:
        payload["order_experiment"] = None
    return payload


def javascript(payload):
    data = json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
    return f"window.ROUTING_INSIGHTS = {data};\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("results/metro.jsonl"))
    parser.add_argument("--output", type=Path, default=Path("maps/insights.js"))
    parser.add_argument("--orderfile", type=Path, default=Path("results/query-order.json"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    source = args.source if args.source.is_absolute() else root / args.source
    output = args.output if args.output.is_absolute() else root / args.output
    orderfile = args.orderfile if args.orderfile.is_absolute() else root / args.orderfile
    payload = build_payload(source, orderfile)
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_suffix(".partial")
    with partial.open("w", encoding="utf-8", newline="\n") as file:
        file.write(javascript(payload))
    partial.replace(output)
    print(f"Wrote schema {payload['schema']} for {len(payload['cities'])} cities from {payload['source_sha256']}")


if __name__ == "__main__":
    main()
