"""Build site/data.js, the normalized evidence behind the published results page.

The page formats these values and takes medians and ratios of them; it runs no model. Inputs are the
retained measurements in results/. Normalizations:

    cost index      core time per request relative to plain Dijkstra in the same city, = 100
    core-hours/1e9  1e9 / (one-worker queries per second) / 3600
    cut ratio       junction-cut value / edge-cut value at the same cell bound, = 1.00
    carryover share (fixed-order median - pre-cleared median) / fixed-order median
    daily cost      core-hours per day at R requests and K updates: R q / 1e9 + K u / 3600
    regret          cost on the actual slot of the route chosen on stale times / optimum - 1
    partial ratio   core-seconds of a partial CCH update / a full one on the same update, = 1.00
"""

import argparse
import hashlib
import json
import math
import re
import statistics
from pathlib import Path

import scale
import traffic
from metro import BOUNDS, CITIES, load_cities, validate_report_set

ROOT = Path(__file__).resolve().parents[1]
LABELS = {
    "sejong": "Sejong", "seoul": "Seoul", "busan": "Busan", "daegu": "Daegu", "incheon": "Incheon",
    "gwangju": "Gwangju", "daejeon": "Daejeon", "ulsan": "Ulsan", "london-large": "London",
    "new_york-large": "New York", "tokyo-large": "Tokyo", "korea": "South Korea, nationwide",
}
TRAFFIC_LABELS = {
    "dijkstra": "Plain Dijkstra", "bidirectional": "Bidirectional Dijkstra",
    "overlay-1024": "Overlay, junction cut, 1,024", "overlay-4096": "Overlay, junction cut, 4,096",
    "fast_paths-CH": "Contraction hierarchy, rebuilt", "fast_paths-CH-fixed-order": "Contraction hierarchy, fixed order",
    "CCH": "Customizable contraction hierarchy",
}
# Updates per day beyond this (once per second) are off the exhibit's scale.
K_MAX = 86400
WORLD = {"london-large", "new_york-large", "tokyo-large"}
ENGINES = (
    ("dijkstra", "Plain Dijkstra"),
    ("bidirectional", "Bidirectional Dijkstra"),
    ("Graph", "Overlay, edge cut"),
    ("Hypergraph", "Overlay, junction cut"),
    ("fast_paths-CH", "Contraction hierarchy"),
)

# Published results on the same 1,000 Maze-Hard test mazes. "exact" is exact match with the
# reference answer grid; "shortest" accepts any shortest path. Figures as stated by each source.
PUBLISHED = (
    ("HRM", "learned", "exact", 74.5, "https://arxiv.org/abs/2506.21734"),
    ("TRM", "learned", "exact", 85.3, "https://arxiv.org/abs/2510.04871"),
    ("EBR, five samples", "learned", "exact", 88.3, "https://arxiv.org/abs/2611.08977"),
    ("GFPR", "learned", "exact", 92.0, "https://arxiv.org/abs/2609.33540"),
    ("TRM, best checkpoint", "learned", "exact", 95.0, "https://arxiv.org/abs/2611.23677"),
    ("STRM, best checkpoint", "learned", "exact", 95.1, "https://arxiv.org/abs/2611.23677"),
    ("LDT, 1.8M parameters", "learned", "shortest", 99.8, "https://arxiv.org/abs/2605.08605"),
    ("LoopedWSP", "hand-written weights", "shortest", 100.0, "https://mbzuai-oryx.github.io/LoopedWSP/"),
)
# SALSA-CLRS, arXiv 2309.12253, Tables 1-4: best share of n = 1,600 graphs fully correct, ER / WS / Delaunay.
SALSA_PUBLISHED = {"bfs": [0.4, 0.0, 0.0], "dijkstra": [0.0, 0.0, 0.0]}


def validate_order_experiment(experiment, results, source_sha, producer_sha):
    """Check the registered query-order diagnostic against its frozen inputs."""
    provenance = experiment.get("provenance") if isinstance(experiment, dict) else None
    if (not isinstance(experiment, dict) or experiment.get("schema") != 1
            or not isinstance(provenance, dict) or provenance.get("frozen_results_sha256") != source_sha):
        raise ValueError("query-order experiment does not identify these frozen results")
    experiments = experiment.get("experiments")
    if (not isinstance(experiments, list) or len(experiments) != 2
            or {e.get("city") for e in experiments if isinstance(e, dict)} != {"sejong", "london-large"}):
        raise ValueError("query-order experiment city population is invalid")
    expected = {(c, m) for c in ("fixed", "reversed", "precleared") for m in ("dijkstra", "Hypergraph-1024")}
    for item in experiments:
        _order_city(item, results, producer_sha)
        _order_rows(item.get("results"), expected)
    return experiment


def _order_city(item, results, producer_sha):
    graphs = [r for r in results[item["city"]] if r.get("kind") == "graph" and r.get("scale") == 0]
    if len(graphs) != 1:
        raise ValueError("query-order city lacks one frozen native graph")
    if (item.get("scale") != 0 or item.get("bound") != 1024 or item.get("queries") != 1000
            or item.get("repeats") != 3 or item.get("same_current_query_work") is not True
            or item.get("source_binary_sha256") != graphs[0].get("binary_sha256")
            or item.get("source_benchmark_sha") != producer_sha):
        raise ValueError("query-order experiment settings or source hashes mismatch")


def _order_groups(row):
    groups = row.get("groups")
    if (not isinstance(groups, list) or len(groups) != 3
            or {g.get("category"): g.get("count") for g in groups if isinstance(g, dict)}
            != {"walk8": 250, "walk32": 250, "uniform": 500}):
        raise ValueError("query-order group population is invalid")
    if any(not _finite(v) for g in groups for k, v in g.items() if k != "category"):
        raise ValueError("query-order group metric is invalid")


def _order_samples(row):
    for field in ("per_query_mean_us", "per_query_preparation_us", "per_query_previous_touched", "per_query_work"):
        values = row.get(field)
        if not isinstance(values, list) or len(values) != 1000:
            raise ValueError("query-order per-query population is invalid")
        if field == "per_query_work":
            if any(not isinstance(p, list) or len(p) != 2
                   or any(isinstance(v, bool) or not isinstance(v, int) or v < 0 for v in p) for p in values):
                raise ValueError("query-order work counters are invalid")
        elif not all(_finite(v) for v in values):
            raise ValueError("query-order per-query metric is invalid")


def _order_rows(rows, expected):
    if not isinstance(rows, list) or len(rows) != 6:
        raise ValueError("query-order result population is incomplete")
    work, seen = {}, set()
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("query-order result row is invalid")
        key = (row.get("condition"), row.get("method"))
        if key not in expected or key in seen:
            raise ValueError("query-order condition/method population is invalid")
        seen.add(key)
        if row.get("wrong") != 0:
            raise ValueError("query-order diagnostic contains incorrect answers")
        for field in ("mean_query_us", "mean_preparation_us"):
            if not _finite(row.get(field)):
                raise ValueError("query-order timing metric is invalid")
        _order_groups(row)
        _order_samples(row)
        if work.setdefault(key[1], row["per_query_work"]) != row["per_query_work"]:
            raise ValueError("query-order work counters differ across conditions")
    if seen != expected:
        raise ValueError("query-order condition/method population is incomplete")


def _finite(v):
    return not isinstance(v, bool) and isinstance(v, (int, float)) and math.isfinite(v) and v >= 0


def _rows(records, kind, scale, threads=None):
    return {r["method"]: r for r in records if r["kind"] == kind and r["scale"] == scale
            and (threads is None or r["threads"] == threads)}


def city_entry(city, records):
    """Normalized cost, latency, build and cut figures for one city."""
    graph = next(r for r in records if r["kind"] == "graph" and r["scale"] == 0)
    revision = graph["source"]["revision"]
    source = (f"https://huggingface.co/datasets/HBNU-NLP/InstructOSM/tree/{revision}" if city in WORLD
              else f"https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta/tree/{revision}")
    # Cost: rounded metric, where all five engines run; one worker, so 1/QPS is core time per request.
    qps = _rows(records, "throughput", 1000, threads=1)
    latency = _rows(records, "latency", 1000)
    builds = _rows(records, "build", 1000)
    engines = {}
    for key, _label in ENGINES:
        if key in ("Graph", "Hypergraph"):
            method = max((f"{key}-{u}" for u in BOUNDS), key=lambda m: qps[m]["median_qps"])
        else:
            method = key
        engines[key] = {
            "method": method,
            "core_hours_per_billion": 1e9 / qps[method]["median_qps"] / 3600,
            "mean_us": latency[method]["mean_us"],
            "p50_us": latency[method]["p50_us"],
            "p95_us": latency[method]["p95_us"],
            "build_s": builds[method]["serial_s"] if method in builds else 0.0,
        }
    base = engines["dijkstra"]["core_hours_per_billion"]
    for e in engines.values():
        e["cost_index"] = 100 * e["core_hours_per_billion"] / base
    # Cut comparison: native seconds, junction cut / edge cut at each cell bound.
    nb, nl = _rows(records, "build", 0), _rows(records, "latency", 0)
    cuts = []
    for u in BOUNDS:
        g, h = f"Graph-{u}", f"Hypergraph-{u}"
        cuts.append({
            "bound": u,
            "boundary": nb[h]["boundary"] / nb[g]["boundary"],
            "shortcuts": nb[h]["arcs"] / nb[g]["arcs"],
            "settled": nl[h]["settled"] / nl[g]["settled"],
            "mean_us": nl[h]["mean_us"] / nl[g]["mean_us"],
            "p50_us": nl[h]["p50_us"] / nl[g]["p50_us"],
            "build_s": nb[h]["serial_s"] / nb[g]["serial_s"],
        })
    return {
        "id": city, "label": LABELS[city], "nodes": graph["nodes"], "arcs": graph["arcs"],
        "cpu": graph["machine"]["cpu"], "source_url": source, "engines": engines, "cuts": cuts,
    }


def engine_summary(cities):
    """Median and range over the cities of each engine's cost index, core-hours and build seconds."""
    out = {}
    for key, _ in ENGINES:
        rows = [c["engines"][key] for c in cities]
        out[key] = {f: {"median": statistics.median(r[f] for r in rows), "min": min(r[f] for r in rows),
                        "max": max(r[f] for r in rows)}
                    for f in ("cost_index", "core_hours_per_billion", "mean_us", "build_s")}
    cheapest = [min(c["engines"], key=lambda k: c["engines"][k]["cost_index"]) for c in cities]
    out["cheapest_counts"] = {k: cheapest.count(k) for k, _ in ENGINES}
    return out


def cut_summary(cities):
    """Median and range of each junction/edge ratio over the cities, per cell bound."""
    out = []
    for i, u in enumerate(BOUNDS):
        row = {"bound": u}
        for field in ("boundary", "shortcuts", "settled", "mean_us", "p50_us", "build_s"):
            values = [c["cuts"][i][field] for c in cities]
            row[field] = {"median": statistics.median(values), "min": min(values), "max": max(values)}
        row["p50_wins"] = sum(c["cuts"][i]["p50_us"] < 1 for c in cities)
        out.append(row)
    return out


def order_summary(experiment):
    """Fixed-order and pre-cleared medians of the 8-arc walk group, whose call inherits a uniform query's state."""
    out = []
    for item in experiment["experiments"]:
        for method in ("dijkstra", "Hypergraph-1024"):
            group = {}
            for row in item["results"]:
                if row["method"] == method:
                    group[row["condition"]] = next(g for g in row["groups"] if g["category"] == "walk8")
            fixed, cleared = group["fixed"]["median_query_us"], group["precleared"]["median_query_us"]
            out.append({
                "city": LABELS[item["city"]], "method": method,
                "fixed_us": fixed, "reversed_us": group["reversed"]["median_query_us"],
                "precleared_us": cleared, "carryover_share": (fixed - cleared) / fixed,
                "previous_touched": group["fixed"]["median_previous_touched"],
            })
    return out


def maze_bench(text):
    """Parse the first table of results/bench.md: method, exact, optimal, median, p95, mazes/s, ratio, heap."""
    rows = []
    for line in text.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) == 8 and re.fullmatch(r"\d+", cells[1] or ""):
            rows.append({"method": cells[0], "exact": int(cells[1]), "optimal": int(cells[2]),
                         "median_us": float(cells[3]), "heap_bytes": int(cells[7])})
        elif rows:
            break
    if not rows or rows[0]["method"] != "wave":
        raise ValueError("bench.md does not start with the wave row")
    return rows


def build_payload(results_dir=ROOT / "results"):
    source = results_dir / "metro.jsonl"
    raw = source.read_bytes()
    results = load_cities(source)
    queries, repeats, threads, seed, producer, _flags = validate_report_set(results)
    sha = hashlib.sha256(raw).hexdigest()
    cities = [city_entry(c, results[c]) for c in sorted(CITIES, key=lambda c: LABELS[c])]

    order = json.loads((results_dir / "query-order.json").read_text(encoding="utf-8"))
    validate_order_experiment(order, results, sha, producer)

    evals = [json.loads(l) for l in (results_dir / "eval.jsonl").read_text(encoding="utf-8").splitlines() if l]
    test = next(r for r in evals if r["file"] == "test.csv")
    if not test["rows"] == test["certified"] == test["exact"] == 1000:
        raise ValueError("eval.jsonl no longer reports 1,000 certified exact test answers")
    salsa = [json.loads(l) for l in (results_dir / "salsa.jsonl").read_text(encoding="utf-8").splitlines() if l]
    if len(salsa) != 30 or any(r["graph"] != 1.0 or r["certified"] != r["graphs"] for r in salsa):
        raise ValueError("salsa.jsonl no longer reports 30 fully certified sets")
    big = [r for r in salsa if r["set"].endswith("_1600")]

    tpath, spath = results_dir / "traffic.jsonl", results_dir / "scale.jsonl"
    return {
        "schema": 3,
        "traffic": traffic_payload(tpath) if tpath.exists() else None,
        "scale": scale_payload(spath) if spath.exists() else None,
        "provenance": {"metro_sha256": sha, "producer_sha": producer, "queries": queries,
                       "repeats": repeats, "threads": threads, "seed": seed},
        "engines": [{"id": k, "label": v} for k, v in ENGINES],
        "cities": cities,
        "summary": engine_summary(cities),
        "cuts": cut_summary(cities),
        "order": order_summary(order),
        "maze": {
            "test": test,
            "bench": maze_bench((results_dir / "bench.md").read_text(encoding="utf-8")),
            "published": [dict(zip(("method", "kind", "metric", "percent", "url"), p)) for p in PUBLISHED],
        },
        "salsa": {
            "graphs": sum(r["graphs"] for r in salsa),
            "max_us_1600": max(r["us"] for r in big),
            "published_1600": SALSA_PUBLISHED,
        },
    }


def traffic_payload(path):
    """Normalized traffic-study evidence: per-region engines, break-even and regret rows."""
    raw = path.read_bytes()
    rows = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line]
    regions = [r for r in traffic.REGIONS if any(x["kind"] == "source" and x["region"] == r for x in rows)]
    if not regions:
        raise ValueError("traffic.jsonl has no regions")
    out = []
    for region in regions:
        src = next(r for r in rows if r["kind"] == "source" and r["region"] == region)
        eng = {r["method"]: r for r in rows if r["kind"] == "engine" and r["region"] == region}
        if set(eng) != set(traffic.ENGINES):
            raise ValueError(f"{region}: incomplete engine population")
        base = eng["dijkstra"]["q"]
        breakeven = []
        for b in (r for r in rows if r["kind"] == "breakeven" and r["region"] == region):
            first, switches = traffic.breakeven(eng, b["requests"])
            if first != b["cheapest_static"] or [e for _, e in switches] != [x["to"] for x in b["switches"]]:
                raise ValueError(f"{region}: break-even rows do not recompute")
            breakeven.append({"requests": b["requests"], "static": first, "ties": b["static_ties"],
                              "switches": [{"k": x["k"], "to": x["to"], "k_pass": x["k_pass"]}
                                           for x in b["switches"] if x["k"] <= K_MAX]})
        regret = [{k: r[k] for k in ("series", "from", "to", "group", "pairs", "mean", "p95", "share_ge5", "share_changed",
                                     "eta_mean", "eta_abs_mean", "eta_abs_p95")}
                  for r in rows if r["kind"] == "regret" and r["region"] == region]
        if any(r["mean"] < 0 for r in regret):
            raise ValueError(f"{region}: negative regret")
        out.append({
            "id": region, "label": LABELS.get(region, region.title()), "nodes": src["nodes"], "arcs": src["arcs"],
            "queries": src["queries"], "reachable": src["reachable"], "trees": src["certified_trees"],
            "engines": {e: {"q": v["q"], "q_range": [min(v["q_pass"]), max(v["q_pass"])], "u": v["u"],
                            "u_range": [min(v["u_pass"]), max(v["u_pass"])], "prepare_s": v["prepare_s"],
                            "index": 100 * v["q"] / base, "checked": v["checked"]} for e, v in eng.items()},
            "breakeven": breakeven, "regret": regret,
        })
    return {"sha256": hashlib.sha256(raw).hexdigest(), "k_max": K_MAX,
            "engines": [{"id": e, "label": TRAFFIC_LABELS[e]} for e in traffic.ENGINES],
            "slots": list(traffic.METRICS), "observed": sorted(traffic.OBSERVED), "regions": out}


def scale_payload(path):
    """Normalized size-study evidence: per size its nodes, K* with its pass range, the cheapest
    engine and switches at 10⁶ requests, q and u of each engine, and the partial-update ratios
    u(f)/u_full; the fitted growth exponents. K* and f* are recomputed and must match the rows."""
    raw = path.read_bytes()
    rows = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line]
    sizes = [r for r in rows if r["kind"] == "source"]
    if [s["size"] for s in sizes] != [s for s in scale.SIZES if any(x["size"] == s for x in sizes)] or not sizes:
        raise ValueError("scale.jsonl sizes out of order or missing")
    out = []
    for s in sizes:
        eng = {r["method"]: r for r in rows if r["kind"] == "engine" and r["size"] == s["size"]}
        b = next(r for r in rows if r["kind"] == "breakeven" and r["size"] == s["size"] and r["requests"] == 10**6)
        first, switches, k = scale.kstar(eng)
        if first != b["cheapest_static"] or k != b["k_cch"] or [e for _, e in switches] != [x["to"] for x in b["switches"]]:
            raise ValueError(f"{s['size']}: break-even rows do not recompute")
        ups = [r for r in rows if r["kind"] == "update" and r["size"] == s["size"]]
        part = next(r for r in rows if r["kind"] == "partial" and r["size"] == s["size"])
        syn = {f: [u["ratio"] for u in ups if u["target_f"] == f] for f in scale.F}
        if part["f_star"] != scale.crossing(*scale.curve(ups)):
            raise ValueError(f"{s['size']}: f* does not recompute")
        data = [u for u in ups if u["sequence"] != "synthetic" and u["changed"]]
        rk = next((r for r in rows if r["kind"] == "routingkit" and r["size"] == s["size"]), None)
        out.append({
            "id": s["size"], "label": s["label"], "nodes": s["nodes"], "queries": s["queries"],
            "trees": s["certified_trees"], "k": b["k_cch"], "k_range": [min(b["k_cch_pass"]), max(b["k_cch_pass"])],
            "static": first, "switches": b["switches"],
            "engines": {e: {"q": v["q"], "u": v["u"], "u_wall": v["u_wall"], "prepare_s": v["prepare_s"],
                            "memory_bytes": v["memory_bytes"]} for e, v in eng.items()},
            "f_star": part["f_star"], "f_star_range": [min(x for x in part["f_star_pass"] if x is not None),
                                                       max(x for x in part["f_star_pass"] if x is not None)],
            "ratio": [{"f": f, "mean": statistics.fmean(v), "min": min(v), "max": max(v)} for f, v in syn.items()],
            "dataset": {"f_min": min(u["f"] for u in data), "f_max": max(u["f"] for u in data),
                        "ratio_mean": statistics.fmean(u["ratio"] for u in data)},
            "routingkit": rk and {k: rk[k] for k in ("customize_s", "customize4_wall_s", "q", "prepare_s", "ratio_by_f")},
        })
    growth = {r["method"]: r for r in rows if r["kind"] == "growth"}
    return {"sha256": hashlib.sha256(raw).hexdigest(), "sizes": out, "growth": growth}


def javascript(payload):
    data = json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":"))
    return f"window.DATA = {data};\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, default=ROOT / "site" / "data.js")
    args = parser.parse_args()
    payload = build_payload()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(javascript(payload), encoding="utf-8", newline="\n")
    print(f"wrote {args.output.relative_to(ROOT)} from metro.jsonl {payload['provenance']['metro_sha256'][:12]}")


if __name__ == "__main__":
    main()
