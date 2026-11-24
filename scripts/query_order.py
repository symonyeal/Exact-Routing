"""Replay the registered Sejong/London cleanup diagnostic; retain all conditions separately."""

import argparse
import datetime
import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("results/query-order.json"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    executable = root / "target" / "release" / ("query_order.exe" if os.name == "nt" else "query_order")
    subprocess.run(["cargo", "build", "--release", "--locked", "--bin", "query_order"], cwd=root, check=True)
    source = root / "results" / "metro.jsonl"
    files = ["Cargo.toml", "Cargo.lock", "src/lib.rs", "src/graph.rs", "src/overlay.rs", "src/road.rs", "src/bin/query_order.rs"]
    hashes = {file: hashlib.sha256((root / file).read_bytes().replace(b"\r\n", b"\n")).hexdigest() for file in files}
    experiments = []
    for city in ("sejong", "london-large"):
        print(f"Replaying {city}: fixed, reversed and separately prepared state...", flush=True)
        binary = root / "data" / "roads" / f"{city}.bin"
        digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        result = subprocess.run([str(executable), str(binary), str(source), city], cwd=root,
                                check=True, capture_output=True, text=True, timeout=240)
        data = json.loads(result.stdout)
        assert data["source_binary_sha256"] == digest
        assert data["same_current_query_work"] is True
        assert len(data["results"]) == 6 and all(row["wrong"] == 0 for row in data["results"])
        experiments.append(data)
    payload = {"schema": 1, "experiment": "registered native-weight query-order and preparation diagnostic",
               "experiments": experiments, "provenance": {
                   "measured_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   "cpu": platform.processor(), "os": platform.platform(), "logical_cpus": os.cpu_count(),
                   "compiler": subprocess.run(["rustc", "-Vv"], check=True, capture_output=True, text=True).stdout.strip(),
                   "rustflags": os.environ.get("RUSTFLAGS", "default"),
                   "git_sha": subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, check=True, capture_output=True, text=True).stdout.strip(),
                   "git_dirty": bool(subprocess.run(["git", "status", "--porcelain"], cwd=root, check=True, capture_output=True, text=True).stdout.strip()),
                   "source_hash_normalization": "CRLF converted to LF", "source_file_sha256": hashes,
                   "binary_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                   "frozen_results_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
               }}
    output = args.output if args.output.is_absolute() else root / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_suffix(".partial")
    partial.write_text(json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":")) + "\n", encoding="utf-8", newline="\n")
    partial.replace(output)
    for experiment in experiments:
        for row in experiment["results"]:
            print(experiment["city"], row["method"], row["condition"],
                  [(g["category"], round(g["median_query_us"], 2), round(g["median_preparation_us"], 2), round(g["median_previous_touched"])) for g in row["groups"]])


if __name__ == "__main__":
    main()
