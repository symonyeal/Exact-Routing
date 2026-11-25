"""Shared input and directed OSM export for native industry-router comparisons.

The input is `scale export`'s little-endian binary (src/bin/scale.rs). Node i becomes
OSM node i+1; arc a becomes one two-node, one-way way a+1. Tags preserve all eight
integer-millisecond metrics. Coordinates retain the source f32 values, subject to
OSM PBF's 1e-7 degree resolution. No speeds from OpenStreetMap are substituted.
"""

import argparse
import json
import struct
from pathlib import Path

import numpy as np


def schedule(q, p):
    """(slot, query indices) in precisely scale.rs's SplitMix64 shuffle order for pass p."""
    z = 20261125 + p
    mask = (1 << 64) - 1

    def shuffle(x):
        nonlocal z
        for i in range(len(x) - 1, 0, -1):
            z = (z + 0x9E3779B97F4A7C15) & mask
            a = z
            a = ((a ^ (a >> 30)) * 0xBF58476D1CE4E5B9) & mask
            a = ((a ^ (a >> 27)) * 0x94D049BB133111EB) & mask
            j = (a ^ (a >> 31)) % (i + 1)
            x[i], x[j] = x[j], x[i]

    order = list(range(8))
    shuffle(order)
    out = []
    for s in order:
        mine = [i for i in range(q) if (i + p) % 8 == s]
        shuffle(mine)
        out.append((s, mine))
    return out


def read_input(path):
    """Read the shared binary as memory-mapped arrays; reject truncated or invalid data.

    Returns n, m, tail/head (m u32), lat/lon (n f32), metrics (8 x m u32),
    pairs (2q u32), reference (8 x q u32; UINT32_MAX means unreachable).
    The optional trailing synthetic updates are consumed by RoutingKit only.
    """
    data = np.memmap(path, mode="r", dtype="u1")
    if len(data) < 16:
        raise ValueError("truncated industry input header")
    n, m = struct.unpack_from("<QQ", data)
    if not 0 < n <= 0xFFFFFFFF or m == 0:
        raise ValueError("industry input needs nodes and arcs")
    at = 16

    def take(count, dtype):
        nonlocal at
        end = at + count * np.dtype(dtype).itemsize
        if end > len(data):
            raise ValueError("truncated industry input")
        out = np.frombuffer(data, dtype=dtype, count=count, offset=at)
        at = end
        return out

    tail, head = take(m, "<u4"), take(m, "<u4")
    lat, lon = take(n, "<f4"), take(n, "<f4")
    metrics = take(8 * m, "<u4").reshape(8, m)
    q = int(take(1, "<u8")[0])
    pairs, reference = take(2 * q, "<u4"), take(8 * q, "<u4").reshape(8, q)
    if q == 0 or (tail >= n).any() or (head >= n).any() or (pairs >= n).any():
        raise ValueError("invalid industry endpoints or empty workload")
    if not (np.isfinite(lat).all() and np.isfinite(lon).all()) or (np.abs(lat) > 90).any() or (np.abs(lon) > 180).any():
        raise ValueError("invalid industry coordinates")
    if (metrics == 0).any() or (metrics == 0xFFFFFFFF).any():
        raise ValueError("industry arc weights must be positive finite milliseconds")
    return dict(n=n, m=m, tail=tail, head=head, lat=lat, lon=lon,
                metrics=metrics, pairs=pairs, reference=reference)


def export(input_path, outdir, slot=0):
    """Write graph.osm.pbf with one way per directed arc; arc_ms selects slot 0..7."""
    import osmium

    if slot not in range(8):
        raise ValueError("slot must be 0..7")
    g = read_input(input_path)
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    path = outdir / "graph.osm.pbf"
    # overwrite=True makes repeated per-metric imports use the same artifact.
    with osmium.SimpleWriter(str(path), overwrite=True) as writer:
        for i in range(g["n"]):
            writer.add_node(osmium.osm.mutable.Node(
                id=i + 1, location=(float(g["lon"][i]), float(g["lat"][i]))))
        for a, (u, v) in enumerate(zip(g["tail"], g["head"])):
            tags = {"highway": "road", "oneway": "yes", "arc_ms": str(int(g["metrics"][slot, a]))}
            tags.update({f"arc_ms_{j}": str(int(g["metrics"][j, a])) for j in range(8)})
            writer.add_way(osmium.osm.mutable.Way(
                id=a + 1, nodes=[int(u) + 1, int(v) + 1], tags=tags))
    return {"pbf": path}


def fixture(path):
    """Small directed graph for native conversion probes: competing paths, two components,
    eight distinct metrics, identity and unreachable requests. Distances use Bellman--Ford.
    """
    tail = [0, 0, 1, 1, 2, 2, 3, 4, 5]
    head = [1, 2, 0, 2, 0, 3, 2, 5, 4]
    base = [501, 1049, 1101, 501, 1901, 799, 851, 1251, 1451]
    pairs = [0, 2, 2, 0, 0, 3, 3, 0, 1, 3, 3, 1, 0, 0, 2, 2,
             4, 5, 5, 4, 0, 4, 4, 0, 1, 2, 2, 1, 5, 5, 3, 3]
    metrics = [[w + j * (a % 3 + 1) * 51 for a, w in enumerate(base)] for j in range(8)]
    refs = []
    for weights in metrics:
        distances = []
        for s, t in zip(pairs[::2], pairs[1::2]):
            d = [0xFFFFFFFF] * 6
            d[s] = 0
            for _ in range(5):
                for u, v, w in zip(tail, head, weights):
                    d[v] = min(d[v], d[u] + w)
            distances.append(d[t])
        refs.append(distances)
    coords = ([36.0, 36.0, 36.001, 36.002, 36.01, 36.011],
              [127.0, 127.001, 127.001, 127.001, 127.01, 127.011])
    parts = [struct.pack("<QQ", 6, len(tail))]
    parts += [np.asarray(v, dtype=dtype).tobytes() for v, dtype in
              ((tail, "<u4"), (head, "<u4"), (coords[0], "<f4"), (coords[1], "<f4"), (metrics, "<u4"))]
    parts += [struct.pack("<Q", len(pairs) // 2), np.asarray(pairs, "<u4").tobytes(),
              np.asarray(refs, "<u4").tobytes(), struct.pack("<Q", 0)]
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"".join(parts))


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fixture")
    f.add_argument("output", type=Path)
    e = sub.add_parser("export")
    e.add_argument("input", type=Path)
    e.add_argument("outdir", type=Path)
    e.add_argument("--slot", type=int, default=0)
    args = p.parse_args()
    if args.command == "fixture":
        fixture(args.output)
    else:
        print(json.dumps({k: str(v) for k, v in export(args.input, args.outdir, args.slot).items()}))


if __name__ == "__main__":
    main()
