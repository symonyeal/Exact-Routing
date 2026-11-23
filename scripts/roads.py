"""Convert a region of the Korea Road Network ETA dataset into a binary arc list for `cargo run --bin roads`.

Source: https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta (CC BY-NC 4.0; topology from
OpenStreetMap, ODbL; speeds from KTDB 2024). The files are fetched, never stored in the repository.

Usage: python scripts/roads.py REGION [SLOT]   (REGION: sejong, seoul, ..., or korea for the nationwide
graph; SLOT: speed column, default weekday_am_peak_p50)

Writes data/roads/REGION.bin, little-endian: n, m as u64; lat, lon as f64[n]; tail, head as u32[m];
travel seconds as f64[m]. A two-way segment gives two arcs. Travel time is length / speed with the
speed clipped below at 5 km/h (the dataset card's rule), the default speed where the slot is missing.
"""

import io
import ssl
import sys
import urllib.request
from pathlib import Path

import numpy as np
import pyarrow.parquet as pq

URL = "https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta/resolve/main/{}"
DATA = Path(__file__).resolve().parent.parent / "data" / "roads"

ctx = ssl.create_default_context()
# Some proxies re-sign with chains lacking a Subject Key Identifier; chain and hostname checks remain.
ctx.verify_flags &= ~ssl.VERIFY_X509_STRICT


def table(name, columns):
    with urllib.request.urlopen(URL.format(name), timeout=600, context=ctx) as r:
        return pq.read_table(io.BytesIO(r.read()), columns=columns).to_pandas()


def main():
    region = sys.argv[1]
    slot = sys.argv[2] if len(sys.argv) > 2 else "weekday_am_peak_p50"
    stem = "nationwide/korea" if region == "korea" else f"regions/{region}"
    nodes = table(f"{stem}_road_graph_nodes.parquet", ["node_id", "lat", "lon"])
    edges = table(f"{stem}_road_graph_edges.parquet", ["source", "target", "length_m", "oneway", "default_speed_kmh", slot])
    if not (nodes.node_id.to_numpy() == np.arange(len(nodes))).all():
        raise SystemExit("node ids are not 0..n-1")
    kmh = edges[slot].fillna(edges.default_speed_kmh).clip(lower=5.0).to_numpy(np.float64)
    secs = edges.length_m.to_numpy(np.float64) / (kmh / 3.6)
    u, v = edges.source.to_numpy(np.uint32), edges.target.to_numpy(np.uint32)
    two = ~edges.oneway.to_numpy(bool)
    tail, head = np.concatenate([u, v[two]]), np.concatenate([v, u[two]])
    w = np.concatenate([secs, secs[two]])
    DATA.mkdir(parents=True, exist_ok=True)
    with open(DATA / f"{region}.bin", "wb") as f:
        np.array([len(nodes), len(tail)], "<u8").tofile(f)
        for x, t in ((nodes.lat, "<f8"), (nodes.lon, "<f8"), (tail, "<u4"), (head, "<u4"), (w, "<f8")):
            np.asarray(x, t).tofile(f)
    print(f"{DATA / region}.bin: {len(nodes)} nodes, {len(tail)} arcs, {int(two.sum())} two-way segments")


if __name__ == "__main__":
    main()
