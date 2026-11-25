"""Run and summarize the exact metro routing benchmark."""

import argparse
import hashlib
import json
import math
import os
import platform
import re
import subprocess
import sys
from pathlib import Path

CITIES = {
    "sejong", "seoul", "busan", "daegu", "incheon", "gwangju",
    "daejeon", "ulsan", "london-large", "new_york-large", "tokyo-large",
}
BOUNDS = (256, 1024, 4096)
KOREA_REVISION = "b111710756ee5727a0fbe3f6203c0cc2f93de2be"
WORLD_REVISION = "9b0548fc99a53ce509e4458748c2246ae8884fe4"


def positive(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        raise ValueError(f"{label} must be finite and positive")


def validate(records, city, query_count=None, repeats=None, threads=None):
    """Validate exact graph, build, latency, and throughput populations for one city."""
    _validate_city_records(records, city)
    query_count, repeats, threads = _workload(records[0], query_count, repeats, threads)

    methods = {"dijkstra", "bidirectional"} | {f"{model}-{u}" for model in ("Graph", "Hypergraph") for u in BOUNDS}
    builds, latencies, throughputs = {}, {}, {}
    graphs = {s: [] for s in (0, 1000)}
    for row in records:
        kind, scale = row["kind"], row["scale"]
        if kind == "graph":
            graphs[scale].append(row)
            _validate_graph(row, city, query_count, repeats, threads, scale)
        elif kind == "build":
            key = (scale, row.get("method"))
            if key in builds:
                raise ValueError("duplicate build row")
            builds[key] = row
            _validate_build(row, repeats, query_count)
        elif kind == "latency":
            key = (scale, row.get("method"))
            if key in latencies:
                raise ValueError("duplicate latency row")
            latencies[key] = row
            _validate_latency(row, scale, repeats, query_count)
        elif kind == "throughput":
            key = (scale, row.get("method"), row.get("threads"))
            if key in throughputs:
                raise ValueError("duplicate throughput row")
            throughputs[key] = row
            _validate_throughput(row, repeats, query_count)
        else:
            raise ValueError(f"unknown result kind {kind!r}")

    _validate_populations(graphs, builds, latencies, throughputs, methods, query_count, repeats, threads)
    return records


def _validate_city_records(records, city):
    if not records:
        raise ValueError("result is empty")
    if any(not _finite_record(row) for row in records):
        raise ValueError("result contains an empty string or non-finite value")
    if any(row.get("city") != city or row.get("scale") not in (0, 1000) for row in records):
        raise ValueError("unexpected city or scale")
    if len([r for r in records if r["kind"] == "graph"]) != 2:
        raise ValueError("expected one graph row per scale")


def _workload(first, query_count, repeats, threads):
    if query_count is None:
        query_count = first.get("queries")
    if repeats is None:
        repeats = first.get("repeats")
    if threads is None:
        threads = first.get("threads")
    for value, label in ((query_count, "queries"), (repeats, "repeats"), (threads, "threads")):
        positive(value, label)
    if not all(isinstance(value, int) for value in (query_count, repeats, threads)):
        raise ValueError("queries, repeats, and threads must be integers")
    return query_count, repeats, threads


def _finite_record(value):
    if isinstance(value, float):
        return math.isfinite(value)
    if isinstance(value, str):
        return bool(value.strip())
    if isinstance(value, dict):
        return all(_finite_record(v) for v in value.values())
    if isinstance(value, list):
        return all(_finite_record(v) for v in value)
    return True

def _validate_graph(row, city, query_count, repeats, threads, scale):
    if row.get("certified_trees") != 3 or row.get("queries") != query_count or row.get("repeats") != repeats or row.get("threads") != threads:
        raise ValueError("invalid graph certification or workload")
    expected_metric = "native seconds" if scale == 0 else "max(1, round(scale * seconds))"
    if row.get("metric") != expected_metric:
        raise ValueError("unexpected graph weight scale")
    if row.get("certified_queries") != query_count:
        raise ValueError("invalid certified query population")
    positive(row.get("nodes"), "graph nodes")
    positive(row.get("arcs"), "graph arcs")
    for key in ("reverse_s",):
        positive(row.get(key), key)
    _validate_source(row, city)
    _validate_machine(row.get("machine"))
    _validate_query_details(row, query_count)


def _validate_source(row, city):
    source = row.get("source")
    if (not isinstance(source, dict) or source.get("output_sha256") != row.get("binary_sha256")
            or not isinstance(row.get("binary_sha256"), str)
            or not re.fullmatch(r"[0-9a-f]{64}", row["binary_sha256"])):
        raise ValueError("source binary hash mismatch")
    if (source.get("weight_units", source.get("native_units")) != "seconds"
            or source.get("node_count") != row.get("nodes")
            or not isinstance(source.get("arc_count"), int) or source["arc_count"] < row["arcs"]):
        raise ValueError("source units or node count do not match graph")
    if city in CITIES - {"london-large", "new_york-large", "tokyo-large"}:
        expected_source = ("CAMUS-LAB/data-road_network_eta", KOREA_REVISION, city)
        actual_source = (source.get("dataset"), source.get("revision"), source.get("region"))
    else:
        base_city = city.removesuffix("-large")
        expected_source = ("https://huggingface.co/datasets/HBNU-NLP/InstructOSM", WORLD_REVISION, base_city, "large")
        actual_source = (source.get("dataset"), source.get("revision"), source.get("city"), source.get("scale"))
    if actual_source != expected_source:
        raise ValueError("source city or pinned dataset revision mismatch")


def _validate_machine(machine):
    if not isinstance(machine, dict) or any(not isinstance(machine.get(k), str) or not machine[k]
                                             for k in ("cpu", "os", "compiler", "git_sha", "rustflags")):
        raise ValueError("missing machine provenance")
    for key in ("os_cpu_count", "ram_bytes"):
        if machine.get(key) is not None:
            positive(machine[key], key)
    if not isinstance(machine.get("git_dirty"), bool):
        raise ValueError("missing git dirty state")


def _validate_query_details(row, query_count):
    pairs, reference = row.get("pairs"), row.get("reference")
    if not isinstance(pairs, list) or len(pairs) != query_count or not isinstance(reference, list) or len(reference) != query_count:
        raise ValueError("invalid certified query details")
    for pair, answer in zip(pairs, reference):
        if (not isinstance(pair, list) or len(pair) != 2
                or any(not isinstance(v, int) or not 0 <= v < row["nodes"] for v in pair)):
            raise ValueError("invalid certified query endpoints")
        if answer is not None and (not isinstance(answer, (int, float)) or not math.isfinite(answer) or answer < 0):
            raise ValueError("invalid certified query reference")


def _validate_build(row, repeats, query_count):
    positive(row.get("serial_s"), "serial build seconds")
    if row.get("method") != "fast_paths-CH":
        positive(row.get("parallel_s"), "parallel build seconds")
        for key in ("serial_trials_s", "parallel_trials_s"):
            trials = row.get(key)
            if not isinstance(trials, list) or len(trials) != repeats:
                raise ValueError("invalid overlay build trial population")
            for seconds in trials:
                positive(seconds, key)
    if row.get("method") != "fast_paths-CH" and (row.get("wrong") != 0 or row.get("parity_queries") != query_count):
        raise ValueError("invalid overlay build correctness")


def _validate_latency(row, scale, repeats, query_count):
    if row.get("queries") != query_count or row.get("repeats") != repeats or row.get("wrong") != 0:
        raise ValueError("invalid latency correctness or workload")
    for metric in ("mean_us", "p50_us", "p95_us"):
        positive(row.get(metric), metric)
    if scale == 1000:
        positive(row.get("paired_ratio_to_ch"), "paired ratio to CH")
    trials = row.get("trial_mean_us")
    if not isinstance(trials, list) or len(trials) != repeats:
        raise ValueError("invalid latency trial population")
    for value in trials:
        positive(value, "latency trial mean")


def _validate_throughput(row, repeats, query_count):
    if row.get("queries") != query_count or row.get("repeats") != repeats or row.get("wrong") != 0:
        raise ValueError("invalid throughput correctness or workload")
    positive(row.get("median_qps"), "median qps")
    if len(row.get("trial_seconds", [])) != repeats:
        raise ValueError("invalid throughput trial population")
    for seconds in row["trial_seconds"]:
        positive(seconds, "throughput trial seconds")


def _validate_populations(graphs, builds, latencies, throughputs, methods, query_count, repeats, threads):
    if any(len(rows) != 1 for rows in graphs.values()):
        raise ValueError("expected one graph row per scale")
    if graphs[0][0]["pairs"] != graphs[1000][0]["pairs"]:
        raise ValueError("scale query pairs differ")
    if graphs[0][0]["machine"] != graphs[1000][0]["machine"] or graphs[0][0]["binary_sha256"] != graphs[1000][0]["binary_sha256"]:
        raise ValueError("scale provenance or input binary differs")
    for scale in (0, 1000):
        expected_methods = methods | ({"fast_paths-CH"} if scale == 1000 else set())
        expected_builds = expected_methods - {"dijkstra", "bidirectional"}
        if {m for s, m in builds if s == scale} != expected_builds:
            raise ValueError("missing or extra build methods")
        if {m for s, m in latencies if s == scale} != expected_methods:
            raise ValueError("missing or extra latency methods")
        if {m for s, m, _ in throughputs if s == scale} != expected_methods:
            raise ValueError("missing or extra throughput methods")
        for method in expected_methods:
            _validate_method_workers(scale, method, expected_builds, builds, latencies, throughputs,
                                     query_count, repeats, threads)


def _validate_method_workers(scale, method, expected_builds, builds, latencies, throughputs,
                             query_count, repeats, threads):
    latency = latencies[scale, method]
    if method in expected_builds:
        build = builds[scale, method]
        if method != "fast_paths-CH" and build.get("parity_queries") != query_count:
            raise ValueError("overlay build parity population mismatch")
    if method == "fast_paths-CH" and scale != 1000:
        raise ValueError("CH is only expected at scaled weights")
    if latency.get("queries") != query_count or latency.get("repeats") != repeats:
        raise ValueError("latency population mismatch")
    expected_workers = {1, threads}
    if threads == 1:
        expected_workers = {1}
    if {w for s, m, w in throughputs if s == scale and m == method} != expected_workers:
        raise ValueError("missing or extra throughput thread counts")


def command_run(args):
    for value, label in ((args.queries, "queries"), (args.threads, "threads"), (args.repeats, "repeats")):
        positive(value, label)
    if args.threads > args.queries:
        raise ValueError("threads must not exceed queries")
    source_path = args.file
    sidecar = source_path.with_suffix(".meta.json")
    metadata = json.loads(sidecar.read_text(encoding="utf-8"))
    binary = source_path.read_bytes()
    digest = hashlib.sha256(binary).hexdigest()
    if digest != metadata.get("output_sha256"):
        raise ValueError("input binary does not match source metadata")
    repo = Path(__file__).resolve().parents[1]
    executable = repo / "target" / "release" / ("metro.exe" if os.name == "nt" else "metro")
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
    provenance = {
        "cpu": cpu,
        "os": platform.platform(aliased=True, terse=True),
        "compiler": subprocess.check_output(["rustc", "-Vv"], cwd=repo, text=True).strip(),
        "git_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "git_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip()),
        "rustflags": os.environ.get("RUSTFLAGS") or "default",
        "os_cpu_count": os.cpu_count(),
        "ram_bytes": memory,
    }
    records = []
    for scale in (0, 1000):
        print(f"{source_path.stem}: {args.queries} queries, scale {scale}", flush=True)
        try:
            output = subprocess.run(
                [str(executable), str(source_path), str(args.queries), str(args.threads), str(scale), str(args.repeats)],
                cwd=repo, check=True, capture_output=True, text=True,
            ).stdout
        except subprocess.CalledProcessError as exc:
            print(exc.stderr, file=sys.stderr)
            raise
        for line in output.splitlines():
            row = json.loads(line)
            row.update(city=source_path.stem, scale=scale)
            row["file"] = source_path.name
            row["binary_sha256"] = digest
            if row["kind"] == "graph":
                row["machine"] = provenance
            records.append(row)
    validate(records, source_path.stem, args.queries, args.repeats, args.threads)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    partial = args.output.with_suffix(".partial")
    partial.write_text("".join(json.dumps(r, sort_keys=True, allow_nan=False) + "\n" for r in records), encoding="utf-8")
    partial.replace(args.output)


def load_cities(source):
    results = {}
    if source.is_file():
        for number, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                continue
            row = json.loads(line)
            city = row.get("city")
            if city not in CITIES:
                raise ValueError(f"unknown city at line {number}")
            results.setdefault(city, []).append(row)
    elif source.is_dir():
        for path in source.glob("*.jsonl"):
            city = path.stem
            if city in results:
                raise ValueError(f"duplicate city file {city}")
            results[city] = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    else:
        raise ValueError("report source must be a JSONL file or directory")
    for city, records in results.items():
        validate(records, city)
    if set(results) != CITIES:
        raise ValueError(f"expected exactly these cities: {', '.join(sorted(CITIES))}")
    return results


def validate_report_set(results):
    if set(results) != CITIES:
        raise ValueError(f"expected exactly these cities: {', '.join(sorted(CITIES))}")
    anchor = None
    for city, records in results.items():
        validate(records, city)
        graph = next(row for row in records if row["kind"] == "graph" and row["scale"] == 0)
        machine = graph["machine"]
        if machine["git_dirty"]:
            raise ValueError(f"refusing publication report for dirty worktree: {city}")
        key = (graph["queries"], graph["repeats"], graph["threads"], graph["seed"], machine["git_sha"], machine["rustflags"])
        if anchor is not None and key != anchor:
            raise ValueError("cross-city query, repeat, thread, seed, git SHA, or RUSTFLAGS differ")
        anchor = key
    return anchor


def report_text(results):
    query_count, repeats, threads, seed, git_sha, rustflags = validate_report_set(results)
    methods = sorted({"dijkstra", "bidirectional"} | {f"{model}-{u}" for model in ("Graph", "Hypergraph") for u in BOUNDS})
    lines = ["# Metro routing benchmark", "", "Latency tables show mean, p50, and p95 microseconds across per-query mean latencies; each query mean averages the repeated passes. Native and rounded weights are separate. Rankings use p50 only.",
             "Native weights are travel seconds. Rounded weights are max(1, round(1000 × seconds)).",
             "All adapters return distances. CH internally reconstructs a path through its public API; that extra work is included. These are implementation timings, not isolated search kernels or equivalent path-output timings.", "",
             "Calls include clearing the previous query's touched state. The original query order is fixed (8-arc walk, 32-arc walk, uniform, uniform), so group timings include that preceding-query context. The [controlled query-order diagnostic](insights.md) exposes this carryover; these historical numbers remain unchanged and describe that workload.", "",
             "## Native weights", "", "| City | Nodes | Method | Mean µs | p50 µs | p95 µs |", "|---|---:|---|---:|---:|---:|"]
    lines.extend(_latency_rows(results, methods, 0))
    lines.extend(["", "## Rounded weights", "", "| City | Nodes | Method | Mean µs | p50 µs | p95 µs |", "|---|---:|---|---:|---:|---:|"])
    rounded_methods = methods + ["fast_paths-CH"]
    lines.extend(_latency_rows(results, rounded_methods, 1000))
    lines.extend(["", "## Best-method comparison", "", "Rankings use p50. Dijkstra ratios divide Dijkstra p50 by the fastest method p50 at the same scale. H/CH uses the selected best-p50 hypergraph method's paired median of per-query mean-latency ratios.",
                  "| City | Native fastest | Native Dijkstra / best | Rounded fastest | Rounded Dijkstra / best | Rounded paired H / CH |", "|---|---|---:|---|---:|---:|"])
    lines.extend(_comparison_rows(results))
    lines.extend(["", "## Throughput scaling", "", "Queries per second; each throughput sample includes worker launch and join time but excludes worker-state allocation. Ratios are descriptive; losses remain visible.",
                  "| City | Metric | Method | 1 worker QPS | Requested workers QPS | Parallel / serial |", "|---|---|---|---:|---:|---:|"])
    lines.extend(_throughput_rows(results, methods))
    lines.extend(["", "## Preprocessing", "", "Each overlay total sums median build times across six study configurations for one metric. Deployment builds only its chosen configuration; individual times remain in JSONL. Parallelism covers cell customization; partitioning is serial. CH is one separate rounded-metric build. Ratios are descriptive; regressions below 1 remain visible.",
                  "| City | Metric | Overlay study serial s | With parallel customization s | Serial / parallel | CH build s |", "|---|---|---:|---:|---:|---:|"])
    lines.extend(_preprocessing_rows(results))
    lines.extend(["", "## Correctness population", "", f"Each city and weight scale: {query_count:,} individually certified query distances; 3 certified full trees. Latency and throughput rows report zero wrong answers; overlay build rows checked parity on all {query_count:,} queries.",
                  "", "## Machine and build provenance", "", f"Queries {query_count:,}; repeats {repeats}; threads {threads}; seed {seed}; git {git_sha}; RUSTFLAGS `{rustflags}`.",
                  "Comparisons are within each city job. CPU models vary across jobs, so absolute latency differences across cities also reflect different hardware.",
                  "", "| City | CPU | OS | Rust compiler | Logical CPUs | RAM bytes |", "|---|---|---|---|---:|---:|"])
    lines.extend(_machine_rows(results))
    return "\n".join(lines) + "\n"


def _latency_rows(results, methods, scale):
    lines = []
    for city in sorted(CITIES):
        records = results[city]
        nodes = next(r["nodes"] for r in records if r["kind"] == "graph" and r["scale"] == scale)
        rows = {r["method"]: r for r in records if r["kind"] == "latency" and r["scale"] == scale}
        for method in methods:
            row = rows[method]
            lines.append(f"| {city} | {nodes:,} | {method} | {row['mean_us']:.2f} | {row['p50_us']:.2f} | {row['p95_us']:.2f} |")
    return lines


def _comparison_rows(results):
    lines = []
    for city in sorted(CITIES):
        records = results[city]
        native = {r["method"]: r["p50_us"] for r in records if r["kind"] == "latency" and r["scale"] == 0}
        rounded = {r["method"]: r["p50_us"] for r in records if r["kind"] == "latency" and r["scale"] == 1000}
        native_best_method, native_best = min(native.items(), key=lambda item: item[1])
        rounded_best_method, rounded_best = min(rounded.items(), key=lambda item: item[1])
        best_hyper_method = min((f"Hypergraph-{u}" for u in BOUNDS), key=lambda method: rounded[method])
        paired_hyper_ch = next(r["paired_ratio_to_ch"] for r in results[city]
                               if r["kind"] == "latency" and r["scale"] == 1000 and r["method"] == best_hyper_method)
        lines.append(f"| {city} | {native_best_method} | {native['dijkstra']/native_best:.2f}x | {rounded_best_method} | {rounded['dijkstra']/rounded_best:.2f}x | {paired_hyper_ch:.2f}x |")
    return lines


def _throughput_rows(results, methods):
    lines = []
    for city in sorted(CITIES):
        records = results[city]
        q = next(r["threads"] for r in records if r["kind"] == "graph" and r["scale"] == 0)
        for scale, label in [(0, "native"), (1000, "rounded")]:
            rows = {(r["method"], r["threads"]): r for r in records if r["kind"] == "throughput" and r["scale"] == scale}
            for method in methods + (["fast_paths-CH"] if scale else []):
                serial = rows[method, 1]["median_qps"]
                parallel = rows[method, q]["median_qps"]
                lines.append(f"| {city} | {label} | {method} | {serial:.1f} | {parallel:.1f} | {parallel/serial:.2f}× |")
    return lines


def _preprocessing_rows(results):
    lines = []
    for city in sorted(CITIES):
        records = results[city]
        for scale, label in [(0, "native"), (1000, "rounded")]:
            builds = [r for r in records if r["kind"] == "build" and r["scale"] == scale and r["method"] != "fast_paths-CH"]
            serial = sum(r["serial_s"] for r in builds)
            parallel = sum(r["parallel_s"] for r in builds)
            ch = f"{next(r['serial_s'] for r in records if r['kind'] == 'build' and r['method'] == 'fast_paths-CH'):.2f}" if scale else "n/a"
            lines.append(f"| {city} | {label} | {serial:.2f} | {parallel:.2f} | {serial/parallel:.3f}× | {ch} |")
    return lines


def _machine_rows(results):
    lines = []
    for city in sorted(CITIES):
        machine = next(r["machine"] for r in results[city] if r["kind"] == "graph" and r["scale"] == 0)
        lines.append(f"| {city} | {machine['cpu']} | {machine['os']} | {machine['compiler'].replace(chr(10), '<br>')} | {machine['os_cpu_count'] or 'n/a'} | {machine['ram_bytes'] or 'n/a'} |")
    return lines


def command_report(args):
    results = load_cities(args.source)
    text = report_text(results)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text)


def parser():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="action", required=True)
    run = sub.add_parser("run")
    run.add_argument("file", type=Path)
    run.add_argument("output", type=Path)
    run.add_argument("--queries", type=int, default=1000)
    run.add_argument("--threads", type=int, default=2)
    run.add_argument("--repeats", type=int, default=3)
    report = sub.add_parser("report")
    report.add_argument("source", type=Path, help="city JSONL directory or one consolidated JSONL file")
    report.add_argument("--output", type=Path)
    return p


if __name__ == "__main__":
    args = parser().parse_args()
    command_run(args) if args.action == "run" else command_report(args)
