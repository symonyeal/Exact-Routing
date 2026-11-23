"""Convert one pinned InstructOSM city graph to the `roads` binary format.

Usage: python scripts/international.py CITY [SCALE] (default: large).
Weights are the dataset's `travel_time_min` values converted to seconds. The
GraphML is parsed as XML; directed parallel edges are retained as separate arcs.
"""

import argparse
import hashlib
import json
import math
import ssl
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np

REVISION = "9b0548fc99a53ce509e4458748c2246ae8884fe4"
DATASET = "https://huggingface.co/datasets/HBNU-NLP/InstructOSM"
URL = f"{DATASET}/resolve/{REVISION}/maps/{{}}/{{}}/graph.graphml"
CITIES = ("london", "new_york", "tokyo", "seoul")
SCALES = ("small", "medium", "large")
# Canonical large-snapshot counts in the pinned dataset card (27 March 2026).
LARGE = {"london": (50301, 117618), "new_york": (50306, 122668),
         "tokyo": (50231, 131806), "seoul": (50475, 137865)}
DATA = Path(__file__).resolve().parent.parent / "data" / "roads"
ctx = ssl.create_default_context()
# Keep chain and hostname verification; office proxies can lack strict X.509 extensions.
ctx.verify_flags &= ~ssl.VERIFY_X509_STRICT


def tag(element):
    return element.tag.rsplit("}", 1)[-1]


def encode(raw):
    """Parse GraphML bytes and return (roads binary, node count, arc count)."""
    try:
        root = ET.fromstring(raw)
    except ET.ParseError as exc:
        raise ValueError(f"invalid GraphML: {exc}") from exc

    keys = {
        key.get("id"): (key.get("for"), key.get("attr.name"))
        for key in root.iter()
        if tag(key) == "key"
    }
    graphs = [element for element in root.iter() if tag(element) == "graph"]
    if len(graphs) != 1 or graphs[0].get("edgedefault") != "directed":
        raise ValueError("GraphML must contain one directed graph")
    graph = graphs[0]

    def data(element, scope):
        values = {}
        for item in element:
            if tag(item) == "data":
                key = keys.get(item.get("key"))
                if key is not None and key[0] in (scope, "all"):
                    values[key[1]] = item.text
        return values

    nodes = [element for element in graph if tag(element) == "node"]
    n = len(nodes)
    if n == 0 or n > 0xFFFFFFFF:
        raise ValueError("node count must fit the nonempty u32 endpoint format")
    node_id = {}
    coords = []
    for i, node in enumerate(nodes):
        ident = node.get("id")
        if ident is None or ident in node_id:
            raise ValueError("node ids must be present and unique")
        node_id[ident] = i
        values = data(node, "node")
        try:
            lon, lat = float(values["x"]), float(values["y"])
        except (KeyError, TypeError, ValueError) as exc:
            raise ValueError("each node needs numeric x (longitude) and y (latitude)") from exc
        if not (math.isfinite(lon) and math.isfinite(lat)):
            raise ValueError("node coordinates must be finite")
        if abs(lat) > 90 or abs(lon) > 180:
            raise ValueError("node coordinates must be geographic degrees")
        coords.append((lat, lon))

    arcs = []
    for edge in graph:
        if tag(edge) != "edge":
            continue
        if edge.get("directed", "true").lower() == "false":
            raise ValueError("undirected edge cannot be encoded without expansion")
        try:
            u, v = node_id[edge.get("source")], node_id[edge.get("target")]
        except KeyError as exc:
            raise ValueError("edge endpoint references an unknown node") from exc
        values = data(edge, "edge")
        if values.get("directed", "true").lower() == "false":
            raise ValueError("undirected edge cannot be encoded without expansion")
        try:
            minutes = float(values["travel_time_min"])
        except (KeyError, TypeError, ValueError) as exc:
            raise ValueError("each edge needs numeric travel_time_min") from exc
        seconds = minutes * 60.0
        if not math.isfinite(seconds) or seconds < 0:
            raise ValueError("travel_time_min must be finite and nonnegative")
        arcs.append((u, v, seconds))
    m = len(arcs)

    lat, lon = ([point[i] for point in coords] for i in (0, 1))
    tail, head, weight = ([arc[i] for arc in arcs] for i in (0, 1, 2))
    arrays = (
        (np.array([n, m], dtype="<u8"),),
        (np.asarray(lat, dtype="<f8"), np.asarray(lon, dtype="<f8")),
        (np.asarray(tail, dtype="<u4"), np.asarray(head, dtype="<u4")),
        (np.asarray(weight, dtype="<f8"),),
    )
    return b"".join(array.tobytes() for group in arrays for array in group), n, m


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("city", choices=CITIES)
    parser.add_argument("scale", nargs="?", choices=SCALES, default="large")
    args = parser.parse_args()
    url = URL.format(args.city, args.scale)
    with urllib.request.urlopen(url, timeout=600, context=ctx) as response:
        raw = response.read()
    binary, n, m = encode(raw)
    if args.scale == "large" and (n, m) != LARGE[args.city]:
        raise ValueError("graph counts differ from the pinned snapshot catalogue")
    DATA.mkdir(parents=True, exist_ok=True)
    output = DATA / f"{args.city}-{args.scale}.bin"
    output.write_bytes(binary)
    metadata = {
        "dataset": DATASET,
        "revision": REVISION,
        "city": args.city,
        "scale": args.scale,
        "source_url": url,
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "output_sha256": hashlib.sha256(binary).hexdigest(),
        "source_weight_attribute": "travel_time_min",
        "conversion": "travel_time_min * 60",
        "native_units": "seconds",
        "weight_units": "seconds",
        "node_count": n,
        "arc_count": m,
    }
    output.with_suffix(".meta.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )
    print(f"{args.city}-{args.scale}: {n} nodes, {m} arcs")


if __name__ == "__main__":
    main()
