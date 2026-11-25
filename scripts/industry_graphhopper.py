"""Run GraphHopper's native CH importer and query API, one fresh import per weekday metric.

Input is scale's routingkit.bin and industry.py's PBF, which retains all eight millisecond tags.
Each Java invocation uses one thread. A new cache is required: loading an old cache would skip
the parser that records original arc identities. Rows stream to OUTPUT.partial; completed rows
replace OUTPUT only after every selected pass and slot succeeds.
"""

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

import industry

ROOT = Path(__file__).resolve().parents[1]
COMMIT = "69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4"


def prepare(input_path, work):
    """Export the PBF; all timed preparation is repeated inside each metric import."""
    industry.export(input_path, work)
    return []


def slot(input_path, work, pass_index, metric, pbf=None, java="java"):
    """Return one native metric row after a fresh import; remove the disposable native cache."""
    jar = ROOT / "external/graphhopper/target/graphhopper-benchmark-1.jar"
    work.mkdir(parents=True, exist_ok=True)
    cache = work / "graphhopper-cache"
    rss = work / "graphhopper-rss.txt"
    if cache.exists():
        raise ValueError(f"GraphHopper cache already exists: {cache}")
    pbf = pbf or work / "graph.osm.pbf"
    command = [java, "-Xmx10g", "-Dorg.slf4j.simpleLogger.defaultLogLevel=warn",
               "-jar", str(jar), str(input_path), str(pbf), str(cache), str(pass_index), str(metric)]
    if sys.platform == "linux":
        command = ["/usr/bin/time", "-f", "%M", "-o", str(rss), *command]
    try:
        result = subprocess.run(command, check=True, text=True, stdout=subprocess.PIPE)
        rows = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        if len(rows) != 1 or rows[0].get("commit") != COMMIT:
            raise ValueError("GraphHopper did not produce one pinned metric row")
        row = rows[0]
        if rss.exists():
            row["memory_bytes"] = row["update_memory_bytes"] = int(rss.read_text().strip()) * 1024
        row.update(memory_scope="whole JVM process including original input and certificates",
                   tag="11.0", licence="Apache-2.0")
        return row
    finally:
        if cache.exists():
            shutil.rmtree(cache)
        rss.unlink(missing_ok=True)


def run(input_path, pbf, output, passes, slots, java="java"):
    partial = output.with_suffix(output.suffix + ".partial")
    output.parent.mkdir(parents=True, exist_ok=True)
    g = industry.read_input(input_path)
    with partial.open("w", encoding="utf-8", newline="\n") as out:
        for p in passes:
            for metric, _ in industry.schedule(len(g["pairs"]) // 2, p):
                if metric not in slots:
                    continue
                row = slot(input_path, output.parent, p, metric, pbf, java)
                out.write(json.dumps(row, separators=(",", ":"), allow_nan=False) + "\n")
                out.flush()
    partial.replace(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("input", type=Path)
    parser.add_argument("pbf", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--passes", default="0,1,2")
    parser.add_argument("--slots", default="0,1,2,3,4,5,6,7")
    args = parser.parse_args()
    passes = [int(p) for p in args.passes.split(",")]
    slots = [int(s) for s in args.slots.split(",")]
    if not passes or not slots or any(p not in range(3) for p in passes) or any(s not in range(8) for s in slots):
        parser.error("passes must be 0..2 and slots 0..7")
    run(args.input, args.pbf, args.output, passes, slots)


if __name__ == "__main__":
    main()
