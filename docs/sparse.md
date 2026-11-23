# Sparse graphs and hypergraph partitions

## SALSA-CLRS: exact trees at every size

**Data.** [SALSA-CLRS](https://huggingface.co/datasets/SALSA-CLRS/SALSA-CLRS) (Minder, Grötschla, Mathys
and Wattenhofer, LoG 2023, [arXiv 2309.12253](https://arxiv.org/abs/2309.12253); Apache-2.0) tests
whether neural algorithmic reasoners trained on graphs of at most 16 nodes generalize to larger ones. The
test sets hold 1,000 graphs each for three sparse families (Erdős–Rényi with p = c·ln n / n,
Watts–Strogatz, Delaunay triangulations) at n = 16, 80, 160, 800 and 1,600. The output for breadth-first
search and Dijkstra is the parent of every node in the shortest-path tree from a source, π(s) = s and
π(v) = v for unreached nodes. Edge weights for Dijkstra are uniform draws transformed by √(w + 0.001),
stored as 32-bit floats. `scripts/salsa.py` reads the published PyTorch files with a restricted
unpickler and numpy, so no torch install is needed.

**Tie rule.** A node usually has several breadth-first parents. The reference takes the smallest-index
node one layer closer to the source, the rule of the CLRS benchmark's layer-synchronous breadth-first
search. `graph::bfs` applies it in both directions of its search: top-down layers process the frontier
in ascending order and keep the first discoverer; bottom-up layers scan each unreached node's in-arcs in
ascending order and take the first frontier node (Beamer, Asanović and Patterson 2012). Dijkstra's
reference parent is the first settled node attaining the final distance; with continuous weights no
ties occurred.

**Certificate.** `graph::check` verifies a shortest-path tree without trusting the search (Mehlhorn and
Näher, LEDA, 1999): d(s) = 0; d(v) ≤ d(u) + w(u, v) on every arc, which bounds every s–v path below by
d(v); every reached v has a tight parent arc, d(v) = d(π(v)) + w, and following π reaches s, which
exhibits a path of length d(v). Sums are compared as the same floating-point additions the search
performs.

**Results.** Every one of the 15,000 breadth-first and 15,000 Dijkstra test graphs is solved with all
parents equal to the reference and a certified tree (`results/salsa.jsonl`). Published neural
baselines at n = 1,600 without hints (arXiv 2309.12253, Tables 1–4; ER / WS / Delaunay):

| Algorithm | Best node accuracy | Graphs fully correct | This repository |
| --- | --- | --- | --- |
| BFS | 99.2 / 82.1 / 55.6 (RecGNN) | 0.4 / 0.0 / 0.0 | 100 / 100 / 100, both measures |
| Dijkstra | 92.2 / 72.5 / 51.0 (PGN) | 0.0 / 0.0 / 0.0 | 100 / 100 / 100, both measures |

The networks are trained on graphs of at most 16 nodes and must extrapolate; the exact search has no
size to extrapolate from. Each graph has one source and asks for the whole tree, so there is no query
workload over which a partition could repay its cost: hypergraph methods have nothing to improve here.

## Korea road network: does a hypergraph partition help?

**Data.** [Korea Road Network ETA](https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta)
(CAMUS Lab; CC BY-NC 4.0; topology from OpenStreetMap, speeds observed by KTDB in 2024). Arc weights
are weekday morning-peak travel times in seconds, length / speed with speeds clipped below at 5 km/h.
A two-way segment gives two arcs. `scripts/roads.py` converts one region at a time.

**Workload.** Many point-to-point queries on one graph. A two-level overlay (customizable route
planning, Delling, Goldberg, Pajor and Werneck 2011) partitions the edges into cells; a node incident to
edges of two cells, or to a cut edge, is a boundary node. Preprocessing stores the in-cell distances
between the boundary nodes of each cell. A query runs Dijkstra over the arcs of the cells of s and t
plus the stored distances. Every shortest path splits at boundary nodes into in-cell pieces, so the
answer is exact; on every query it equals plain Dijkstra's.

**Two cut models, one partitioner.** Cells come from recursive inertial-flow bisection (Schild and
Sommer 2015): project the nodes on four directions, take the first and last quarters as sources and
sinks, and compute a minimum cut between them by augmenting paths, until cells have at most U nodes.

- *Graph model:* the cut is a set of edges. Each cut edge makes both of its end nodes boundary nodes.
- *Hypergraph model:* the cut is a set of nodes, shared by the cells on both sides. Equivalently, the
  dual hypergraph (one hyperedge per node, spanning that node's edges) is bisected under the
  connectivity metric, which counts boundary nodes directly. Where a source touches a sink no node cut
  exists and an edge cut is taken.

Picking one end of every cut edge gives a node cut, so a minimum node cut is never larger than a minimum
edge cut, and it creates one boundary node where an edge cut creates two. The prediction is about half
the boundary nodes at equal U; the measurements below agree.

**Results.** Two regions, uniform random node pairs (`results/roads.jsonl`). Plain Dijkstra stops when
it settles t:

| Region | Nodes | Arcs | Queries (reachable) | Settled | Arcs scanned | Query µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sejong | 85,647 | 164,671 | 1,000 (937) | 41,650 | 80,158 | 3,148 |
| Seoul | 328,610 | 676,989 | 500 (486) | 162,191 | 334,208 | 16,282 |

Overlays, graph model → hypergraph model, at the same cell bound U (the two models produce about the
same number of cells). Every overlay distance equals plain Dijkstra's, and three full Dijkstra trees
per region are certified.

| Region, U | Boundary nodes | Overlay arcs | Settled | Arcs scanned | Query µs, paired | Build s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sejong, 256 | 2,690 → 1,304 | 13,037 → 9,856 | 1,691 → 948 | 10,966 → 7,860 | 374 → 187 | 2.0 → 2.3 |
| Sejong, 1,024 | 992 → 458 | 6,394 → 4,924 | 1,532 → 1,297 | 6,840 → 5,566 | 223 → 154 | 2.0 → 2.0 |
| Sejong, 4,096 | 375 → 177 | 3,822 → 2,732 | 3,973 → 4,114 | 9,964 → 9,635 | 372 → 352 | 1.3 → 2.3 |
| Seoul, 256 | 19,099 → 9,081 | 183,331 → 139,238 | 10,093 → 4,929 | 119,102 → 85,394 | 3,134 → 1,500 | 13.9 → 29.9 |
| Seoul, 1,024 | 7,349 → 3,471 | 101,665 → 80,416 | 4,859 → 2,825 | 64,096 → 49,060 | 1,411 → 805 | 13.4 → 29.3 |
| Seoul, 4,096 | 2,823 → 1,327 | 51,585 → 43,119 | 5,654 → 4,760 | 39,661 → 33,620 | 984 → 733 | 15.9 → 25.6 |

Settled, scanned and query time are means per query. Each query is answered by every method in turn,
starting from a different method each time, so clock drift is shared. Absolute times varied between
earlier unpaired runs by up to 2.5×.

**The hypergraph model helps.** At equal U it has 51–54% fewer boundary nodes, as predicted, and
16–29% fewer overlay arcs. Queries settle 15–51% fewer nodes, except Sejong at U = 4,096 (+4%), and
take 5–52% less time. The gain is largest for small cells, where boundary nodes are a large share of
the search; with large cells the search inside the cells of s and t dominates, and those cells are the
same size in both models. Query time follows settled nodes (heap operations) more closely than arcs
scanned: a separator node belongs to several cells and carries the stored distances of each, so the
hypergraph overlay scans more arcs per settled node (Seoul, U = 256: 17.3 against 11.8), but the fall
in settled nodes outweighs it. The best overlay answers a query 20–22 times faster than plain
Dijkstra in both regions (hypergraph model; graph model 14–17 times). Preprocessing takes 1.0–2.2
times as long, because the node-split flow network has twice the nodes.

## Limits

- One partitioning level and one bisection heuristic; a multilevel overlay or a dedicated hypergraph
  partitioner (KaHyPar, Mt-KaHyPar) may change the margins. The baseline is one-directional Dijkstra;
  bidirectional search and contraction hierarchies are not compared.
- Timings are one paired run on a laptop (Intel Core i5-1335U); node, arc, settled and scanned counts
  are deterministic. Two regions of sixteen were run; the nationwide graph was not.
- OpenStreetMap ways are themselves hyperedges over their nodes; that structure is not used here.
- SALSA-CLRS reference parents for distributed MIS, DFS, spanning trees and eccentricity are not scored.
