# Exact Routing

Shortest paths solved exactly, each with a proof of optimality. On the Sapient Maze-Hard benchmark
the solver reproduces all 1,000 held-out reference answers, in about 3 µs and at most 2.3 KB of heap
per maze on a laptop.

## Background

Maze-Hard ([sapientinc/maze-30x30-hard-1k](https://huggingface.co/datasets/sapientinc/maze-30x30-hard-1k))
is a standard test for neural reasoning models: 1,000 training and 1,000 test mazes of 30×30 cells, each
with a start S, a goal G and an answer grid marking a shortest path. Models such as HRM and TRM read
the maze and write the answer grid, and are usually scored on exact match with the reference. Every
maze has several shortest paths, so exact match also requires the dataset's choice among them, which
the dataset does not document.

## The reference answer is a rule

On the training split, 336 candidate tie-breaking rules were scored (`scripts/ties.py`,
`results/ties.txt`). One rule, in two equivalent forms, reproduces all 1,000 training answers: **the
reference is the lexicographically smallest shortest move sequence from S to G, with moves ordered
Down < Left < Right < Up.** The next best rule reproduced 956. The rule was fixed before the test split
was downloaded and then scored once.

| Split | Mazes | Exact match | Proved shortest | Rating = path cells |
| --- | ---: | ---: | ---: | ---: |
| train | 1,000 | 1,000 | 1,000 | 1,000 |
| test | 1,000 | 1,000 | 1,000 | 1,000 |

[`docs/grid.md`](docs/grid.md) proves that greedy descent and first-discoverer breadth-first
search both produce this path (Propositions 1 and 2), which is why both forms matched.

Published results on the same 1,000 test mazes:

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
was not checked for its metric. The learned models must discover search from examples; this repository
runs a search directly, so the table measures the benchmark's ceiling, not learning ability. It also
shows that exact match on Maze-Hard is fully determined by one simple rule.

## Speed and memory

End to end, from the question string to the answer grid, on the 1,000 test mazes. One run on an Intel
Core i5-1335U laptop restricted to its two performance cores; medians of paired per-maze times
(`results/bench.md`). The intervals are bootstrap intervals over mazes within that run; between runs
and cores the times vary by about 20%.

| Method | Exact | Optimal | Median µs | Time relative to wave (95% CI) | Peak heap |
| --- | ---: | ---: | ---: | ---: | ---: |
| wave (this repository) | 1,000 | 1,000 | 3.28 | 1.00 | 2.3 KB |
| wave with certificate | 1,000 | 1,000 | 10.99 | 3.33 (3.32–3.34) | 5.7 KB |
| queue breadth-first search | 1,000 | 1,000 | 4.11 | 1.24 (1.24–1.25) | 11.8 KB |
| `pathfinding` crate, BFS | 1,000 | 1,000 | 14.76 | 4.47 (4.43–4.50) | 41.5 KB |
| `pathfinding` crate, A* | 52 | 1,000 | 17.79 | 5.43 (5.38–5.53) | 55.3 KB |

Peak heap is the maximum over the test mazes and includes the 900-byte answer grid. The queue search
is a direct array-queue implementation in this repository with the same tie rule. A* finds shortest
paths but breaks ties differently, so 52 answers match. On all 12 logical processors the wave solved
between 0.57 and 0.90 million mazes per second across two runs.

On generated n×n grids with the same wall density, one run had the wave ahead by roughly 1.4 to 2.7
times from n = 30 to n = 512 and behind at n = 1024 (5 mazes). Its work grows with path length times
grid area, while a queue search grows with area alone.

## How it works

1. **Wave.** Cells are bits, two 30-cell rows per 64-bit word. Each breadth-first layer from G is
   computed for a whole block of rows at once, four words per AVX2 instruction (Lee 1961).
2. **Labels.** Each reached cell keeps its distance modulo 3 in two bits, enough to tell which
   neighbour is one step closer to G (Akers 1967). The open-cell plane, labels and wave buffers take
   840 bytes for a 30×30 maze.
3. **Descent.** From S, take the first move in Down, Left, Right, Up order that brings the path one step
   closer. This is the lexicographically smallest shortest path.
4. **Certificate.** A potential p with p(G) = 0, |p(u) − p(v)| ≤ 1 across every open edge and
   p(S) equal to the path length proves that no shorter path exists (linear-programming duality).
   With p(S) = ∞, it proves that G is unreachable. `check` verifies both independently of the search;
   it shares only the parser, which the tests check against the raw grid.

The recent breakthroughs in single-source shortest paths concern real-valued weights; unit-cost
grids already admit linear-time search, so the gains available here are constant factors in time
and memory. [`docs/grid.md`](docs/grid.md) gives the formulation, proofs, costs and sources.

## Limits

- The tie rule is inferred from 2,000 of 2,000 answers; the dataset's generator code is not public.
- Timings come from one laptop; CI prints a second run on a Linux runner.
- The certificate is optional and triples the time.
- Grid reductions (dead-end pruning, corridor contraction) are not implemented. Sparse graphs,
  resource-constrained paths, network flows, multi-agent routing and convex extensions are not built
  yet.
- The dataset declares no licence, so it is fetched by pinned revision and SHA-256 rather than stored.

## Project map

| Path | Contents |
| --- | --- |
| `src/grid.rs` | grid parser, wave, descent, potential, certificate check |
| `src/bin/maze.rs` | scores Maze-Hard CSV files: certified, exact and rating counts per file |
| `tests/grid.rs` | every wall pattern of eight shapes up to 4×4 against exhaustive path enumeration (every S–G pair up to 9 cells, one random pair above), wide grids against queue search, certificate mutations |
| `benches/grid.rs` | timing, peak heap and generated-grid scaling against the rivals |
| `scripts/fetch.py` | downloads the pinned CSVs into `data/` and checks their hashes |
| `scripts/ties.py` | scores the 336 tie-breaking rules on the training split |
| `results/` | `eval.jsonl` (checked in CI), `bench.md`, `ties.txt` |

```text
python scripts/fetch.py
cargo test --release
cargo run --release --bin maze -- data/train.csv data/test.csv
cargo bench --bench grid -- data/test.csv
python scripts/ties.py train
```

Rust 1.87 or later. MIT licence.
