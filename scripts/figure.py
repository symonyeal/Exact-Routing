"""Draw site/sejong-query.png: one Sejong query searched on the edge-cut and junction-cut overlays.

Reads the road graph data/roads/sejong.bin and the partition_view export for U = 256, native
seconds. The export is rebuilt with partition_view when absent; it certifies the query distance
against a full Dijkstra tree before writing. Needs numpy and matplotlib.
"""

import json
import subprocess
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.collections import LineCollection

ROOT = Path(__file__).resolve().parents[1]
QUERY = 2  # preset index 2: uniform pair 80,385 -> 51,842, the case quoted in results/insights.md
INK, MUTED, ROAD, AMBER, BLUE = "#1d2b33", "#5b6b73", "#d5d9dc", "#c46a12", "#2f6fb0"


def graph(path):
    """Return lon, lat : f64[n] and tail, head : u32[m] from the road format of src/road.rs."""
    b = path.read_bytes()
    n, m = np.frombuffer(b, "<u8", 2)
    lat = np.frombuffer(b, "<f8", n, 16)
    lon = np.frombuffer(b, "<f8", n, 16 + 8 * n)
    tail = np.frombuffer(b, "<u4", m, 16 + 16 * n)
    head = np.frombuffer(b, "<u4", m, 16 + 16 * n + 4 * m)
    return lon, lat, tail, head


def export():
    path = ROOT / "data" / "insights" / "sejong-256-0.json"
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        out = subprocess.run(["cargo", "run", "--release", "--locked", "--bin", "partition_view", "--",
                              "data/roads/sejong.bin", "results/metro.jsonl", "sejong", "256", "0"],
                             cwd=ROOT, check=True, capture_output=True)
        path.write_bytes(out.stdout)
    return json.loads(path.read_text(encoding="utf-8"))


def main():
    lon, lat, tail, head = graph(ROOT / "data" / "roads" / "sejong.bin")
    x, y = lon * np.cos(np.radians(lat.mean())), lat
    view = export()
    q = view["queries"][QUERY]
    assert q["certified"] and (q["source"], q["target"]) == (80385, 51842)

    settled = np.concatenate([q[m]["settled_nodes"] for m in ("Graph", "Hypergraph")])
    pad = 0.05
    x0, x1 = x[settled].min(), x[settled].max()
    y0, y1 = y[settled].min(), y[settled].max()
    w, h = x1 - x0, y1 - y0
    box = (x0 - pad * w, x1 + pad * w, y0 - pad * h, y1 + pad * h)
    panel = 4.6
    fig, axes = plt.subplots(1, 2, figsize=(2 * panel + 0.3, panel * h / w + 1.0), dpi=150)
    fig.patch.set_facecolor("white")
    seen = (x[tail] > box[0]) & (x[tail] < box[1]) & (y[tail] > box[2]) & (y[tail] < box[3])
    segments = list(np.stack([np.c_[x[tail[seen]], y[tail[seen]]], np.c_[x[head[seen]], y[head[seen]]]], axis=1))

    titles = {"Graph": "Edge cut", "Hypergraph": "Junction cut"}
    for ax, model in zip(axes, ("Graph", "Hypergraph")):
        work = q[model]
        boundary = np.zeros(len(x), bool)
        boundary[view["models"][model]["boundary_nodes"]] = True
        s = np.array(work["settled_nodes"])
        ax.add_collection(LineCollection(segments, colors=ROAD, linewidths=0.35, zorder=1))
        inside = boundary & (x > box[0]) & (x < box[1]) & (y > box[2]) & (y < box[3])
        ax.scatter(x[inside], y[inside], s=1.2, c="#e9c9a3", linewidths=0, zorder=2)
        ax.scatter(x[s[~boundary[s]]], y[s[~boundary[s]]], s=3, c=BLUE, linewidths=0, zorder=3)
        ax.scatter(x[s[boundary[s]]], y[s[boundary[s]]], s=3, c=AMBER, linewidths=0, zorder=4)
        for v in (q["source"], q["target"]):
            ax.scatter(x[v], y[v], s=42, c=INK, edgecolors="white", linewidths=1.2, zorder=5)
        ax.set_xlim(box[0], box[1]); ax.set_ylim(box[2], box[3]); ax.set_aspect("equal")
        ax.set_xticks([]); ax.set_yticks([])
        for side in ax.spines.values():
            side.set_visible(False)
        ax.set_title(f"{titles[model]}: {work['settled']:,} junctions searched", loc="left",
                     fontsize=12, color=INK, fontweight="bold")
        ax.text(0, -0.012, f"{work['boundary_settled']:,} on cell boundaries (orange), "
                f"{work['interior_settled']:,} inside cells (blue)", transform=ax.transAxes,
                fontsize=9.5, color=MUTED, va="top")
    fig.subplots_adjust(left=0.01, right=0.99, top=0.955, bottom=0.045, wspace=0.06)
    out = ROOT / "site" / "sejong-query.png"
    fig.savefig(out, facecolor="white")
    print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
