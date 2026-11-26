"""Build the road comparison page from retained measurements and recompute cost boundaries.

q: core-hours per billion requests; u: core-seconds per traffic update.
C = R q / 1e9 + K u / 3600. K*: entry into CCH among scale.BASE at R=1e6;
the all-engine envelope includes RoutingKit and CCH variants separately.
"""

import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

import scale
import traffic

ROOT = Path(__file__).resolve().parents[1]
LABELS = {
    "dijkstra": "Dijkstra", "bidirectional": "Bidirectional Dijkstra",
    "fast_paths-CH": "fast_paths CH, rebuilt", "fast_paths-CH-fixed-order": "fast_paths CH, fixed order",
    "CCH": "Rust CCH", "CCH-4-threads": "Rust CCH, four update threads",
    "CCH-partial": "Rust CCH, partial update", "CCH-perfect": "Rust CCH, perfect",
    "overlay-1024-32768": "Overlay 1,024 / 32,768", "overlay-256-4096-65536": "Overlay 256 / 4,096 / 65,536",
    "routingkit-cch": "RoutingKit CCH", "routingkit-ch": "RoutingKit CH",
}


def read_rows(path):
    raw = path.read_bytes()
    return [json.loads(line) for line in raw.decode("utf-8").splitlines() if line], hashlib.sha256(raw).hexdigest()


def close(a, b):
    return a is b if a is None or b is None else math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-12)


def scale_payload(path):
    """Retain pass values and provenance; recompute K*, f* and the all-engine envelope."""
    rows, sha = read_rows(path)
    sources = [r for r in rows if r["kind"] == "source"]
    ids = [s["size"] for s in sources]
    if not ids or len(set(ids)) != len(ids) or not set(ids) <= set(scale.SIZES):
        raise ValueError("invalid size population")
    sizes = []
    rejected = [s["size"] for s in sources if s.get("timing_contract") != "paths-reverse-update-v1"]
    for src in sorted((s for s in sources if s["size"] not in rejected), key=lambda s: s["nodes"]):
        local = [r for r in rows if r.get("size") == src["size"]]
        eng = {r["method"]: r for r in local if r["kind"] == "engine"}
        if not set(scale.ENGINES) <= set(eng) or not set(eng) <= set(LABELS):
            raise ValueError(f"{src['size']}: incomplete or unknown engine population")
        b = next(r for r in local if r["kind"] == "breakeven" and r["requests"] == 10**6)
        first, switches, k = scale.kstar(eng)
        if (first != b["cheapest_static"] or not close(k, b["k_cch"])
                or len(switches) != len(b["switches"])
                or any(e != v["to"] or not close(x, v["k"]) for (x, e), v in zip(switches, b["switches"]))):
            raise ValueError(f"{src['size']}: cost boundaries do not recompute")
        ups = [r for r in local if r["kind"] == "update"]
        part = next(r for r in local if r["kind"] == "partial")
        if not close(part["f_star"], scale.crossing(*scale.curve(ups))):
            raise ValueError(f"{src['size']}: partial crossover does not recompute")
        engines = {e: {key: v[key] for key in ("q", "u", "q_pass", "u_pass", "u_wall", "prepare_s",
                                                "memory_bytes", "checked")}
                   for e, v in eng.items()}
        for e, v in eng.items():
            if v.get("process_peak_rss_bytes") is not None:
                engines[e]["process_peak_rss_bytes"] = v["process_peak_rss_bytes"]
        rk = next((r for r in local if r["kind"] == "routingkit"), None)
        if rk and "routingkit-cch" in engines:
            engines["routingkit-cch"]["four_threads_wall_s"] = rk["customize4_wall_s"]
        for e, v in engines.items():
            if any(not math.isfinite(v[key]) or v[key] < 0 for key in ("q", "u", "u_wall", "prepare_s")):
                raise ValueError(f"{src['size']}: invalid engine cost")
            if len(v["q_pass"]) != scale.PASSES or len(v["u_pass"]) != scale.PASSES:
                raise ValueError(f"{src['size']}: incomplete pass population")
        ties = [e for e, v in engines.items() if e != first
                and traffic.overlaps(v["q_pass"], engines[first]["q_pass"])]
        sizes.append({"id": src["size"], "label": src["label"], "nodes": src["nodes"], "arcs": src["arcs"],
                      "queries": src["queries"], "reachable": src["reachable_time"],
                      "identity": src["identical_endpoints"], "trees": src["certified_trees"],
                      "raw_sha256": src["raw_sha256"], "machine": src["machine"], "source": src["source"],
                      "answer_kind": src["answer_kind"], "timing_contract": src["timing_contract"],
                      "engines": engines, "k": k, "k_pass": b["k_cch_pass"],
                      "static": first, "static_overlaps": ties, "switches": b["switches"],
                      "passes_agree": b["passes_agree"], "f_star": part["f_star"],
                      "f_star_pass": part["f_star_pass"],
                      "updates": [{key: u[key] for key in ("sequence", "from", "to", "target_f", "f", "changed",
                                                            "partial_s", "full_s", "partial_pass", "full_pass",
                                                            "ratio", "distinguishable")} for u in ups]})
    return {"sha256": sha, "sizes": sizes, "missing": [s for s in scale.SIZES if s not in ids or s in rejected],
            "rejected": rejected,
            "growth": [r for r in rows if r["kind"] == "growth"]}


def traffic_payload(path):
    """Stale-route regret only; previous distance-only timings do not enter this comparison."""
    rows, sha = read_rows(path)
    regions = []
    for src in (r for r in rows if r["kind"] == "source"):
        series = {}
        for name in ("free", "consecutive", "observed"):
            group = [r for r in rows if r["kind"] == "regret" and r["region"] == src["region"]
                     and r["series"] == name and r["group"] == "all"
                     and not (name == "consecutive" and r["from"] == "midday" and r["to"] == "afternoon")]
            if not group or any(r["mean"] < 0 for r in group):
                raise ValueError(f"{src['region']}: missing or invalid regret population")
            series[name] = {"mean": statistics.fmean(r["mean"] for r in group),
                            "pairs": group[0]["pairs"], "transitions": len(group)}
        regions.append({"id": src["region"], "label": "Nationwide" if src["region"] == "korea" else src["region"].title(),
                        "series": series, "raw_sha256": src["raw_sha256"],
                        "machine": src["machine"], "source": src["source"]})
    if not regions:
        raise ValueError("no traffic regions")
    return {"sha256": sha, "regions": regions}


def build_payload(results_dir=ROOT / "results"):
    spath, tpath, ipath = (results_dir / f for f in ("scale.jsonl", "traffic.jsonl", "industry.json"))
    return {"schema": 4, "labels": LABELS, "scale": scale_payload(spath) if spath.exists() else None,
            "traffic": traffic_payload(tpath) if tpath.exists() else None,
            "industry": json.loads(ipath.read_text(encoding="utf-8")) if ipath.exists() else None}


def javascript(payload):
    return "window.DATA = " + json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":")) + ";\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, default=ROOT / "site" / "data.js")
    args = parser.parse_args()
    args.output.write_text(javascript(build_payload()), encoding="utf-8", newline="\n")
    print(f"wrote {args.output.name}")


if __name__ == "__main__":
    main()
