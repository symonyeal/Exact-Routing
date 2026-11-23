"""Convert one Korea Road Network ETA region to the `roads` binary format.

Source: https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta at
revision b111710756ee5727a0fbe3f6203c0cc2f93de2be (CC BY-NC 4.0; topology
from OpenStreetMap, ODbL; observed speeds from KTDB 2024).

Usage: python scripts/roads.py REGION [SLOT]. The default slot is
weekday_am_peak_p50. Binary output stores node and arc counts as little-endian
u64, coordinates as f64, endpoints as u32, and travel times as f64 seconds.
Each two-way segment becomes two arcs. Missing slot speeds use the default
speed, then speeds are clipped below at 5 km/h as specified by the dataset.
"""

import hashlib
import io
import json
import ssl
import sys
import urllib.request
from pathlib import Path

import numpy as np
import pyarrow.parquet as pq

REVISION = "b111710756ee5727a0fbe3f6203c0cc2f93de2be"
URL = f"https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta/resolve/{REVISION}/{{}}"
DATA = Path(__file__).resolve().parent.parent / "data" / "roads"

ctx = ssl.create_default_context()
# Some proxies re-sign with chains lacking a Subject Key Identifier; chain and hostname checks remain.
ctx.verify_flags &= ~ssl.VERIFY_X509_STRICT


def encode(nodes, edges, slot):
    """Return the roads binary and graph counts after validating source tables."""
    n = len(nodes)
    if n == 0 or n > 0xFFFFFFFF:
        raise ValueError("node count must fit the nonempty u32 endpoint format")
    node_ids = nodes.node_id.to_numpy()
    if not np.array_equal(node_ids, np.arange(n)):
        raise ValueError("node ids must be consecutive integers from 0 to n-1")
    lat, lon = nodes.lat.to_numpy(np.float64), nodes.lon.to_numpy(np.float64)
    if not (np.isfinite(lat).all() and np.isfinite(lon).all()):
        raise ValueError("node coordinates must be finite")
    if (np.abs(lat) > 90).any() or (np.abs(lon) > 180).any():
        raise ValueError("node coordinates must be geographic degrees")

    endpoints = []
    for name in ("source", "target"):
        values = edges[name].to_numpy()
        try:
            numeric = values.astype(np.float64)
        except (TypeError, ValueError) as exc:
            raise ValueError(f"edge {name} values must be integers") from exc
        if not (np.isfinite(numeric).all() and np.equal(numeric, np.floor(numeric)).all()):
            raise ValueError(f"edge {name} values must be integers")
        if ((numeric < 0) | (numeric >= n)).any():
            raise ValueError(f"edge {name} endpoint is outside 0..{n - 1}")
        endpoints.append(numeric.astype(np.uint32))
    u, v = endpoints

    length = edges.length_m.to_numpy(np.float64)
    if not (np.isfinite(length).all() and (length >= 0).all()):
        raise ValueError("edge lengths must be finite and nonnegative")
    oneway = edges.oneway
    if oneway.isna().any() or not oneway.isin([True, False]).all():
        raise ValueError("oneway values must be present booleans")
    two = ~oneway.to_numpy(bool)
    kmh = edges[slot].fillna(edges.default_speed_kmh).to_numpy(np.float64)
    kmh = np.maximum(kmh, 5.0)
    if not (np.isfinite(kmh).all() and (kmh > 0).all()):
        raise ValueError("effective speeds must be finite and positive")
    secs = length / (kmh / 3.6)
    if not np.isfinite(secs).all():
        raise ValueError("travel times must be finite")

    tail, head = np.concatenate([u, v[two]]), np.concatenate([v, u[two]])
    weights = np.concatenate([secs, secs[two]])
    parts = [np.array([n, len(tail)], "<u8").tobytes()]
    parts.extend(np.asarray(x, dtype).tobytes() for x, dtype in (
        (lat, "<f8"), (lon, "<f8"), (tail, "<u4"), (head, "<u4"), (weights, "<f8")
    ))
    return b"".join(parts), n, len(tail)


def read_table(name, columns):
    with urllib.request.urlopen(URL.format(name), timeout=600, context=ctx) as response:
        raw = response.read()
    return pq.read_table(io.BytesIO(raw), columns=columns).to_pandas(), hashlib.sha256(raw).hexdigest()


def main():
    region = sys.argv[1]
    slot = sys.argv[2] if len(sys.argv) > 2 else "weekday_am_peak_p50"
    stem = "nationwide/korea" if region == "korea" else f"regions/{region}"
    nodes, nodes_hash = read_table(f"{stem}_road_graph_nodes.parquet", ["node_id", "lat", "lon"])
    edges, edges_hash = read_table(
        f"{stem}_road_graph_edges.parquet",
        ["source", "target", "length_m", "oneway", "default_speed_kmh", slot],
    )
    binary, n, m = encode(nodes, edges, slot)
    DATA.mkdir(parents=True, exist_ok=True)
    output = DATA / f"{region}.bin"
    output.write_bytes(binary)
    metadata = {
        "dataset": "CAMUS-LAB/data-road_network_eta",
        "revision": REVISION,
        "region": region,
        "slot": slot,
        "weight_units": "seconds",
        "nodes_sha256": nodes_hash,
        "edges_sha256": edges_hash,
        "output_sha256": hashlib.sha256(binary).hexdigest(),
        "node_count": n,
        "arc_count": m,
    }
    output.with_suffix(".meta.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(f"{region}: {n} nodes, {m} arcs")


if __name__ == "__main__":
    main()
