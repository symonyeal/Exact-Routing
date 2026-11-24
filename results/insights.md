# What the cuts and timers change

The spatial and timing evidence answer different questions. Hypergraph cuts consistently reduce the boundary interface in this study. The reusable search also charges the previous query's state cleanup to the next call, so query-group medians depend on their order.

## The controlled replay changes timing context, not answers

The registered replay uses Sejong and London-large, the same 1,000 native-weight pairs, three passes, Dijkstra and Hypergraph U=1,024. Conditions run in a rotating order. The method-order schedule is tied to the frozen query index, so it stays identical for that query and pass when the sequence reverses. Six full reference trees are independently certified; all 36,000 measured answers match the frozen certified references. Settled/scanned counts for each current query are asserted identical in every condition.

| Network | Method | Group | Fixed order p50, us | Reversed order p50, us | Pre-cleared query p50, us | Preparation p50, us | Preparation + query p50, us |
|---|---|---|---:|---:|---:|---:|---:|
| sejong | dijkstra | walk8 | 88.03 | 0.60 | 0.70 | 101.73 | 102.53 |
| sejong | dijkstra | walk32 | 0.87 | 98.27 | 1.07 | 0.10 | 1.17 |
| sejong | Hypergraph-1024 | walk8 | 7.97 | 0.87 | 0.87 | 8.17 | 9.13 |
| sejong | Hypergraph-1024 | walk32 | 1.63 | 8.40 | 1.70 | 0.27 | 2.03 |
| london-large | dijkstra | walk8 | 47.83 | 0.87 | 0.67 | 45.03 | 47.83 |
| london-large | dijkstra | walk32 | 2.03 | 57.07 | 2.20 | 0.07 | 2.23 |
| london-large | Hypergraph-1024 | walk8 | 5.73 | 1.23 | 1.03 | 3.77 | 4.83 |
| london-large | Hypergraph-1024 | walk32 | 3.37 | 9.33 | 2.97 | 0.10 | 3.13 |

The fixed cycle is 8-arc walk, 32-arc walk, uniform, uniform. The 8-arc walk therefore clears the previous uniform query's touched entries. Reversal changes which group inherits that state. In Sejong Dijkstra, the median prior touched count for walk8 shifts from 41,594 to 17; walk32 shifts from 7 to 37,795.

Preparation runs an identity query outside the actual query timer, pays prior cleanup, and leaves one touched entry. Its cost is timed and retained. This condition does not make preparation free. The independently calculated group medians in the columns need not add up. P50 uses the lower median of per-query mean times over three passes.

Reversal also changes cache and temporal context. These results do not attribute every microsecond of the difference to cleanup alone. They demonstrate that the original group labels are not context-free search costs. The source explicitly clears touched distances/flags at the beginning of each call; the preparation condition makes that work visible. These are local exploratory measurements, not competitive timing records.

## Half the boundary does not imply half the latency

Each row below aggregates eleven matched native-weight configurations at one cell bound. Ratios divide graph time by hypergraph time; greater than one favors hypergraph.

| Cell bound | Median boundary reduction | Median ratio of mean times | Median ratio of p50 times | Hypergraph p50 wins |
|---:|---:|---:|---:|---:|
| 256 | 51.6% | 1.76x | 1.22x | 11 / 11 |
| 1,024 | 52.5% | 1.52x | 1.09x | 8 / 11 |
| 4,096 | 52.6% | 1.15x | 0.97x | 4 / 11 |

Across all 33 native comparisons the boundary reduction ranges from 49.8% to 56.5%, median 52.2%. Hypergraph has the lower mean in all 33; it has the lower observed p50 in 23. At U=4,096, seven cities have a slower hypergraph p50 despite the smaller boundary. Small timing differences have no confidence interval here and may include noise. These are results for the declared fixed sequence, including prior-query cleanup.

## A spatial decomposition of one actual query

For Sejong, U=256, native seconds, frozen query #3 (uniform pair 80,385 to 51,842), both overlays match the certified travel cost of 1,043.070324896 seconds. The actual Rust search masks give:

| Search | Settled boundary nodes | Settled interior nodes | Total settled |
|---|---:|---:|---:|
| Graph overlay | 933 | 283 | 1,216 |
| Hypergraph overlay | 452 | 287 | 739 |

There are 481 fewer settled boundary nodes and four more interior nodes, a net saving of 477. For this query, almost all the saved work is at the interface. Plain Dijkstra settles 24,908 nodes. This is one disclosed query, not an average or a claim that interior work always stays constant. The viewer includes ten fixed presets and all 33 frozen aggregate comparisons.

The diagrams color actual arc-cell assignments and show actual settled-node masks. Cell IDs are independent between models, and equal node bounds do not imply equal cell counts. The exporter checks its cell, boundary, overlay-arc and stored-array totals against the frozen build rows, certifies the full reference tree for each preset, and checks both query answers before it emits geometry.

## Reproduce and inspect

```text
python scripts/query_order.py
python scripts/insights.py --output maps/insights.js
python scripts/serve_maps.py
```

For the spatial case, after fetching the pinned Sejong graph:

```text
cargo run --release --locked --bin partition_view -- data/roads/sejong.bin results/metro.jsonl sejong 256 0
```

The frozen city timings in `metro.jsonl` remain unchanged, SHA-256 `04b5e828613139384cbe18f6e91ba19c6821f2965b8bb1d6dbca23ebc6185808`, producer `cee8c6b`. The controlled replay is separate in `query-order.json`, including raw per-query mean/preparation costs, prior touched counts, per-query work and compiler/source/executable provenance. It was measured during implementation; the dirty-tree flag remains explicit and canonical source hashes identify the tested Rust code.

The replay used Intel64 Family 6 Model 186 Stepping 3, GenuineIntel; Windows-11-10.0.26200-SP0; 12 logical CPUs; rustc 1.99.1 (58cf8f9c3 2026-10-13); RUSTFLAGS `default`. Absolute times are not compared with Linux runner timings.

The next performance experiment is to compare touched-list clearing with epoch-stamped state, randomize query order, and retain full API costs alongside separately attributed preparation/search costs. That optimization has not been measured here.
