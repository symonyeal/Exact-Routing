"""OSRM CH/MLD through libosrm and its segment-speed update path (Linux CI).

  prepare INPUT WORK --build BUILD     export, extract and partition, one thread
  slot INPUT WORK ch|mld PASS SLOT --build BUILD
  run INPUT WORK OUTPUT --build BUILD [--passes 0,1,2] [--slots 0,...,7]

Build is external/osrm/CMakeLists.txt's build tree; tools live in BUILD/upstream.
Rows stream to OUTPUT.partial. A failed conversion or endpoint check is fatal;
there is no fallback to OSM speeds or a different graph.
"""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import industry

ROOT = Path(__file__).resolve().parents[1]
TAG = "v5.27.1"
COMMIT = "4f3ee609ec1af40eb1f445c6706cfa5beb04c990"
LICENCE = "BSD-2-Clause"


def invoke(command, work, capture=False):
    """Return wall seconds, process peak RSS bytes and JSON rows from one native call."""
    timing = work / "process.time.txt"
    start = time.perf_counter()
    result = subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(timing),
                             *map(str, command)], check=True, text=True,
                            stdout=subprocess.PIPE if capture else sys.stderr)
    seconds = time.perf_counter() - start
    rss = int(timing.read_text(encoding="utf-8").strip()) * 1024
    rows = [json.loads(line) for line in (result.stdout or "").splitlines() if line.startswith("{")]
    return seconds, rss, rows


def prepare(input_path, work, build):
    """Load the graph once; save preparation costs for both algorithms."""
    work.mkdir(parents=True, exist_ok=True)
    pbf = industry.export(input_path, work)["pbf"]
    invoke([build / "osrm-bench", "segments", input_path, work], work)
    extract_s, extract_rss, _ = invoke(
        [build / "upstream" / "osrm-extract", pbf, "-p", ROOT / "external/osrm/profile.lua",
         "--threads", 1, "--small-component-size", 0], work)
    partition_s, partition_rss, _ = invoke(
        [build / "upstream" / "osrm-partition", work / "graph.osrm", "--threads", 1], work)
    data = {"tag": TAG, "commit": COMMIT, "licence": LICENCE, "extract_s": extract_s,
            "partition_s": partition_s, "extract_memory_bytes": extract_rss,
            "partition_memory_bytes": partition_rss}
    (work / "preparation.json").write_text(json.dumps(data) + "\n", encoding="utf-8", newline="\n")
    return [{"kind": "industry_prepare", "method": f"osrm-{a}", **data,
             "prepare_s": extract_s + (partition_s if a == "mld" else 0),
             "memory_bytes": max(extract_rss, partition_rss if a == "mld" else 0)}
            for a in ("ch", "mld")]


def slot(input_path, work, build, algorithm, pass_index, metric):
    """Update through OSRM, audit every stored segment, then request and re-price paths."""
    tool = "osrm-contract" if algorithm == "ch" else "osrm-customize"
    update_s, update_rss, _ = invoke(
        [build / "upstream" / tool, work / "graph.osrm", "--threads", 1,
         "--segment-speed-file", work / f"slot-{metric}.csv"], work)
    _, _, audit = invoke([build / "osrm-bench", "audit", input_path, work / "graph.osrm", metric], work, True)
    g = industry.read_input(input_path)
    mine = next(indices for s, indices in industry.schedule(len(g["pairs"]) // 2, pass_index) if s == metric)
    _, _, rows = invoke([build / "osrm-bench", "query", input_path, work / "graph.osrm",
                        algorithm, pass_index, metric, ",".join(map(str, mine))], work, True)
    if len(audit) != 1 or len(rows) != 1:
        raise ValueError("OSRM native driver did not return one conversion and query row")
    rows[0].update(update_s=update_s, update_memory_bytes=update_rss, conversion=audit[0],
                   tag=TAG, licence=LICENCE, threads=1)
    return rows[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "slot", "run"):
        p = sub.add_parser(name)
        p.add_argument("input", type=Path)
        p.add_argument("work", type=Path)
        p.add_argument("--build", type=Path, required=True)
        if name == "slot":
            p.add_argument("algorithm", choices=("ch", "mld"))
            p.add_argument("pass_index", type=int)
            p.add_argument("metric", type=int, choices=range(8))
        if name == "run":
            p.add_argument("output", type=Path)
            p.add_argument("--passes", default="0,1,2")
            p.add_argument("--slots", default="0,1,2,3,4,5,6,7")
    args = parser.parse_args()
    args.input, args.work, args.build = (p.resolve() for p in (args.input, args.work, args.build))
    if args.command == "prepare":
        for row in prepare(args.input, args.work, args.build):
            print(json.dumps(row), flush=True)
    elif args.command == "slot":
        print(json.dumps(slot(args.input, args.work, args.build, args.algorithm, args.pass_index, args.metric)), flush=True)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        partial = args.output.with_name(args.output.name + ".partial")
        with partial.open("w", encoding="utf-8", newline="\n") as out:
            def emit(row):
                out.write(json.dumps(row, allow_nan=False) + "\n")
                out.flush()
            for row in prepare(args.input, args.work, args.build):
                emit(row)
            g = industry.read_input(args.input)
            selected = set(map(int, args.slots.split(",")))
            for pass_index in map(int, args.passes.split(",")):
                for metric, _ in industry.schedule(len(g["pairs"]) // 2, pass_index):
                    if metric not in selected:
                        continue
                    for algorithm in ("ch", "mld") if (pass_index + metric) % 2 == 0 else ("mld", "ch"):
                        emit(slot(args.input, args.work, args.build, algorithm, pass_index, metric))
        args.output.write_bytes(partial.read_bytes())


if __name__ == "__main__":
    main()
