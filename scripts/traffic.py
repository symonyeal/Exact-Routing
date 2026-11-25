"""Time-of-day traffic study: fetch slot metrics, run the `traffic` harness, summarize.

  python scripts/traffic.py fetch REGION             data/traffic/REGION.{bin,metrics.bin,meta.json}
  python scripts/traffic.py run REGION OUT.jsonl     certified costs and timings for one region
  python scripts/traffic.py report DIR               results/traffic.jsonl and results/traffic.md

Source: CAMUS-LAB/data-road_network_eta at the revision pinned in roads.py (CC BY-NC 4.0, so the
results are non-commercial; topology OpenStreetMap, ODbL; speeds KTDB 2024). Metric 0 is free flow,
`default_speed_kmh`: the region and nationwide files carry no `free_flow_speed_kmh`, and the examples
config's `free_flow_speed_kmh` equals `default_speed_kmh` row for row (checked on the first 1,048,576
nationwide rows, 2026-11-25). It is a static speed by road class, below the observed slot speed on
about half the edges, not a measured uncongested speed. Metrics
1-8 are the weekday p50 slots in the order of SLOTS, which this script treats as the day's order
(the card names the slots but gives no clock hours). The card marks am_peak, midday, pm_peak and
offpeak as observed and the other four as derived from them.

Legend
  q    core-hours per 10^9 requests: one-thread query seconds per request x 10^9 / 3600
  u    core-seconds to bring an index to a new slot's metric (one thread)
  C    daily core-hours R q / 10^9 + K u / 3600 for R requests and K metric updates per day
  c    c[tau][sigma]: cost on metric sigma of the path chosen on metric tau, milliseconds
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import roads  # noqa: E402
from metro import machine  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
DATA = REPO / "data" / "traffic"
SLOTS = ("am_peak", "am_shoulder", "midday", "afternoon", "pm_peak", "pm_shoulder", "offpeak", "night")
OBSERVED = {"am_peak", "midday", "pm_peak", "offpeak"}
COLUMNS = ("default_speed_kmh",) + tuple(f"weekday_{s}_p50" for s in SLOTS)
METRICS = ("free",) + SLOTS
REGIONS = ("sejong", "seoul", "busan", "daegu", "incheon", "gwangju", "daejeon", "ulsan", "korea")
ENGINES = ("dijkstra", "bidirectional", "overlay-1024", "overlay-4096", "fast_paths-CH",
           "fast_paths-CH-fixed-order", "CCH")
REQUESTS = (10**6, 10**7, 10**8)
LICENCE = "CC BY-NC 4.0"


def fetch(region):
    stem = "nationwide/korea" if region == "korea" else f"regions/{region}"
    nodes, nodes_hash = roads.read_table(f"{stem}_road_graph_nodes.parquet", ["node_id", "lat", "lon"])
    edges, edges_hash = roads.read_table(
        f"{stem}_road_graph_edges.parquet",
        ["source", "target", "length_m", "oneway", "default_speed_kmh", *COLUMNS[1:]],
    )
    source = {
        "dataset": "CAMUS-LAB/data-road_network_eta",
        "url": roads.URL.format(f"{stem}_road_graph_edges.parquet"),
        "revision": roads.REVISION,
        "licence": LICENCE,
        "region": region,
        "edge_rows": len(edges),
        "nodes_sha256": nodes_hash,
        "edges_sha256": edges_hash,
    }
    store(DATA, region, nodes, edges, COLUMNS, METRICS, source)


def store(directory, name, nodes, edges, columns, metrics, source):
    """Write directory/name.bin (the road binary, am_peak weights), name.metrics.bin (one
    travel-time column per speed column, named by `metrics`) and name.meta.json (`source` plus
    hashes and per-column statistics)."""
    binary, n, m = roads.encode(nodes, edges, "weekday_am_peak_p50")
    two = ~edges.oneway.to_numpy(bool)
    w = np.stack([roads.seconds(edges, c, two) for c in columns])
    cols = np.array([len(columns), m], "<u8").tobytes() + w.astype("<f8").tobytes()
    default = edges.default_speed_kmh.to_numpy(np.float64)
    stats = {}
    for metric, column in zip(metrics, columns):
        kmh = edges[column].to_numpy(np.float64)
        filled = np.where(np.isnan(kmh), default, kmh)
        stats[metric] = {
            "missing_edges": int(np.isnan(kmh).sum()),
            "clipped_edges": int((filled < 5.0).sum()),
            "faster_than_default_edges": int((filled > default).sum()),
            "observed": metric.split(" ")[-1] in OBSERVED,
        }
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{name}.bin").write_bytes(binary)
    (directory / f"{name}.metrics.bin").write_bytes(cols)
    meta = {
        **source,
        "output_sha256": hashlib.sha256(binary).hexdigest(),
        "metrics_sha256": hashlib.sha256(cols).hexdigest(),
        "node_count": n,
        "arc_count": m,
        "metrics": list(metrics),
        "columns": list(columns),
        "speed_rule": "missing slot speed = default_speed_kmh; speed clipped below at 5 km/h",
        "column_stats": stats,
    }
    (directory / f"{name}.meta.json").write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"{name}: {n} nodes, {m} arcs, {len(columns)} metrics")


def run(region, output, queries, repeats, threads):
    import subprocess

    meta = json.loads((DATA / f"{region}.meta.json").read_text(encoding="utf-8"))
    road, cols = DATA / f"{region}.bin", DATA / f"{region}.metrics.bin"
    for path, key in ((road, "output_sha256"), (cols, "metrics_sha256")):
        if hashlib.sha256(path.read_bytes()).hexdigest() != meta[key]:
            raise ValueError(f"{path.name} does not match its metadata")
    exe = REPO / "target" / "release" / ("traffic.exe" if sys.platform == "win32" else "traffic")
    # Rows are copied to OUTPUT.partial as they arrive, so a run cut off by a time limit keeps them.
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_name(output.name + ".partial")
    lines = []
    with subprocess.Popen([str(exe), str(road), str(cols), str(queries), str(repeats), str(threads)],
                          cwd=REPO, stdout=subprocess.PIPE, text=True) as proc, \
            partial.open("w", encoding="utf-8", newline="\n") as f:
        for line in proc.stdout:
            lines.append(line)
            f.write(line)
            f.flush()
    if proc.returncode:
        raise subprocess.CalledProcessError(proc.returncode, proc.args)
    rows = [json.loads(line) for line in lines]
    try:
        import resource  # POSIX only; ru_maxrss is in KiB on Linux
        peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024
    except ImportError:
        peak = None
    rows[0].update(region=region, source=meta, machine=machine(REPO), peak_rss_bytes=peak)
    for row in rows:
        row["region"] = region
    validate(rows)
    output.write_text("".join(json.dumps(r, sort_keys=True, allow_nan=False) + "\n" for r in rows),
                      encoding="utf-8", newline="\n")


def validate(rows):
    """Check one region's raw rows: the reference matrix and a complete, correct timing population."""
    g = rows[0]
    k, q, repeats = len(METRICS), g["queries"], g["repeats"]
    if g["kind"] != "graph" or g["metrics"] != k or len(g["costs"]) != q or g["certified_trees"] != q * k:
        raise ValueError("bad graph row")
    for (s, t), c in zip(g["pairs"], g["costs"]):
        if c is None:
            continue
        c = np.array(c).reshape(k, k)
        if s == t and c.any():
            raise ValueError("identical endpoints with a nonzero cost")
        if (c < np.diag(c)[None, :]).any():
            raise ValueError("a stale path beats the exact optimum")
    slots = [r for r in rows if r["kind"] == "slot"]
    if any(r["wrong"] for r in slots):
        raise ValueError("wrong answer")
    seen = {(r["method"], r["pass"], r["slot"]) for r in slots}
    if seen != {(e, p, s) for e in ENGINES for p in range(repeats) for s in range(len(SLOTS))} or len(slots) != len(seen):
        raise ValueError("incomplete timing population")
    for e in ENGINES:
        for p in range(repeats):
            if sum(r["queries"] for r in slots if r["method"] == e and r["pass"] == p) != q:
                raise ValueError("a pass does not cover every query once")
    if {r["method"] for r in rows if r["kind"] == "prepare"} != {"overlay-1024", "overlay-4096", "fast_paths-CH-fixed-order", "CCH"}:
        raise ValueError("missing preparation rows")


def engines(rows):
    """q, u and their per-pass values for each engine of one region."""
    g = rows[0]
    out = {}
    prep = {r["method"]: r for r in rows if r["kind"] == "prepare"}
    for e in ENGINES:
        slot = [r for r in rows if r["kind"] == "slot" and r["method"] == e]
        passes = range(g["repeats"])
        q_pass = [sum(r["query_s"] for r in slot if r["pass"] == p)
                  / sum(r["queries"] for r in slot if r["pass"] == p) * 1e9 / 3600 for p in passes]
        u_pass = [float(np.mean([r["update_s"] for r in slot if r["pass"] == p])) for p in passes]
        u_slot = [float(np.median([r["update_s"] for r in slot if r["slot"] == s])) for s in range(len(SLOTS))]
        out[e] = {
            "q": sum(r["query_s"] for r in slot) / sum(r["queries"] for r in slot) * 1e9 / 3600,
            "q_pass": q_pass,
            "u": float(np.mean(u_slot)),
            "u_slot": u_slot,
            "u_pass": u_pass,
            "prepare_s": prep[e]["seconds"] if e in prep else 0.0,
            "checked": sum(r["checked"] for r in slot),
        }
    return out


def envelope(lines):
    """Lower envelope over K >= 0 of lines C_e(K) = a_e + b_e K: the cheapest engine at K = 0 and
    each (K*, engine) where the cheapest changes. Ties at K = 0 go to the smaller slope."""
    e = min(lines, key=lambda x: (lines[x][0], lines[x][1]))
    at, out = 0.0, []
    while True:
        a, b = lines[e]
        nxt = None
        for f, (c, d) in lines.items():
            if d < b:
                k = (c - a) / (b - d)
                if k > at - 1e-12 and (nxt is None or (k, d) < (nxt[0], lines[nxt[1]][1])):
                    nxt = (max(k, at), f)
        if nxt is None:
            return out
        at, e = nxt
        out.append(nxt)


def breakeven(eng, requests, p=None):
    """Cheapest engine at K = 0 and its switch points, for R requests per day; from pass p's q and u
    when given, else from the means over passes."""
    pick = (lambda v: (v["q"], v["u"])) if p is None else (lambda v: (v["q_pass"][p], v["u_pass"][p]))
    lines = {e: (requests * pick(v)[0] / 1e9, pick(v)[1] / 3600) for e, v in eng.items()}
    first = min(lines, key=lambda x: (lines[x][0], lines[x][1]))
    return first, envelope(lines)


def overlaps(a, b):
    """Whether two engines' per-pass ranges of a measure overlap: not distinguishable."""
    return min(a) <= max(b) and min(b) <= max(a)


def regret(rows):
    """Regret and ETA error over reachable pairs with distinct endpoints. `series`: free (free flow ->
    each slot), consecutive (each slot -> the next, night wrapping to am_peak) or observed (each
    observed slot -> the next observed one, skipping the derived slots); `group` all or uniform (the
    uniform-pair half of the workload)."""
    g = rows[0]
    k = len(METRICS)
    idx = [i for i, ((s, t), c) in enumerate(zip(g["pairs"], g["costs"])) if c is not None and s != t]
    c = np.array([g["costs"][i] for i in idx], float).reshape(-1, k, k)
    uniform = np.array([i % 4 >= 2 for i in idx])
    seen = [METRICS.index(m) for m in METRICS if m in OBSERVED]
    moves = ([("free", 0, j) for j in range(1, k)] + [("consecutive", j, j % (k - 1) + 1) for j in range(1, k)]
             + [("observed", a, b) for a, b in zip(seen, seen[1:] + seen[:1])])
    out = []
    for series, tau, sigma in moves:
        for group, keep in (("all", np.ones(len(idx), bool)), ("uniform", uniform)):
            if not keep.any():
                continue
            x = c[keep]
            opt = x[:, sigma, sigma]
            r = x[:, tau, sigma] / opt - 1
            eta = x[:, tau, tau] / opt - 1
            out.append({
                "series": series, "from": METRICS[tau], "to": METRICS[sigma], "group": group, "pairs": int(keep.sum()),
                "mean": float(r.mean()), "p95": float(np.quantile(r, 0.95, method="inverted_cdf")),
                "share_ge5": float((r >= 0.05).mean()), "share_changed": float((r > 0).mean()),
                "eta_mean": float(eta.mean()), "eta_abs_mean": float(np.abs(eta).mean()),
                "eta_abs_p95": float(np.quantile(np.abs(eta), 0.95, method="inverted_cdf")),
            })
    return out


def summarize(path):
    """Committed summary rows for one raw region file."""
    raw = path.read_bytes()
    rows = [json.loads(line) for line in raw.decode("utf-8").splitlines()]
    validate(rows)
    g, region = rows[0], rows[0]["region"]
    prep = {r["method"]: r for r in rows if r["kind"] == "prepare"}
    out = [{
        "kind": "source", "region": region, "nodes": g["nodes"], "arcs": g["arcs"], "queries": g["queries"],
        "repeats": g["repeats"], "reachable": g["reachable"], "identical_endpoints": g["identical_endpoints"],
        "certified_trees": g["certified_trees"], "reference_s": g["reference_s"],
        "peak_rss_bytes": g.get("peak_rss_bytes"),
        "cch_edges": prep["CCH"]["edges"], "cch_tree_height": prep["CCH"]["tree_height"],
        "raw_sha256": hashlib.sha256(raw).hexdigest(), "source": g["source"], "machine": g["machine"],
    }]
    eng = engines(rows)
    out += [{"kind": "engine", "region": region, "method": e, **v} for e, v in eng.items()]
    for r in REQUESTS:
        first, switches = breakeven(eng, r)
        passes = [breakeven(eng, r, p) for p in range(g["repeats"])]
        same = all(f == first and [e for _, e in sw] == [e for _, e in switches] for f, sw in passes)
        out.append({
            "kind": "breakeven", "region": region, "requests": r, "cheapest_static": first,
            "static_ties": [e for e in ENGINES if e != first and overlaps(eng[e]["q_pass"], eng[first]["q_pass"])],
            "passes_agree": same,
            "switches": [{"k": k, "to": e, "k_pass": [sw[i][0] for _, sw in passes] if same else None}
                         for i, (k, e) in enumerate(switches)],
        })
    out += [{"kind": "regret", "region": region, **r} for r in regret(rows)]
    return out


def report(directory, results):
    files = sorted(directory.glob("*.jsonl"))
    rows = [r for f in files for r in summarize(f)]
    found = {r["region"] for r in rows if r["kind"] == "source"}
    if not found <= set(REGIONS) or not found:
        raise ValueError(f"unexpected regions {sorted(found)}")
    (results / "traffic.jsonl").write_text(
        "".join(json.dumps(r, sort_keys=True, allow_nan=False) + "\n" for r in rows), encoding="utf-8", newline="\n")
    (results / "traffic.md").write_text(markdown(rows), encoding="utf-8", newline="\n")


def markdown(rows):
    regions = [r for r in REGIONS if any(x["region"] == r for x in rows)]
    eng = {(r["region"], r["method"]): r for r in rows if r["kind"] == "engine"}
    src = {r["region"]: r for r in rows if r["kind"] == "source"}
    lines = ["# Traffic study", "", "Generated by `scripts/traffic.py report` from per-region raw files; "
             "Korea Road Network ETA, CC BY-NC 4.0 (non-commercial). Integer-millisecond travel times.", "",
             "## Engines", "",
             "| Region | Nodes | Engine | q, core-h / 10⁹ (pass range) | u, s per update (pass range) | Prepare, s |",
             "| --- | ---: | --- | ---: | ---: | ---: |"]
    for g in regions:
        for e in ENGINES:
            v = eng[g, e]
            lines.append(f"| {g} | {src[g]['nodes']:,} | {e} | {v['q']:.3g} ({min(v['q_pass']):.3g}–{max(v['q_pass']):.3g}) "
                         f"| {v['u']:.3g} ({min(v['u_pass']):.3g}–{max(v['u_pass']):.3g}) | {v['prepare_s']:.3g} |")
    lines += ["", "## Break-even updates per day", "",
              "| Region | Requests / day | Cheapest without updates | Switches (K per day → engine) |", "| --- | ---: | --- | --- |"]
    for r in (x for x in rows if x["kind"] == "breakeven"):
        sw = "; ".join(f"{s['k']:.3g}" + (f" ({min(s['k_pass']):.3g}–{max(s['k_pass']):.3g})" if s["k_pass"] else "")
                       + f" → {s['to']}" for s in r["switches"]) or "none"
        ties = f" (not distinguishable from {', '.join(r['static_ties'])})" if r["static_ties"] else ""
        lines.append(f"| {r['region']} | {r['requests']:,} | {r['cheapest_static']}{ties} | {sw} |")
    lines += ["", "## Regret of routing on stale times (all reachable pairs)", "",
              "| Region | Series | From → to | Pairs | Mean | p95 | ≥ 5% | Route changes | ETA error, mean abs |",
              "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for r in (x for x in rows if x["kind"] == "regret" and x["group"] == "all"):
        lines.append(f"| {r['region']} | {r['series']} | {r['from']} → {r['to']} | {r['pairs']} | {100 * r['mean']:.2f}% | "
                     f"{100 * r['p95']:.2f}% | {100 * r['share_ge5']:.1f}% | {100 * r['share_changed']:.1f}% | "
                     f"{100 * r['eta_abs_mean']:.1f}% |")
    return "\n".join(lines) + "\n"


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("region", choices=REGIONS)
    r = sub.add_parser("run")
    r.add_argument("region", choices=REGIONS)
    r.add_argument("output", type=Path)
    r.add_argument("--queries", type=int, default=1000)
    r.add_argument("--repeats", type=int, default=3)
    r.add_argument("--threads", type=int, default=2)
    s = sub.add_parser("report")
    s.add_argument("directory", type=Path)
    s.add_argument("--results", type=Path, default=REPO / "results")
    a = p.parse_args()
    if a.command == "fetch":
        fetch(a.region)
    elif a.command == "run":
        run(a.region, a.output, a.queries, a.repeats, a.threads)
    else:
        report(a.directory, a.results)


if __name__ == "__main__":
    main()
