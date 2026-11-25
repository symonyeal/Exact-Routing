# Exact Routing

Rust engines for exact shortest-path queries, and a benchmark harness that measures what each engine
costs per request. Every benchmark answer is checked against an independently certified optimal
answer, so speed is never bought with a wrong route.

**Results page: <https://symonyeal.github.io/Exact-Routing/>**

## The question this answers

Navigation, delivery dispatch and ride-hailing price quotes send a stream of "fastest route from A to B"
requests to a routing engine. Two properties of that engine reach the business: compute per request,
and how quickly it absorbs new travel times (traffic, closures). Engines trade one against the other by
precomputing structure on the road network.

This repository measures five exact engines on the same 1,000 requests in each of eleven city networks
(eight Korean regions; London, New York and Tokyo snapshots): 22,000 certified answers, none wrong.
Half the requests are uniform random pairs and half short random walks; 626 have identical endpoints
and 596 are unreachable, all retained.

| Engine | Core-hours per billion requests | Mean request, µs | Full rebuild, s |
| --- | ---: | ---: | ---: |
| Plain Dijkstra | 624 (225–1,812) | 2,367 (831–7,747) | none |
| Bidirectional Dijkstra | 509 (146–1,228) | 1,989 (549–4,611) | none |
| Two-level overlay, edge cut | 46 (14–93) | 217 (63–495) | 3.5 (0.8–16.3) |
| Two-level overlay, junction cut | 30 (10–57) | 147 (48–408) | 9.0 (1.7–47.9) |
| Contraction hierarchy (`fast_paths` crate) | 3.7 (1.5–4.9) | 37 (17–59) | 0.8 (0.2–2.9) |

Median over the eleven cities, range in brackets. Integer-millisecond travel times, which the
contraction hierarchy requires; one worker thread on a 4-vCPU Linux CI runner per city, so compare
engines within a city. Core-hours = 10⁹ ÷ one-worker requests per second ÷ 3,600, from a separate
throughput run that includes worker launch and checking every answer against the reference. Mean
request time is the latency run's mean over three passes; it includes clearing the previous request's
state (see below) and, for the contraction hierarchy, path reconstruction. Overlays: best of three
cell sizes per city, chosen on the same throughput run. Overlay rebuild is the median of three trials;
the contraction hierarchy was built once.

What follows from the table:

Percentages below are medians of the per-city reductions, so they differ from ratios of the table's
medians (509 ÷ 624 gives 18% for bidirectional search, against a 31% median per city).

- **Precomputation is the cost lever.** Overlays need 95–96% less compute per request than plain
  Dijkstra; the contraction hierarchy 99.6% less.
- **The contraction hierarchy was cheapest in all eleven cities**, at 13% of the best overlay's compute,
  and a full rebuild took at most 2.9 seconds (Seoul, 328,610 junctions).
- **Whether overlays win when travel times change is not settled here.** Production engines re-weight
  overlay cells to take in traffic. OSRM's
  [traffic notes](https://github.com/Project-OSRM/osrm-backend/wiki/Traffic) (last edited 2022) say MLD
  "calculates routes more slowly than CH, but traffic imports are significantly faster"; customizable
  route planning, described by
  [its authors](https://www.microsoft.com/en-us/research/publication/customizable-route-planning-in-road-networks/)
  in 2013 as the core of Bing Maps' engine, "can incorporate a new metric in less than a second". This
  repository times partitioning and re-weighting together, and those full overlay rebuilds were slower
  than the contraction hierarchy's (9.0 against 0.8 seconds at the median). At these network sizes a
  contraction rebuild takes seconds; timing overlay re-weighting alone against it is the measurement
  that would decide the case.
- **Bidirectional search without precomputation** saves 31% at the median but cost more than plain
  Dijkstra in two cities: Daegu (index 128, plain Dijkstra = 100) and Incheon (140). They are the two
  cities with the most unreachable pairs (178 and 321 of 1,000); whether that causes it is untested.
  By median latency, bidirectional search is faster than plain Dijkstra in Daegu.

The [results page](https://symonyeal.github.io/Exact-Routing/) shows each city, the cut comparison, one
search drawn on the Sejong map and the benchmark-order effect below.

## Findings

### Cutting overlay cells at junctions halves the boundary

An overlay splits the network into cells of at most U junctions and stores travel times between each
cell's boundary points; a request searches its two end cells plus those stored times. Cells come from
recursive inertial-flow bisection under two cut models. An **edge cut** cuts road segments, so both
ends of each cut segment become boundary points. A **junction cut** cuts at junctions shared by both
sides (a bisection of the dual hypergraph), so each boundary junction counts once.

Junction cut ÷ edge cut, median of eleven cities, unrounded travel seconds:

| Cell bound U | Boundary points | Stored shortcuts | Junctions searched | Mean request | Median request | Build time | Junction cut has lower median |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 256 | 0.48 | 0.76 | 0.53 | 0.57 | 0.82 | 2.12 | 11 of 11 |
| 1,024 | 0.47 | 0.78 | 0.72 | 0.66 | 0.91 | 2.23 | 8 of 11 |
| 4,096 | 0.47 | 0.84 | 0.94 | 0.87 | 1.03 | 2.31 | 4 of 11 |

The boundary halves at every cell size (49.8–56.5% fewer points over all 33 comparisons). The request
gain shrinks as cells grow; with larger cells, work inside cells can offset the smaller boundary. The
junction cut's flow network has twice the nodes, and its build takes about twice as long. It had the
lower mean request time in all 33 comparisons. Small median differences may be timing noise; no
confidence intervals are computed. On one Sejong request (U = 256) the junction cut searches 452
boundary and 287 interior junctions against 933 and 283 for the edge cut; plain Dijkstra searches
24,908. Over the six uniform-pair requests exported for Sejong, boundary work falls 51–52% while
interior work changes by −18% to +31%.

### Request order can dominate small timings

The search engines reuse memory and clear the previous request's touched entries at the start of the
next call. In the fixed benchmark order, short 8-step requests followed long uniform ones and inherited
their cleanup: in Sejong, the reported plain-Dijkstra median for that group fell from 88.0 µs to
0.70 µs (99%) once cleanup was timed separately. Reversing the order moved the cost to the other
group; all 36,000 answers and every request's search work were identical across conditions. Reversal
also changes cache and temporal context, so not every microsecond is attributed to cleanup. Medians
over all requests move too (Sejong plain Dijkstra, local: 172 µs in fixed order, 14 µs with cleanup
separated), while means hardly move (2,539 and 2,394 µs); the table above therefore reports means.
Randomized order is the next experiment. Details: [`results/insights.md`](results/insights.md).

### Learned solvers on public reasoning benchmarks

[Maze-Hard](https://huggingface.co/datasets/sapientinc/maze-30x30-hard-1k) (1,000 test mazes,
30×30) and [SALSA-CLRS](https://huggingface.co/datasets/SALSA-CLRS/SALSA-CLRS) test whether neural
models learn to search. Exact search sets their ceiling.

Maze-Hard scores exact match with a reference grid, and every maze has several shortest paths. On the
training split, 336 candidate tie rules were scored (`scripts/ties.py`, `results/ties.txt`); one rule
reproduces all 1,000 answers: **the lexicographically smallest shortest move sequence, with moves
ordered Down < Left < Right < Up**. The next best rule reproduced 956. The rule was fixed before the
test split was downloaded and then scored once: 1,000 of 1,000 test answers match, each proved shortest,
and every rating equals the path's cell count. The dataset does not document its tie choice.

| Method | Kind | Exact match | Any shortest path | Source |
| --- | --- | ---: | ---: | --- |
| This repository | exact search with certificate | **100.0%** | **100.0%** | `results/eval.jsonl` |
| LoopedWSP looped transformer | hand-written weights | — | 100% | [MBZUAI, 2026](https://mbzuai-oryx.github.io/LoopedWSP/) |
| LDT, 1.8M parameters | learned | — | 99.8% | [arXiv 2605.08605](https://arxiv.org/abs/2605.08605) |
| TRM, best checkpoint, seeds 0 / 1 | learned | 94.0% / 95.0% | — | [arXiv 2611.23677](https://arxiv.org/abs/2611.23677), Table 3 |
| STRM, best checkpoint, seeds 0 / 1 | learned | 95.1% / 93.3% | — | [arXiv 2611.23677](https://arxiv.org/abs/2611.23677), Table 3 |
| GFPR | learned | 92.0% | — | [arXiv 2609.33540](https://arxiv.org/abs/2609.33540) |
| EBR, one sample / five samples | learned | 87.4% / 88.3% | — | [arXiv 2611.08977](https://arxiv.org/abs/2611.08977), Table 1 |
| TRM | learned | 85.3%, as reported | — | [arXiv 2510.04871](https://arxiv.org/abs/2510.04871) |
| HRM | learned | 74.5%, as reported | — | [arXiv 2506.21734](https://arxiv.org/abs/2506.21734) |

The LDT figure counts any shortest path, as stated by the LoopedWSP authors who cite it. The HRM source
was not checked for its metric. The table measures the benchmark's ceiling, not learning ability.

Grid solver speed on the 1,000 test mazes, question string to answer grid, Intel Core i5-1335U laptop
pinned to its two performance cores; medians of paired per-maze times, bootstrap 95% intervals over
mazes within one run (`results/bench.md`). Times vary by about 20% between runs and cores.

| Method | Exact | Optimal | Median µs | Time relative to wave (95% CI) | Peak heap |
| --- | ---: | ---: | ---: | ---: | ---: |
| wave (this repository) | 1,000 | 1,000 | 3.28 | 1.00 | 2.3 KB |
| wave with certificate | 1,000 | 1,000 | 10.99 | 3.33 (3.32–3.34) | 5.7 KB |
| queue breadth-first search | 1,000 | 1,000 | 4.11 | 1.24 (1.24–1.25) | 11.8 KB |
| `pathfinding` crate, BFS | 1,000 | 1,000 | 14.76 | 4.47 (4.43–4.50) | 41.5 KB |
| `pathfinding` crate, A* | 52 | 1,000 | 17.79 | 5.43 (5.38–5.53) | 55.3 KB |

Peak heap is the maximum over the test mazes and includes the 900-byte answer grid. The queue search
is a direct array-queue implementation in this repository with the same tie rule; A* finds shortest
paths but breaks ties differently. On all 12 logical processors (unpinned) the wave solved 0.90 million
mazes per second in the retained run. On generated n×n grids
with the same wall density, one run had the wave ahead by 1.4–2.7 times from n = 30 to 512 and behind
at n = 1,024 (5 mazes): its work grows with path length times grid area, a queue search's with area.

SALSA-CLRS asks for the breadth-first or Dijkstra parent of every node on sparse graphs of 16 to 1,600
nodes, with models trained on at most 16. All 30,000 test trees equal the reference and are certified;
mean search time at n = 1,600 is 39–156 µs for breadth-first search and 165–343 µs for Dijkstra
(`results/salsa.jsonl`). The best models without hints in the 2023 paper
([arXiv 2309.12253](https://arxiv.org/abs/2309.12253)) solve at most 0.4% of 1,600-node graphs fully. Details: [`docs/sparse.md`](docs/sparse.md).

## How it works

```text
pinned dataset revisions
  scripts/fetch.py, salsa.py, roads.py, international.py   download, convert, hash   -> data/
src/grid.rs      maze parser, word-parallel wave, descent, potential, certificate check
src/graph.rs     sparse graph: direction-optimizing BFS, Dijkstra, shortest-path-tree certificate
src/road.rs      validated road binary: coordinates, arcs, travel times
src/overlay.rs   inertial-flow partition (edge or junction cut), two-level overlay, query
src/query.rs     reusable bidirectional Dijkstra
src/ch.rs        validated adapter to the fast_paths contraction hierarchy
src/bin/metro.rs certified workload, paired latency, throughput, builds   -> results/metro.jsonl
scripts/metro.py validate all eleven cities, write the report               -> results/metro.md
scripts/page.py  normalize the retained results for the page                -> site/data.js
```

**Certificates.** A route is accepted only with proof that nothing shorter exists. For a road network,
a shortest-path tree is valid when no arc can shorten any distance and every parent arc is tight and
leads back to the source; `graph::check` verifies this without trusting the search. For a maze, a
potential p with p(G) = 0, |p(u) − p(v)| ≤ 1 across every open edge and p(S) equal to the path length
proves optimality by linear-programming duality; p(S) = ∞ proves the goal unreachable. `check`
verifies it independently of the search and shares only the parser, which the tests check against
the raw grid. The certificate is optional: the 3.28 µs timing is without it, 10.99 µs with it. Each
city request is checked against a certified reference tree.

**Maze solver.** Cells are bits, two 30-cell rows per 64-bit word. Each breadth-first layer from G is
computed for a block of rows at once, four words per AVX2 instruction (Lee 1961). Each reached cell
keeps its distance modulo 3 in two bits, enough to find the neighbour one step closer to G
(Akers 1967); the open-cell plane, labels and wave buffers take 840 bytes. From S, the walk takes the
first move in Down, Left, Right, Up order that brings it one step closer, which is the reference path.
[`docs/grid.md`](docs/grid.md) proves this and gives the costs and sources. Recent single-source
shortest-path advances concern real-valued weights; unit-cost grids already admit linear-time search,
so the gains here are constant factors.

## Reproduce

Rust 1.87 or later; Python 3 with numpy (SALSA-CLRS, international snapshots, figure), pandas and
pyarrow (Korea roads), matplotlib (figure).

```text
python scripts/fetch.py                       # Maze-Hard CSVs, hash-checked
cargo test --release
cargo run --release --bin maze -- data/train.csv data/test.csv
cargo bench --bench grid -- data/test.csv
python scripts/ties.py train
python scripts/salsa.py dijkstra              # about 2 GB of text for all 15 sets
cargo run --release --bin salsa -- dijkstra data/salsa/dijkstra/*.txt
python scripts/roads.py sejong
cargo run --release --bin roads -- data/roads/sejong.bin 1000
python scripts/international.py london large  # published travel minutes become seconds
cargo build --release --bin metro
python scripts/metro.py run data/roads/london-large.bin results/metro/london-large.jsonl
python scripts/metro.py report results/metro.jsonl --output results/metro.md
python scripts/query_order.py                 # request-order diagnostic, local
python scripts/page.py                        # site/data.js from results/
python scripts/figure.py                      # site/sejong-query.png, needs data/roads/sejong.bin
```

The eleven-city study runs in `.github/workflows/metros.yml` from benchmark code
[`cee8c6b`](https://github.com/symonyeal/Exact-Routing/tree/cee8c6b56118351c690f68645f0da89717663d5d):
ten cities in [run 45282401948](https://github.com/symonyeal/Exact-Routing/actions/runs/45282401948),
Seoul and the full reconciliation in
[run 45417405041](https://github.com/symonyeal/Exact-Routing/actions/runs/45417405041), same code and
workload. Later commits did not rerun the city timings. Two-worker throughput is a median 1.97× the
one-worker figure over 187 city, metric and engine configurations (1.38–2.45×); parallel cell
customization gives small build gains and partitioning stays serial. CI checks formatting, lints, all
tests, the Maze-Hard scores against `results/eval.jsonl`, the city report and the page data against
rebuilds, SALSA-CLRS and Sejong. Protocol: [`docs/sparse.md`](docs/sparse.md).

## Limits

- Overlay re-weighting alone, multilevel overlays, memory of the searches and the contraction
  hierarchy, and cost in currency are not measured (overlay array sizes are in `metro.jsonl`). Customizable contraction hierarchies and hub labels are not implemented.
- One benchmark run per city on CI runners whose processors differ; engine comparisons are paired
  within each city. The maze and two-region road timings come from one laptop. Node, arc and settled
  counts are deterministic.
- The request-order diagnostic is a local Windows measurement and is not comparable with the Linux city
  times; reversal also changes cache and temporal context.
- The Maze-Hard tie rule is inferred from 2,000 of 2,000 answers; the dataset's generator code is not
  public. The grid certificate is optional and triples the time.
- Grid reductions (dead-end pruning, corridor contraction), resource-constrained paths, general
  network-flow benchmarks, multi-agent routing and convex extensions are not implemented.
- The checked Hugging Face leaderboard endpoints had no entries for these datasets; this is a matched
  implementation comparison, not an external record.
- Raw data is fetched at pinned revisions. Maze-Hard declares no licence; SALSA-CLRS is Apache-2.0;
  Korea Road Network ETA is CC BY-NC 4.0; the international snapshots come from InstructOSM (about
  50,000 junctions each, not whole metropolitan areas).

## Repository layout

| Path | Contents |
| --- | --- |
| `src/` | engines and certificate checks (see How it works) |
| `src/bin/` | `maze`, `salsa`, `roads`, `metro` scorers; `partition_view` and `query_order` diagnostics |
| `tests/` | grids against exhaustive path enumeration and queue search; graph searches against Bellman–Ford; overlays against Dijkstra; certificate mutations |
| `benches/grid.rs` | maze timing, peak heap and generated-grid scaling against the rivals |
| `scripts/` | dataset converters, metro runner and report, tie-rule search, page data and figure |
| `results/` | retained measurements: `eval.jsonl`, `bench.md`, `ties.txt`, `salsa.jsonl`, `roads.jsonl`, `metro.jsonl`, `metro.md`, `query-order.json`, `insights.md` |
| `docs/` | `grid.md` (maze proofs and costs), `sparse.md` (SALSA-CLRS, overlays, cut models, city protocol) |
| `site/` | the results page; `data.js` and `sejong-query.png` are generated |

MIT licence.
