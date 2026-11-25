"""Size and partial-update study: fetch networks of growing size, run the `scale` harness in shards,
summarize.

  python scripts/scale.py fetch SIZE                  data/scale/SIZE.{bin,metrics.bin,meta.json}
  python scripts/scale.py run SIZE MODE OUT [options] one shard: time, partial, memory, routingkit or routingkit-ch
  python scripts/scale.py report DIR                  results/scale.jsonl and results/scale.md

Sizes: the eight cities and Gyeongbuk (one province) from their region files; the union of Seoul,
Incheon, Gyeonggi, Gangwon and Chungbuk; the nation. Region files number their nodes 0..n-1, so the
union is cut from the nationwide graph: its nodes are the nationwide nodes whose coordinates equal
those of a node in one of the five region files, its edges the nationwide edges between two of them.
Source as traffic.py: CAMUS-LAB/data-road_network_eta at the revision pinned in roads.py,
CC BY-NC 4.0, so every result here is non-commercial.

Metrics: free flow (`default_speed_kmh`, a static road-class speed, not a measured one), the eight
weekday p50 slots, then the eight weekend p50 slots, each in traffic.SLOTS order.

Legend (q, u, C, K as traffic.py)
  n     junctions (nodes) of a network
  a, b  exponents of q ∝ nᵃ and u ∝ nᵇ: least squares on log n
  K*    updates per day above which a CCH engine is the cheapest, at R = 10^6 requests per day
  f     changed-arc fraction of an update: arcs whose integer-millisecond weight changed / all arcs
  u(f)  core-seconds of a partial CCH update; u_full of a full one on the same update
  f*    the f at which u(f) = u_full, interpolated linearly in log f between measured updates
"""

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import roads  # noqa: E402
import traffic  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
DATA = REPO / "data" / "scale"
CITIES = ("sejong", "daejeon", "ulsan", "gwangju", "daegu", "busan", "incheon", "seoul")
UNION = ("seoul", "incheon", "gyeonggi", "gangwon", "chungbuk")
SIZES = CITIES + ("gyeongbuk", "union", "korea")
LABELS = {"union": "Seoul, Incheon, Gyeonggi, Gangwon, Chungbuk", "korea": "Nationwide"}
COLUMNS = (("default_speed_kmh",) + tuple(f"weekday_{s}_p50" for s in traffic.SLOTS)
           + tuple(f"weekend_{s}_p50" for s in traffic.SLOTS))
METRICS = ("free",) + tuple(f"weekday {s}" for s in traffic.SLOTS) + tuple(f"weekend {s}" for s in traffic.SLOTS)
ENGINES = ("dijkstra", "bidirectional", "fast_paths-CH", "fast_paths-CH-fixed-order", "CCH", "CCH-4-threads",
           "CCH-partial", "CCH-perfect", "overlay-1024-32768", "overlay-256-4096-65536")
# The engines without the CCH update variants: K* is where CCH becomes the cheapest of these.
BASE = ("dijkstra", "bidirectional", "fast_paths-CH", "fast_paths-CH-fixed-order", "CCH",
        "overlay-1024-32768", "overlay-256-4096-65536")
PREPARED = {"overlay-1024-32768", "overlay-256-4096-65536", "fast_paths-CH-fixed-order", "CCH"}
SAME = ("CCH", "CCH-4-threads", "CCH-partial")
PASSES, SLOT_COUNT = 3, len(traffic.SLOTS)
SEQUENCES = ("weekday-consecutive", "weekday-observed", "weekend-consecutive", "weekend-observed", "synthetic")
F = (0.001, 0.01, 0.1, 0.3, 0.6)
EDGE_COLUMNS = ["source", "target", "length_m", "oneway", "default_speed_kmh", *COLUMNS[1:]]


def fetch(size):
    """Download one size from the pinned files and write it to DATA."""
    if size == "union":
        nodes, edges, source = union()
    else:
        stem = "nationwide/korea" if size == "korea" else f"regions/{size}"
        nodes, nodes_hash = roads.read_table(f"{stem}_road_graph_nodes.parquet", ["node_id", "lat", "lon"])
        edges, edges_hash = roads.read_table(f"{stem}_road_graph_edges.parquet", EDGE_COLUMNS)
        source = {"files": [{"url": roads.URL.format(f"{stem}_road_graph_{t}.parquet"), "sha256": h}
                            for t, h in (("nodes", nodes_hash), ("edges", edges_hash))]}
    source = {"dataset": "CAMUS-LAB/data-road_network_eta", "revision": roads.REVISION, "licence": traffic.LICENCE,
              "size": size, "edge_rows": len(edges), **source}
    traffic.store(DATA, size, nodes, edges, COLUMNS, METRICS, source)


def union():
    """The nationwide nodes at the coordinates of the UNION region nodes, the nationwide edges
    between them, renumbered 0..n-1 in nationwide order; and their provenance."""
    import pandas as pd

    korea, korea_hash = roads.read_table("nationwide/korea_road_graph_nodes.parquet", ["node_id", "lat", "lon"])
    files, keys, matched = [], [], {}
    for region in UNION:
        part, h = roads.read_table(f"regions/{region}_road_graph_nodes.parquet", ["lat", "lon"])
        files.append({"url": roads.URL.format(f"regions/{region}_road_graph_nodes.parquet"), "sha256": h})
        found = part.merge(korea[["lat", "lon"]].drop_duplicates(), on=["lat", "lon"], how="inner")
        matched[region] = {"nodes": len(part), "matched": len(found)}
        if len(found) < 0.99 * len(part):
            raise ValueError(f"{region}: only {len(found)} of {len(part)} nodes found nationwide")
        keys.append(part)
    edges, edges_hash = roads.read_table("nationwide/korea_road_graph_edges.parquet", EDGE_COLUMNS)
    nodes, edges = induce(korea, edges, pd.concat(keys))
    files += [{"url": roads.URL.format(f"nationwide/korea_road_graph_{t}.parquet"), "sha256": h}
              for t, h in (("nodes", korea_hash), ("edges", edges_hash))]
    rule = "nationwide nodes at the exact coordinates of a region node; nationwide edges between them"
    return nodes, edges, {"files": files, "regions": list(UNION), "matched": matched, "rule": rule}


def induce(nodes, edges, keys):
    """The nodes whose (lat, lon) is a row of `keys` and the edges between two of them, renumbered
    0..n-1 in their original order."""
    chosen = nodes.merge(keys.drop_duplicates(), on=["lat", "lon"], how="inner").sort_values("node_id")
    index = np.full(len(nodes), -1, np.int64)
    index[chosen.node_id.to_numpy()] = np.arange(len(chosen))
    u, v = index[edges.source.to_numpy()], index[edges.target.to_numpy()]
    keep = (u >= 0) & (v >= 0)
    edges = edges[keep].assign(source=u[keep], target=v[keep]).reset_index(drop=True)
    return chosen.assign(node_id=np.arange(len(chosen)))[["node_id", "lat", "lon"]].reset_index(drop=True), edges


def machine(repo):
    """CPU, OS, compiler, Git state and build flags of this run."""
    cpu = platform.processor() or platform.machine() or "unknown"
    if sys.platform.startswith("linux"):
        try:
            for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
                if line.lower().startswith("model name"):
                    cpu = line.split(":", 1)[1].strip()
                    break
        except OSError:
            pass
    try:
        memory = next(int(line.split()[1]) * 1024 for line in Path("/proc/meminfo").read_text().splitlines()
                      if line.startswith("MemTotal:"))
    except (OSError, StopIteration, ValueError):
        memory = None
    return {
        "cpu": cpu,
        "os": platform.platform(aliased=True, terse=True),
        "compiler": subprocess.check_output(["rustc", "-Vv"], cwd=repo, text=True).strip(),
        "git_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "git_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip()),
        "rustflags": os.environ.get("RUSTFLAGS") or "default",
        "os_cpu_count": os.cpu_count(),
        "ram_bytes": memory,
    }


def run(size, mode, output, args):
    """Run one shard of the harness on a fetched size; stream rows to OUTPUT.partial, then write
    OUTPUT with provenance on the first row."""
    meta = json.loads((DATA / f"{size}.meta.json").read_text(encoding="utf-8"))
    road, cols = DATA / f"{size}.bin", DATA / f"{size}.metrics.bin"
    for path, key in ((road, "output_sha256"), (cols, "metrics_sha256")):
        if hashlib.sha256(path.read_bytes()).hexdigest() != meta[key]:
            raise ValueError(f"{path.name} does not match its metadata")
    exe = lambda name: str(REPO / "target" / "release" / (name + (".exe" if sys.platform == "win32" else "")))
    rk = DATA / f"{size}.rk.bin"
    if mode == "time":
        cmd = [exe("scale"), road, cols, "time", args.queries, args.passes, args.slots, args.threads]
    elif mode == "partial":
        cmd = [exe("scale"), road, cols, "partial", args.queries, args.threads, rk]
    elif mode == "memory":
        rows = []
        for e in ("none",) + ENGINES:
            rows += stream([exe("scale"), road, cols, "memory", min(20, int(args.queries)), e], output, append=bool(rows))
        return finish(rows, size, meta, output)
    else:
        if mode == "routingkit-ch" and not rk.exists():
            # Certified weekday distances only; the partial study is not needed for the CH.
            stream([exe("scale"), road, cols, "export", args.queries, args.threads, rk], output.with_suffix(".export"))
        cmd = [REPO / "external" / "routingkit" / "routingkit", rk, "cch" if mode == "routingkit" else "ch"]
    finish(stream([str(c) for c in cmd], output), size, meta, output)


def stream(cmd, output, append=False):
    """Run cmd, copying each stdout row to OUTPUT.partial as it arrives; return the rows."""
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_name(output.name + ".partial")
    lines = []
    with subprocess.Popen([str(c) for c in cmd], cwd=REPO, stdout=subprocess.PIPE, text=True) as proc, \
            partial.open("a" if append else "w", encoding="utf-8", newline="\n") as f:
        for line in proc.stdout:
            lines.append(line)
            f.write(line)
            f.flush()
    if proc.returncode:
        raise subprocess.CalledProcessError(proc.returncode, proc.args)
    return [json.loads(line) for line in lines]


def finish(rows, size, meta, output):
    try:
        import resource  # POSIX only; ru_maxrss is in KiB on Linux
        peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024
    except ImportError:
        peak = None
    rows[0].update(source=meta, machine=machine(REPO), process_peak_rss_bytes=peak)
    for row in rows:
        row["size"] = size
    output.write_text("".join(json.dumps(r, sort_keys=True, allow_nan=False) + "\n" for r in rows),
                      encoding="utf-8", newline="\n")


def validate(rows):
    """Check one size's merged shards: a complete, correct timing population over every pass and
    weekday slot, the partial-update population, and a memory row per engine."""
    if any(r.get("wrong") for r in rows):
        raise ValueError("wrong answer")
    graphs = [r for r in rows if r["kind"] == "graph"]
    if len({r["nodes"] for r in graphs}) != 1 or len({r["queries"] for r in graphs if r["mode"] == "time"}) != 1:
        raise ValueError("shards disagree on the graph or workload")
    q = next(r["queries"] for r in graphs if r["mode"] == "time")
    slots = [r for r in rows if r["kind"] == "slot"]
    seen = [(r["method"], r["pass"], r["slot"]) for r in slots]
    if sorted(seen) != sorted((e, p, s) for e in ENGINES for p in range(PASSES) for s in range(SLOT_COUNT)):
        raise ValueError("incomplete or duplicated timing population")
    for e in ENGINES:
        for p in range(PASSES):
            if sum(r["queries"] for r in slots if r["method"] == e and r["pass"] == p) != q:
                raise ValueError("a pass does not cover every query once")
    # Per pass: consecutive and observed updates on weekdays and weekends, and the synthetic ones.
    ups = [r for r in rows if r["kind"] == "update"]
    if len(ups) != PASSES * (2 * (SLOT_COUNT + 4) + 4 * len(F)) or not all(r["identical"] for r in ups):
        raise ValueError("incomplete partial-update population")
    if {r["method"] for r in rows if r["kind"] == "memory"} != {"none", *ENGINES}:
        raise ValueError("missing memory rows")
    if not PREPARED <= {r["method"] for r in rows if r["kind"] == "prepare"}:
        raise ValueError("missing preparation rows")
    return q, sum(r["mode"] == "time" for r in graphs)


def engines(rows):
    """q, u and their per-pass values for each engine of one size, as traffic.engines; u_wall for
    the threaded engine, preparation seconds (mean over shards) and peak memory over baseline.
    Engines of one SAME group answer with one query routine on bit-identical weights, so their q
    pools all their timed queries; q_own keeps each engine's own."""
    out = {}
    memory = {r["method"]: r for r in rows if r["kind"] == "memory"}
    rate = lambda s: sum(r["query_s"] for r in s) / sum(r["queries"] for r in s) * 1e9 / 3600
    for e in ENGINES:
        slot = [r for r in rows if r["kind"] == "slot" and r["method"] == e]
        pool = [r for r in rows if r["kind"] == "slot" and r["method"] in (SAME if e in SAME else (e,))]
        q_pass = [rate([r for r in pool if r["pass"] == p]) for p in range(PASSES)]
        u_slot = [float(np.median([r["update_s"] for r in slot if r["slot"] == s])) for s in range(SLOT_COUNT)]
        u_pass = [float(np.mean([r["update_s"] for r in slot if r["pass"] == p])) for p in range(PASSES)]
        wall = [float(np.median([r["update_wall_s"] for r in slot if r["slot"] == s])) for s in range(SLOT_COUNT)]
        prep = [r["seconds"] for r in rows if r["kind"] == "prepare"
                and r["method"] == ("CCH" if e.startswith("CCH") else e)]
        m = memory.get(e, {})
        out[e] = {
            "q": rate(pool), "q_pass": q_pass, "q_own": rate(slot), "u": float(np.mean(u_slot)), "u_pass": u_pass, "u_wall": float(np.mean(wall)),
            "prepare_s": float(np.mean(prep)) if prep else 0.0, "prepare_runs": len(prep),
            "memory_bytes": (m["peak_rss_bytes"] - m["baseline_rss_bytes"])
            if m.get("peak_rss_bytes") is not None and m.get("baseline_rss_bytes") is not None else None,
            "checked": sum(r["checked"] for r in slot),
        }
    return out


def kstar(eng, requests=10**6, p=None):
    """For R requests per day (pass p's values when given): the cheapest engine at K = 0 and the
    switches of the envelope over every engine; and K*, the K above which CCH is the cheapest of the
    BASE engines (0 if it is at K = 0, None if never)."""
    first, switches = traffic.breakeven(eng, requests, p)
    head, base = traffic.breakeven({e: eng[e] for e in BASE}, requests, p)
    k = 0.0 if head == "CCH" else next((x for x, e in base if e == "CCH"), None)
    return first, switches, k


def fit(n, y):
    """Least-squares slope and intercept of log y on log n, and the slope's standard error."""
    x, y = np.log(np.asarray(n, float)), np.log(np.asarray(y, float))
    a, c = np.polyfit(x, y, 1)
    r = y - (a * x + c)
    se = float(np.sqrt(r @ r / (len(x) - 2) / ((x - x.mean()) @ (x - x.mean())))) if len(x) > 2 else None
    return float(a), float(c), se


def crossing(f, ratio):
    """f*: where ratio(f) = u(f)/u_full first reaches 1, linear in log f between neighbouring
    points sorted by f; 0 if already above 1 at the smallest f, None if never."""
    pts = sorted(zip(f, ratio))
    if pts[0][1] >= 1:
        return 0.0
    for (f0, r0), (f1, r1) in zip(pts, pts[1:]):
        if r0 < 1 <= r1:
            t = (1 - r0) / (r1 - r0)
            return float(np.exp(np.log(f0) + t * (np.log(f1) - np.log(f0))))
    return None


def curve(ups, p=None):
    """The points (f, u(f)/u_full) of the crossover: one per synthetic f, its mean over the four
    observed transitions, and one for the dataset transitions that change an arc, their mean f and
    ratio; from pass p's times when given."""
    r = (lambda u: u["ratio"]) if p is None else (lambda u: u["partial_pass"][p] / u["full_pass"][p])
    groups = [[u for u in ups if u["target_f"] == f] for f in F]
    groups.append([u for u in ups if u["sequence"] != "synthetic" and u["changed"]])
    return ([float(np.mean([u["f"] for u in g])) for g in groups],
            [float(np.mean([r(u) for u in g])) for g in groups])


def updates(rows):
    """One row per update a → b of the partial study: f, mean and per-pass partial and full
    core-seconds, their ratio and whether the per-pass ranges are distinguishable."""
    ups = [r for r in rows if r["kind"] == "update"]
    keys = sorted({(r["sequence"], r["from"], r["to"], r["target_f"]) for r in ups},
                  key=lambda k: (SEQUENCES.index(k[0]), k[3] or 0, k[1]))
    out = []
    for seq, a, b, tf in keys:
        g = sorted((r for r in ups if (r["sequence"], r["from"], r["to"], r["target_f"]) == (seq, a, b, tf)),
                   key=lambda r: r["pass"])
        part, full = [r["partial_s"] for r in g], [r["full_s"] for r in g]
        out.append({"sequence": seq, "from": METRICS[a], "to": METRICS[b], "target_f": tf, "f": g[0]["f"],
                    "changed": g[0]["changed"], "recomputed": g[0]["recomputed"], "edges": g[0]["edges"],
                    "partial_s": float(np.mean(part)), "full_s": float(np.mean(full)),
                    "partial_pass": part, "full_pass": full, "ratio": float(np.mean(part) / np.mean(full)),
                    "distinguishable": not traffic.overlaps(part, full),
                    "checked": sum(r["checked"] for r in g)})
    return out


def routingkit(rows):
    """One summary row of RoutingKit's engines for a size, measured as `engines` measures ours: its
    CCH (preparation, customization on one and four threads, request cost, partial ratios) and its
    CH (rebuild per metric, request cost); none when the size has no RoutingKit rows."""
    prep = [r for r in rows if r["kind"] == "routingkit_prepare"]
    ch = [r for r in rows if r["kind"] == "routingkit_ch_slot"]
    if not prep or len(ch) != PASSES * SLOT_COUNT or any(r["wrong"] for r in ch):
        raise ValueError("incomplete or wrong RoutingKit population")
    rate = lambda s: sum(r["query_s"] for r in s) / sum(r["queries"] for r in s) * 1e9 / 3600
    slots = [r for r in rows if r["kind"] == "routingkit_slot"]
    ups = [r for r in rows if r["kind"] == "routingkit_update"]
    if len(slots) != PASSES * SLOT_COUNT or len(ups) != PASSES * 4 * len(F) or any(r["wrong"] for r in slots + ups):
        raise ValueError("incomplete or wrong RoutingKit population")
    per_slot = lambda key: float(np.mean([np.median([r[key] for r in slots if r["slot"] == s])
                                          for s in range(SLOT_COUNT)]))
    p = prep[0]
    return [{"kind": "routingkit", "commit": p["commit"], "order_s": p["order_s"], "build_s": p["build_s"],
             "prepare_s": p["order_s"] + p["build_s"], "customize_s": per_slot("customize_s"),
             "customize4_wall_s": per_slot("customize4_wall_s"),
             "q": sum(r["query_s"] for r in slots) / sum(r["queries"] for r in slots) * 1e9 / 3600,
             "queries": sum(r["queries"] for r in slots), "query_s": sum(r["query_s"] for r in slots),
             "checked": sum(r["checked"] for r in slots) + sum(r["checked"] for r in ch),
             "ch_build_s": float(np.mean([np.median([r["build_s"] for r in ch if r["slot"] == s])
                                          for s in range(SLOT_COUNT)])),
             "ch_q": rate(ch),
             "ratio_by_f": {str(f): float(np.mean([r["partial_s"] / r["full_s"] for r in ups if r["target_f"] == f]))
                            for f in F}}]


def plan(selected="all"):
    """CI shards as {name, size, modes, passes, slots, queries}. A city or Gyeongbuk runs in one job;
    the union splits its timing by pass, the nation by pass and half of the weekday slots, each
    with the partial study, RoutingKit and memory in separate jobs."""
    out = []
    for size in SIZES if selected == "all" else (selected,):
        q = "200" if size == "korea" else "1000"
        every, slots = "0,1,2", "0,1,2,3,4,5,6,7"
        if size in CITIES + ("gyeongbuk",):
            out.append({"name": size, "size": size, "modes": "time,partial,routingkit,routingkit-ch,memory", "passes": every,
                        "slots": slots, "queries": q})
            continue
        halves = ("0,1,2,3", "4,5,6,7") if size == "korea" else (slots,)
        out += [{"name": f"{size}-time-{p}-{h[0]}", "size": size, "modes": "time", "passes": str(p), "slots": h,
                 "queries": q} for p in range(PASSES) for h in halves]
        out += [{"name": f"{size}-partial", "size": size, "modes": "partial,routingkit", "passes": every,
                 "slots": slots, "queries": q},
                {"name": f"{size}-routingkit-ch", "size": size, "modes": "routingkit-ch", "passes": every,
                 "slots": slots, "queries": q},
                {"name": f"{size}-memory", "size": size, "modes": "memory", "passes": every, "slots": slots,
                 "queries": q}]
    return out


def summarize(files):
    """Committed summary rows for one size from its raw shard files."""
    raws = [(f, f.read_bytes()) for f in files]
    rows = [json.loads(line) for _, raw in raws for line in raw.decode("utf-8").splitlines()]
    # Runs before the refocus also timed one-level overlays; they are not part of the comparison.
    rows = [r for r in rows if r["kind"] not in ("slot", "prepare", "memory") or r["method"] in ENGINES + ("none", "CCH")]
    q, shards = validate(rows)
    size = rows[0]["size"]
    g = next(r for r in rows if r["kind"] == "graph" and r["mode"] == "time")
    part = next(r for r in rows if r["kind"] == "graph" and r["mode"] == "partial")
    cch = next(r for r in rows if r["kind"] == "prepare" and r["method"] == "CCH")
    out = [{"kind": "source", "size": size, "label": LABELS.get(size, size.capitalize()), "nodes": g["nodes"],
            "arcs": g["arcs"], "queries": q, "time_shards": shards, "identical_endpoints": g["identical_endpoints"],
            "reachable_time": [x for r in rows if r["kind"] == "graph" and r["mode"] == "time" for x in r["reachable"]],
            "reachable_partial": part["reachable"],
            "certified_trees": sum(r["certified_trees"] for r in rows if r["kind"] == "graph"),
            "cch_edges": cch["edges"], "cch_triangles": cch["triangles"], "cch_tree_height": cch["tree_height"],
            "raw_sha256": {f.name: hashlib.sha256(raw).hexdigest() for f, raw in raws},
            "source": g["source"], "machine": g["machine"]}]
    eng = engines(rows)
    out += [{"kind": "engine", "size": size, "method": e, **v} for e, v in eng.items()]
    for r in traffic.REQUESTS:
        first, switches, k = kstar(eng, r)
        passes = [kstar(eng, r, p) for p in range(PASSES)]
        out.append({"kind": "breakeven", "size": size, "requests": r, "cheapest_static": first,
                    "switches": [{"k": x, "to": e} for x, e in switches], "k_cch": k,
                    "k_cch_pass": [x[2] for x in passes],
                    "passes_agree": all(x[0] == first and [e for _, e in x[1]] == [e for _, e in switches]
                                        for x in passes)})
    ups = updates(rows)
    out += [{"kind": "update", "size": size, **u} for u in ups]
    f, ratio = curve(ups)
    out.append({"kind": "partial", "size": size, "f": f, "ratio": ratio, "f_star": crossing(f, ratio),
                "f_star_pass": [crossing(*curve(ups, p)) for p in range(PASSES)],
                "dataset_f": [float(min(u["f"] for u in ups if u["sequence"] != "synthetic" and u["changed"])),
                              float(max(u["f"] for u in ups if u["sequence"] != "synthetic" and u["changed"]))]})
    out += [{**r, "size": size} for r in routingkit(rows)]
    return out


def growth(rows):
    """Exponents a (q) and b (u) per engine over all sizes, with per-pass slopes and standard
    errors, and the exponent of K*."""
    sizes = [r for r in rows if r["kind"] == "source"]
    n = [r["nodes"] for r in sizes]
    if len(n) < 3:
        return []
    eng = {(r["size"], r["method"]): r for r in rows if r["kind"] == "engine"}
    out = []
    for e in ENGINES:
        v = [eng[s["size"], e] for s in sizes]
        row = {"kind": "growth", "method": e, "sizes": len(n)}
        for key, name in (("q", "a"), ("u", "b")):
            y = [x[key] for x in v]
            if min(y) <= 0:
                continue
            slope, _, se = fit(n, y)
            row[name] = slope
            row[name + "_se"] = se
            row[name + "_pass"] = [fit(n, [x[key + "_pass"][p] for x in v])[0] for p in range(PASSES)]
        out.append(row)
    k = {r["size"]: r["k_cch"] for r in rows if r["kind"] == "breakeven" and r["requests"] == 10**6}
    pts = [(s["nodes"], k[s["size"]]) for s in sizes if k[s["size"]]]
    if len(pts) >= 2:
        slope, c0, se = fit(*zip(*pts))
        out.append({"kind": "growth", "method": "K*", "sizes": len(pts), "c": slope, "c_se": se,
                    "log_intercept": c0})
    return out


def report(directory, results):
    files = sorted(directory.glob("*.jsonl"))
    by = {}
    for f in files:
        size = json.loads(f.read_text(encoding="utf-8").splitlines()[0])["size"]
        by.setdefault(size, []).append(f)
    if not set(by) <= set(SIZES) or not by:
        raise ValueError(f"unexpected sizes {sorted(by)}")
    rows = [r for s in SIZES if s in by for r in summarize(by[s])]
    rows += growth(rows)
    (results / "scale.jsonl").write_text(
        "".join(json.dumps(r, sort_keys=True, allow_nan=False) + "\n" for r in rows), encoding="utf-8", newline="\n")
    (results / "scale.md").write_text(markdown(rows), encoding="utf-8", newline="\n")


def markdown(rows):
    src = [r for r in rows if r["kind"] == "source"]
    eng = {(r["size"], r["method"]): r for r in rows if r["kind"] == "engine"}
    lines = ["# Size and partial-update study", "",
             "Generated by `scripts/scale.py report` from per-size raw shard files; Korea Road Network ETA, "
             "CC BY-NC 4.0 (non-commercial). Integer-millisecond travel times; q in core-hours per 10⁹ requests, "
             "u in core-seconds per update, ranges over three passes.", "",
             "## Engines by size", "",
             "| Size | Nodes | Engine | q (pass range) | u (pass range) | u wall | Prepare, s | Memory, MB |",
             "| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |"]
    for s in src:
        for e in ENGINES:
            v = eng[s["size"], e]
            mem = f"{v['memory_bytes'] / 1e6:.0f}" if v["memory_bytes"] is not None else "n/a"
            lines.append(f"| {s['label']} | {s['nodes']:,} | {e} | {v['q']:.3g} ({min(v['q_pass']):.3g}–"
                         f"{max(v['q_pass']):.3g}) | {v['u']:.3g} ({min(v['u_pass']):.3g}–{max(v['u_pass']):.3g}) "
                         f"| {v['u_wall']:.3g} | {v['prepare_s']:.3g} | {mem} |")
    lines += ["", "## Break-even at 10⁶ requests per day", "",
              "| Size | Nodes | Cheapest without updates | K* (CCH cheapest above), pass range | Switches |",
              "| --- | ---: | --- | ---: | --- |"]
    for r in (x for x in rows if x["kind"] == "breakeven" and x["requests"] == 10**6):
        s = next(x for x in src if x["size"] == r["size"])
        kp = [k for k in r["k_cch_pass"] if k is not None]
        k = "never" if r["k_cch"] is None else f"{r['k_cch']:.3g}" + (f" ({min(kp):.3g}–{max(kp):.3g})" if kp else "")
        sw = "; ".join(f"{x['k']:.3g} → {x['to']}" for x in r["switches"]) or "none"
        lines.append(f"| {s['label']} | {s['nodes']:,} | {r['cheapest_static']} | {k} | {sw} |")
    lines += ["", "## Growth exponents over all sizes", "",
              "| Engine | a (q ∝ nᵃ) ± s.e. | a per pass | b (u ∝ nᵇ) ± s.e. | b per pass |", "| --- | ---: | --- | ---: | --- |"]
    for r in (x for x in rows if x["kind"] == "growth" and x["method"] != "K*"):
        cell = lambda k: (f"{r[k]:.3f} ± {r[k + '_se']:.3f}", ", ".join(f"{x:.3f}" for x in r[k + "_pass"])) if k in r else ("–", "–")
        lines.append(f"| {r['method']} | {cell('a')[0]} | {cell('a')[1]} | {cell('b')[0]} | {cell('b')[1]} |")
    for r in (x for x in rows if x["kind"] == "growth" and x["method"] == "K*"):
        lines += ["", f"K* ∝ n^c with c = {r['c']:.3f} ± {r['c_se']:.3f} over {r['sizes']} sizes."]
    lines += ["", "## Partial CCH updates", "",
              "| Size | Sequence | From → to | f | Partial, s | Full, s | Partial / full | Distinguishable |",
              "| --- | --- | --- | ---: | ---: | ---: | ---: | --- |"]
    for r in (x for x in rows if x["kind"] == "update"):
        lines.append(f"| {r['size']} | {r['sequence']} | {r['from']} → {r['to']} | {100 * r['f']:.3g}% | "
                     f"{r['partial_s']:.3g} | {r['full_s']:.3g} | {r['ratio']:.3g} | {'yes' if r['distinguishable'] else 'no'} |")
    lines += ["", "| Size | f* (partial = full) | f* per pass | Dataset transitions, f |", "| --- | ---: | --- | --- |"]
    for r in (x for x in rows if x["kind"] == "partial"):
        fs = lambda x: "never" if x is None else f"{100 * x:.3g}%"
        lines.append(f"| {r['size']} | {fs(r['f_star'])} | {', '.join(fs(x) for x in r['f_star_pass'])} | "
                     f"{100 * r['dataset_f'][0]:.3g}–{100 * r['dataset_f'][1]:.3g}% |")
    rk = [r for r in rows if r["kind"] == "routingkit"]
    if rk:
        lines += ["", "## RoutingKit (C++)", "",
                  "| Size | CCH prepare, s | CCH customize, s | 4 threads wall, s | CCH q | CH rebuild, s | CH q | Checked |",
                  "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
        for r in rk:
            lines.append(f"| {r['size']} | {r['prepare_s']:.3g} | {r['customize_s']:.3g} | {r['customize4_wall_s']:.3g} | "
                         f"{r['q']:.3g} | {r['ch_build_s']:.3g} | {r['ch_q']:.3g} | {r['checked']} |")
    return "\n".join(lines) + "\n"


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("size", choices=SIZES)
    r = sub.add_parser("run")
    r.add_argument("size", choices=SIZES)
    r.add_argument("mode", choices=("time", "partial", "memory", "routingkit", "routingkit-ch"))
    r.add_argument("output", type=Path)
    r.add_argument("--queries", default="1000")
    r.add_argument("--passes", default="0,1,2")
    r.add_argument("--slots", default="0,1,2,3,4,5,6,7")
    r.add_argument("--threads", default="4")
    s = sub.add_parser("report")
    s.add_argument("directory", type=Path)
    s.add_argument("--results", type=Path, default=REPO / "results")
    m = sub.add_parser("plan")
    m.add_argument("selected", choices=("all",) + SIZES)
    a = p.parse_args()
    if a.command == "fetch":
        fetch(a.size)
    elif a.command == "run":
        run(a.size, a.mode, a.output, a)
    elif a.command == "plan":
        print(json.dumps({"include": plan(a.selected)}))
    else:
        report(a.directory, a.results)


if __name__ == "__main__":
    main()
