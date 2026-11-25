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
- **When travel times change, a customizable contraction hierarchy takes over** once updates pass
  3–79 a day per million daily requests (median city 33); junction-cut overlays with re-weighted
  cells (U = 1,024 and 4,096, one thread) were never the cheapest engine. See
  [What traffic costs](#what-traffic-costs-a-routing-service) below.
- **Bidirectional search without precomputation** saves 31% at the median but cost more than plain
  Dijkstra in two cities: Daegu (index 128, plain Dijkstra = 100) and Incheon (140). They are the two
  cities with the most unreachable pairs (178 and 321 of 1,000); whether that causes it is untested.
  By median latency, bidirectional search is faster than plain Dijkstra in Daegu.

The [results page](https://symonyeal.github.io/Exact-Routing/) shows each city, the cut comparison, one
search drawn on the Sejong map and the benchmark-order effect below.

## What traffic costs a routing service

**A service that updates travel times more than about 3–79 times a day per million daily requests
spends the least compute on a customizable contraction hierarchy (CCH); below about 2–8 updates a
day per million requests a contraction hierarchy (CH) rebuilt per update is cheapest, and between the
two a CH re-contracted in a fixed node order. Above about 210,000–435,000 updates a day per million
requests, more than twice a second, a Dijkstra search without an index is cheapest.** Junction-cut
overlays with a fixed partition (U = 1,024 and 4,096, one thread) re-weighted their cells in 67 ms
at the median city but were never the cheapest engine at any request or update rate: the CCH was
cheaper per request and per update in all eight cities. Routing on the dataset's free-flow speeds
instead of the current slot's made trips 3.6–7.5% slower on average. Routing on the previous slot's
speeds cost 0.2–0.8% over all consecutive slots, and 0.5–2.0% between the four observed slots.

The study answers: a service handles R requests a day on a network whose travel times change K
times a day; which exact engine minimizes compute per day, including keeping its index current?
Daily core-hours are C = R q / 10⁹ + K u / 3,600, with q the core-hours per billion requests and u
the core-seconds per update, both measured on one thread in the same run.

Eight Korean cities, nine travel-time sets each (the dataset's free-flow speed and its eight weekday
time slots), the same 1,000 requests per city as above in integer milliseconds. 72,000 certified
shortest-path trees, one per request and travel-time set, are the reference; every answer an engine
gave was checked against them, none wrong. The indexed engines answered every request on every slot;
the Dijkstra searches answered each request on one slot per pass. Three timing passes, each covering
all eight slots in random order with requests in random order; ranges are the spread over passes.
Request costs average over all 1,000 requests, including those with identical endpoints and those
with no route (requests with a route: Busan 990, Daegu 822, Daejeon 987, Gwangju 987, Incheon 679,
Sejong 970, Seoul 988, Ulsan 992). Bidirectional Dijkstra is given no update cost; its reverse graph
is rebuilt per slot outside the timing.

Median over eight cities (range):

| Engine | q, core-hours per billion requests | u, core-seconds per update | One-time preparation, s |
| --- | ---: | ---: | ---: |
| Plain Dijkstra | 788 (415–2,513) | none | none |
| Bidirectional Dijkstra | 676 (243–1,517) | none | none |
| Overlay, junction cut, U = 1,024 | 23.9 (17.5–75.2) | 0.067 (0.023–0.235) | 9.6 (3.6–51.8) |
| Overlay, junction cut, U = 4,096 | 51.4 (42.6–94.4) | 0.116 (0.042–0.405) | 9.2 (3.0–48.9) |
| CH, rebuilt per update (`fast_paths` 1.0) | 4.0 (3.0–7.8) | 0.802 (0.300–3.11) | none |
| CH, re-contracted in the free-flow node order | 4.5 (3.8–11.5) | 0.249 (0.099–0.896) | 0.8 (0.3–2.7) |
| CCH (this repository) | 6.9 (4.4–14.8) | 0.0074 (0.0029–0.0213) | 11.2 (4.0–55.8) |

Break-even updates per day, from the means over passes (range over the three passes). Both
thresholds scale with request volume: at 10 million requests a day, multiply by ten.

| City | Junctions | CH rebuilt → fixed order, per 10⁶ requests | Fixed order → CCH, per 10⁶ requests | Fixed order → CCH, per 10⁷ requests |
| --- | ---: | ---: | ---: | ---: |
| Sejong | 85,647 | 3 (2–5) | 52 (46–58) | 519 (462–575) |
| Daejeon | 95,765 | 7 (6–9) | 55 (53–57) | 546 (533–568) |
| Gwangju | 132,467 | 7 (7–7) | 79 (77–81) | 791 (768–807) |
| Ulsan | 146,901 | 8 (6–8) | 30 (28–30) | 296 (284–301) |
| Daegu | 153,887 | 4 (4–4) | 24 (22–25) | 238 (223–246) |
| Busan | 157,831 | 4 (4–5) | 36 (34–37) | 359 (343–371) |
| Incheon | 247,862 | 2 | 3 | 28 |
| Seoul | 328,610 | 6 (6–7) | 13 (12–14) | 134 (118–141) |

In Incheon the three hierarchies' break-even points lie within about one update a day per million
requests of each other, and the passes disagree on the order of the switches. In Sejong, the rebuilt
and fixed-order CH request costs are not distinguishable. A Dijkstra search without an index
(bidirectional in six cities, plain in Daegu and Incheon) becomes cheapest only above 210,000–435,000
updates a day per million requests; at 100,000 requests a day that is 21,000–44,000 updates.

What the numbers mean for a service:

- Refreshing traffic every 15 minutes is 96 updates a day. At a million requests a day that favours
  the CCH in every city; at ten million it favours the CCH only in Incheon and the fixed-order CH
  elsewhere.
- Absolute compute is small at these sizes: at ten million requests and 96 updates a day the cheapest
  engine needs at most 0.14 core-hours a day in any city. The choice matters for many cities, larger
  networks or far higher volumes.
- Every update here starts from scratch; none is incremental. The CCH update is a full
  customization in 3–21 ms; a production system would also have to ship the new weights to its
  workers, which is not timed.

Cost of ignoring traffic. For each request, the route chosen on stale times (free flow, or the
previous slot) is priced on the actual slot's times and compared with the best route for that slot.
Regret is the extra travel time; the ETA error compares the stale prediction with the actual optimum.
Means over the eight free-flow transitions, or over the seven informative consecutive transitions
(midday → afternoon is excluded: the two slots are identical). The consecutive transitions include
night → morning peak, which wraps to the start of the next day, and four of the eight slots are
derived from the observed ones, so consecutive slots are closer than real time steps:

| City | Requests | Free flow: mean regret | worst slot p95 | trips ≥ 5% slower | ETA error, mean abs | Previous slot: mean regret | worst slot p95 | trips ≥ 5% slower | route changes | ETA error, mean abs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Sejong | 898 | 3.6% | 26% | 22% | 23% | 0.42% | 6.1% | 2.0% | 25% | 6.7% |
| Seoul | 929 | 7.5% | 33% | 42% | 37% | 0.84% | 9.0% | 4.8% | 38% | 8.5% |
| Busan | 927 | 5.4% | 25% | 31% | 29% | 0.48% | 3.8% | 2.0% | 33% | 6.9% |
| Daegu | 745 | 4.1% | 27% | 24% | 28% | 0.44% | 2.6% | 1.5% | 25% | 6.9% |
| Incheon | 604 | 3.6% | 22% | 20% | 36% | 0.24% | 1.1% | 0.7% | 15% | 6.3% |
| Gwangju | 915 | 5.3% | 29% | 30% | 28% | 0.58% | 3.8% | 2.3% | 30% | 7.1% |
| Daejeon | 922 | 7.3% | 38% | 36% | 26% | 0.59% | 5.7% | 2.7% | 30% | 6.5% |
| Ulsan | 930 | 4.4% | 37% | 27% | 23% | 0.64% | 14.7% | 2.0% | 25% | 6.0% |

Requests are the reachable pairs with distinct endpoints. Between the four observed slots only
(morning peak → midday → evening peak → off-peak → morning peak) mean regret is 0.5–2.0% (Incheon
0.54%, Seoul 1.97%); per-transition rows are in [`results/traffic.md`](results/traffic.md), series
`observed`. The fastest route changes between
consecutive slots for 15–38% of trips, but the old route is rarely much slower: stale ETAs are the
larger error. On the uniform-pair half of the workload, which holds the longer trips, free-flow
predictions underestimate the actual optimum by 9–37% on average (Seoul 37%), and regret is
6.4–13.4%.

Limits of this study:

- The dataset's free-flow speed is a fixed speed by road class (`default_speed_kmh`; the
  `free_flow_speed_kmh` column of its examples file equals it on all 1,048,576 rows that file shares
  with the nationwide file). It is below the observed slot speed on 12–55% of roads, by city and slot
  (Seoul 12–20%, Incheon and Ulsan up to 55%), so free-flow regret mixes congestion with an outdated
  speed table. Four slots are observed (morning peak, midday, evening peak, evening off-peak; KTDB 2024);
  four are derived by the dataset. The derived afternoon slot equals midday on every road in Sejong
  and gives identical travel times for every request in all eight cities. The dataset gives no clock
  hours; the slot order is its column order.
- Workload: the frozen uniform-pair and random-walk requests. No public origin–destination source
  passed verification: KTDB OD tables need KTDB access, the open Korean OD files found count bus and
  subway trips by district, and NYC taxi trips map onto a network with no time slots. LargeST and
  PeMS sensor speeds would need map-matching and were rejected.
- The partition of the overlays reads no travel times, so a partition built on any slot is the same
  partition (a test checks this on four metrics); a weight-aware partition was not tested. The CH
  order for re-contraction comes from the free-flow CH.
- Both CHs and the CCH return the route with each answer; the overlays and Dijkstra searches return
  travel times only, which favours them. No maintained Rust CCH crate was found; the CCH here uses
  nested dissection from the same inertial-flow separators as the overlays and the basic (not
  perfect) customization.
- One CI run per city on 4-vCPU Linux runners, `fast_paths` 1.0, Rust stable. Raw per-city files
  are CI artifacts; [`results/traffic.jsonl`](results/traffic.jsonl) keeps the summaries with each
  raw file's SHA-256 and the source URL, revision, licence and hashes. The Korea Road Network ETA
  data is CC BY-NC 4.0: these results are for non-commercial use.
- Data: [`results/traffic.md`](results/traffic.md); workflow `.github/workflows/traffic.yml`, run
  [45659518061](https://github.com/symonyeal/Exact-Routing/actions/runs/45659518061) at commit
  815df09.

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
src/ch.rs        validated adapter to the fast_paths contraction hierarchy, rebuilt or fixed-order
src/cch.rs       customizable contraction hierarchy: order, elimination, customization, query, paths
src/bin/traffic.rs nine travel-time sets: certified costs, regret, update and request timing
scripts/traffic.py fetch slot speeds, run, break-even and regret summaries -> results/traffic.*
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
python scripts/traffic.py fetch sejong        # nine travel-time sets for one region
cargo build --release --bin traffic
python scripts/traffic.py run sejong results/traffic/sejong.jsonl --queries 1000 --repeats 3
python scripts/traffic.py report results/traffic  # results/traffic.jsonl and traffic.md
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

- Multilevel overlays, incremental updates, memory of the searches and the hierarchies, and cost in
  currency are not measured (overlay array sizes are in `metro.jsonl`). Hub labels are not
  implemented.
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
| `results/` | retained measurements: `eval.jsonl`, `bench.md`, `ties.txt`, `salsa.jsonl`, `roads.jsonl`, `metro.jsonl`, `metro.md`, `query-order.json`, `insights.md`, `traffic.jsonl`, `traffic.md` |
| `docs/` | `grid.md` (maze proofs and costs), `sparse.md` (SALSA-CLRS, overlays, cut models, city protocol) |
| `site/` | the results page; `data.js` and `sejong-query.png` are generated |

MIT licence.
