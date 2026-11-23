# Benchmark, 2026-11-23

`cargo bench --bench grid -- data/test.csv` on an Intel Core i5-1335U laptop (Windows 11, Rust 1.99.1,
AVX2 kernel selected at run time). The process ran at high priority, restricted to the two performance
cores (logical processors 0–3), because unpinned runs varied by about 20% between performance and
efficiency cores. The thread line comes from a separate unpinned run on all 12 logical processors.
Times include parsing the question and writing the answer grid.

## data/test.csv: 1000 mazes, 30×30

| method | exact | optimal | median µs | p95 µs | mazes/s | time / wave (95% CI) | peak heap B |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| wave | 1000 | 1000 | 3.28 | 3.58 | 303140 | 1.00 (1.00–1.00) | 2300 |
| wave + certificate | 1000 | 1000 | 10.99 | 11.67 | 91212 | 3.33 (3.32–3.34) | 5672 |
| queue | 1000 | 1000 | 4.11 | 4.67 | 244382 | 1.24 (1.24–1.25) | 11792 |
| pathfinding bfs | 1000 | 1000 | 14.76 | 16.52 | 71570 | 4.47 (4.43–4.50) | 41488 |
| pathfinding astar | 52 | 1000 | 17.79 | 24.04 | 53303 | 5.43 (5.38–5.53) | 55312 |

| stage | parse | solve | render | potential | check |
| --- | ---: | ---: | ---: | ---: | ---: |
| median µs | 0.58 | 2.66 | 0.11 | 3.37 | 4.39 |

wave on 12 threads (unpinned): 899770 mazes/s

## Generated n×n grids, wall probability 0.37

| n | mazes | mean D | wave µs | queue µs | pathfinding bfs µs |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 30 | 200 | 35 | 1.6 | 2.6 | 9.4 |
| 64 | 93 | 77 | 10.5 | 23.4 | 56.8 |
| 128 | 46 | 168 | 51.1 | 136.9 | 254.8 |
| 256 | 23 | 310 | 302.7 | 727.8 | 1353.6 |
| 512 | 11 | 620 | 1903.2 | 2711.1 | 11183.5 |
| 1024 | 5 | 1240 | 17901.4 | 13880.4 | 35797.0 |
