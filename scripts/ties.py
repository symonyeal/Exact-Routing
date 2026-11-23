"""Score candidate tie-breaking rules against the Maze-Hard training answers (run scripts/fetch.py first).

Rules: breadth-first parent pointers and distance descent from either endpoint, and A* with five heap
keys from either endpoint, each under all 24 neighbour orders (336 rules). Prints the rule count and
the 15 best rules by the number of training answer grids reproduced exactly.
"""

import csv
import heapq
import io
import itertools
import sys
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
W = 30
DIRS = {"U": -W, "D": W, "L": -1, "R": 1}


def load(split):
    data = (ROOT / "data" / f"{split}.csv").read_bytes()
    return list(csv.DictReader(io.StringIO(data.decode("utf-8-sig"), newline="")))


def nbrs(q, v, order):
    r, c = divmod(v, W)
    for k in order:
        d = DIRS[k]
        if (k == "L" and c == 0) or (k == "R" and c == W - 1) or (k == "U" and r == 0) or (k == "D" and r == W - 1):
            continue
        if q[v + d] != "#":
            yield v + d


def bfs(q, s, order):
    dist, par = {s: 0}, {s: None}
    Q = deque([s])
    while Q:
        v = Q.popleft()
        for w in nbrs(q, v, order):
            if w not in dist:
                dist[w], par[w] = dist[v] + 1, v
                Q.append(w)
    return dist, par


def trace(par, t):
    p = []
    while t is not None:
        p.append(t)
        t = par[t]
    return set(p)


def descend(q, dist, a, order):
    """Walk from a down the distance field, taking the first neighbour in order."""
    p = [a]
    while dist[p[-1]]:
        p.append(next(w for w in nbrs(q, p[-1], order) if dist.get(w) == dist[p[-1]] - 1))
    return set(p)


def astar(q, s, t, order, key):
    tr, tc = divmod(t, W)
    h = lambda v: abs(v // W - tr) + abs(v % W - tc)
    g, par, closed, cnt = {s: 0}, {s: None}, set(), itertools.count()
    H = [(key(g[s], h(s), next(cnt)), s)]
    while H:
        _, v = heapq.heappop(H)
        if v in closed:
            continue
        closed.add(v)
        if v == t:
            return trace(par, t)
        for w in nbrs(q, v, order):
            if w not in g or g[v] + 1 < g[w]:
                g[w], par[w] = g[v] + 1, v
                heapq.heappush(H, (key(g[w], h(w), next(cnt)), w))


KEYS = {
    "f,h,fifo": lambda g, h, c: (g + h, h, c),
    "f,h,lifo": lambda g, h, c: (g + h, h, -c),
    "f,fifo": lambda g, h, c: (g + h, c),
    "f,lifo": lambda g, h, c: (g + h, -c),
    "f,g,fifo": lambda g, h, c: (g + h, g, c),
}


def main():
    split = sys.argv[1] if len(sys.argv) > 1 else "train"
    rows = load(split)
    tasks = []
    for row in rows:
        q, a = row["question"], row["answer"]
        tasks.append((q, q.index("S"), q.index("G"), {v for v, ch in enumerate(a) if ch in "SGo"}))
    score = {}
    for order in map("".join, itertools.permutations("UDLR")):
        for name, src in {"bfsS": 1, "bfsG": 2}.items():
            hit_par = hit_desc = 0
            for q, s, t, mask in tasks:
                x, y = (s, t) if src == 1 else (t, s)
                dist, par = bfs(q, x, order)
                hit_par += trace(par, y) == mask
                hit_desc += descend(q, dist, y, order) == mask
            score[f"{name}-parent-{order}"] = hit_par
            score[f"{name}-descend-{order}"] = hit_desc
        for kname, key in KEYS.items():
            for x in ("S", "G"):
                hit = sum(
                    astar(q, *((s, t) if x == "S" else (t, s)), order, key) == mask for q, s, t, mask in tasks
                )
                score[f"astar{x}-{kname}-{order}"] = hit
    print(f"{len(score)} rules, {len(tasks)} {split} mazes")
    best = sorted(score.items(), key=lambda kv: -kv[1])[:15]
    for k, v in best:
        print(f"{v:5d}  {k}")


if __name__ == "__main__":
    main()
