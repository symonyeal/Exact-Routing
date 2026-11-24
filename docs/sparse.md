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
sinks, and compute a minimum cut between them by Dinic's blocking flows, until cells have at most U nodes.

- *Graph model:* the cut is a set of edges. Each cut edge makes both of its end nodes boundary nodes.
- *Hypergraph model:* the cut is a set of nodes, shared by the cells on both sides. Equivalently, the
  dual hypergraph (one hyperedge per node, spanning that node's edges) is bisected under the
  connectivity metric, which counts boundary nodes directly. Where a source touches a sink no node cut
  exists and an edge cut is taken.

Node cuts can avoid making both ends of each cut edge boundary nodes. The terminal sets are fixed and
cannot be removed, so this is not a universal bound relating the two objectives: adjacent terminals
require the edge-cut fallback. The boundary reductions below are measurements on these networks.

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
the search. With larger cells, interior search can offset the smaller interface. Both models use
the same upper bound U; their actual cell sizes and interior search work can differ. Query time in
this table follows settled nodes (heap operations) more closely than arcs
scanned: a separator node belongs to several cells and carries the stored distances of each, so the
hypergraph overlay scans more arcs per settled node (Seoul, U = 256: 17.3 against 11.8), but the fall
in settled nodes outweighs it. The best overlay answers a query 20–22 times faster than plain
Dijkstra in both regions (hypergraph model; graph model 14–17 times). Preprocessing takes 1.0–2.2
times as long, because the node-split flow network has twice the nodes.

## Reproducible metropolitan study

`.github/workflows/metros.yml` covers eight Korean city regions and three international snapshots:

| Source | Graphs | Pinned revision | Native arc cost |
| --- | --- | --- | --- |
| [Korea ETA](https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta) | Sejong, Seoul, Busan, Daegu, Incheon, Gwangju, Daejeon, Ulsan | `b111710756ee5727a0fbe3f6203c0cc2f93de2be` | weekday morning travel seconds, as above |
| [InstructOSM](https://huggingface.co/datasets/HBNU-NLP/InstructOSM) | London, New York, Tokyo, large snapshots | `9b0548fc99a53ce509e4458748c2246ae8884fe4` | published `travel_time_min` multiplied by 60 |

The international snapshots each contain about 50,000 nodes; they do not cover the entire metropolitan
area. Explicit directed GraphML arcs and parallel arcs are retained; pickles are not loaded. Both
converters validate inputs and save source and binary SHA-256 hashes alongside the binary.

The implementations are reusable Dijkstra, bidirectional Dijkstra, six graph/hypergraph overlays
(U = 256, 1,024, 4,096), and the Rust [`fast_paths` 1.0 contraction hierarchy](https://github.com/easbar/fast_paths).
Dinic's iterative blocking flows replace repeated augmenting-path BFSs in the partitioner
([original paper](https://www.cs.bgu.ac.il/~dinitz/D70.pdf)). Independent cells customize in scoped
threads; partitioning remains serial. Each query worker owns its distances, heaps and touched lists.

CH requires positive integer weights. Two separate comparisons therefore use native travel seconds
and the changed metric `max(1, round(1000 * seconds))` on every arc. All methods within a comparison use
the same metric. Rounded results do not establish the native-second optimum. The CH library internally
reconstructs a path before the adapter returns its distance; this work is included. These are costs of
the available distance-returning implementations, not isolated search kernels or equal path outputs.

The default protocol freezes 1,000 pairs per graph, seed 20261123, three passes and two throughput
workers. One quarter of pairs follows an 8-arc random walk, one quarter a 32-arc walk, and half are
uniform pairs. Walks stop at dead ends. Identical endpoints and unreachable pairs remain and are
counted. Pairs and references are saved. Every reference comes from a full tree whose certificate
passes; this checks the sampled workload, not every possible pair in a city.
Native floating-point answers allow an absolute difference of `1e-9 * max(1, reference)` for
roundoff; rounded integer answers require strict equality.

Sixteen queries warm each implementation. Single-thread timing rotates the first method on each pair
and pass. Each pair's mean over passes is computed first; p50/p95 are then taken across those means.
The paired CH ratio is the median of per-pair mean-time ratios. Separate walk/uniform medians remain
in the results. Serial and parallel overlay builds alternate order over three trials and report their
medians. Throughput is a separate batch measurement including thread launch/join and answer checking,
with workspace allocation excluded. CPU model, OS, compiler, flags and revision accompany each city.
City jobs may use different processors; method comparisons are paired within a job.

Each reusable search clears its previous touched state at the start of the next call. That cleanup
is included in the measured API cost. In the fixed walk8, walk32, uniform, uniform cycle, the walk8
group therefore inherits a preceding uniform query's cleanup. Group medians describe this sequence,
not isolated costs of the current query. A separate [controlled replay](../results/insights.md)
reverses the same pairs and separately times preparation while retaining its cost in the total.
Reversal also changes cache and temporal context, so it does not isolate every microsecond to cleanup.

`scripts/metro.py` reconciles both metrics, all methods and thread counts, and every required city.
Missing results, failed scorers, wrong answers and input-hash mismatches fail publication.

The retained [eleven-city report](../results/metro.md) reconciles 22,000 individually certified
graph/metric query cases with zero wrong answers. Its [JSONL snapshot](../results/metro.jsonl) records
the exact input hashes, pairs, references, trials and benchmark revision `cee8c6b`. Ten artifacts come
from run 45282401948; Seoul and the complete merge passed in run 45417405041 after increasing the
cloud job limit. Both use the same 1,000-query, three-pass, two-worker protocol.

The report can be rebuilt from the retained snapshot without downloading graphs:

```text
python scripts/metro.py report results/metro.jsonl --output results/metro.md
```

The manual workflow also accepts `city` (one city or `all`), `benchmark_ref` (the code revision to
check out) and `previous_run` (completed artifacts to reuse). A retry keeps the original benchmark
revision and workload; its report still requires the complete eleven-city population.

The source cards checked on 23 November 2026 do not link a comparable exact-routing latency ranking.
Hugging Face's [leaderboard API](https://huggingface.co/docs/hub/leaderboard-data-guide) ranks submitted
model scores; SALSA evaluates reasoning accuracy and InstructOSM supplies instruction tasks. This study
compares a published Rust routing implementation on matching inputs and hardware; it establishes no
external speed record.

## Inspect the algorithm's work

`python scripts/serve_maps.py` opens the local HTML study. Its charts are derived from the frozen
eleven-city results; `python scripts/insights.py --output maps/insights.js` rebuilds them without
downloading graphs. They show the boundary/latency crossover, workload cost distributions and the
separate controlled replay. They also work when opening `maps/index.html` directly.

The spatial comparison requires the local server and the pinned road binary. `/api/partition` runs
`partition_view` for the selected city, cell bound and metric, then caches the validated output in
`data/insights/`. The exporter checks cell, boundary, overlay-arc and array-byte counts against the
frozen benchmark before exporting actual arc-cell assignments, boundary nodes and ten frozen search
traces. Every trace uses a certified reference tree; both overlay distances are checked. Synchronized
views distinguish settled boundary and interior nodes and display their counts beside the geometry.
No arbitrary route query is needed to inspect this evidence.

## Limits

- One partitioning level and one bisection heuristic; a multilevel overlay or a dedicated hypergraph
  partitioner (KaHyPar, Mt-KaHyPar) may change the margins. The original two-region table uses only
  one-directional Dijkstra; the expanded protocol adds bidirectional search and integer CH.
- The original two-region timings are one paired run on a laptop (Intel Core i5-1335U). The expanded
  study records each Linux CI runner's CPU and compiler; methods are paired within each city job.
  Node, arc, settled and scanned counts are deterministic. The nationwide graph was not run.
- Customizable contraction hierarchies, hub labeling and multilevel CRP are not implemented. This
  study measures ordinary CH and a two-level overlay and claims no new routing algorithm.
- OpenStreetMap ways are themselves hyperedges over their nodes; that structure is not used here.
- SALSA-CLRS reference parents for distributed MIS, DFS, spanning trees and eccentricity are not scored.
