# Exact Routing

A city-to-nation ranking of the Rust CH/CCH engines, multilevel overlays and RoutingKit CH/CCH is not yet established under the shared path-returning workload: nationwide timings and memory are incomplete.

[Results page](https://symonyeal.github.io/Exact-Routing/) · [Native-router attempt records](results/industry.json)

The comparison asks which engine uses the least compute for R requests and K traffic updates per day:

    C = R q / 10^9 + K u / 3600 core-hours per day

Here q is core-hours per billion requests and u is core-seconds per update. Preparation and memory are separate. The envelope omits preparation amortization, deployment, distribution of new weights and idle capacity.

## Engines and workload

The Rust engines are Dijkstra, bidirectional Dijkstra, `fast_paths` CH rebuilt per update, CH re-contracted in a fixed order, CCH, CCH with four customization threads, partial CCH, perfect CCH, and multilevel overlays with cell bounds 1,024/32,768 and 256/4,096/65,536. Query timings include a node path. Four-thread customization reports core-seconds and wall time separately. CCH variants sharing one query routine pool their query samples.

The external engines are RoutingKit CH and CCH, pinned at [`54d49bb0cdea56dde182357522e4e86a03c57852`](https://github.com/RoutingKit/RoutingKit/tree/54d49bb0cdea56dde182357522e4e86a03c57852), BSD-2-Clause. Both drivers use the same integer-millisecond graph, certified distances and shuffled schedule: each request is timed once per pass, on slot `(query_index + pass) % 8`. Other indexed queries are checked outside timing. Rust engines rotate within a slot; RoutingKit is a separate process. Large networks use separate instances of the same CI runner type, so small cross-process differences need caution.

Sizes are eight Korean cities, Gyeongbuk province, a five-region union and South Korea. The union keeps nationwide nodes at coordinates present in Seoul, Incheon, Gyeonggi, Gangwon or Chungbuk, and nationwide arcs between those nodes. Every size uses three passes and eight weekday metrics. Cities, province and union use 1,000 pairs; the nation uses 200. Half are uniform random endpoints and half short random walks. Identical endpoints and unreachable requests remain in costs; per-size rows carry reachable counts.

Travel times are `max(1, round(1000 × seconds))` milliseconds. A feasible shortest-path tree certifies each reference distance. Rust paths are re-priced outside timing. Bidirectional update cost includes rebuilding the reverse graph. Overlay paths are recovered by searches inside shortcut cells, included in request time. Memory runs prepare one engine in a fresh process; Rust reports peak resident memory above its loaded-input baseline, while RoutingKit records whole-process peak memory. Those scopes differ.

## Results and limits

The original scale run [45706748560](https://github.com/symonyeal/Exact-Routing/actions/runs/45706748560) has incomplete nationwide timing and memory. RoutingKit CH supplement [45722865636](https://github.com/symonyeal/Exact-Routing/actions/runs/45722865636) completed its engine jobs but could not produce the combined report. Earlier Dijkstra, bidirectional and overlay query timings returned distances only. Fresh timing and memory runs are required with the current harness. Nationwide timing now splits by individual slot and pass to reduce peak memory.

`scripts/scale.py report` retains per-pass q and u, preparation, memory, checked counts, provenance, the full cost envelope, K*, growth fits and partial-update u(f). K* is entry into CCH among the Rust BASE engines declared in `scale.py`; the full envelope also includes CCH variants and RoutingKit. CCH can later leave the envelope when an engine with lower update cost becomes cheaper. Overlapping per-pass ranges are reported as not distinguishable. The partial-update crossover f* interpolates in log changed-arc fraction; observed and consecutive slot transitions are separate populations.

OSRM CH/MLD and GraphHopper are excluded after two failed integration attempts each. OSRM v5.27.1 (`4f3ee609ec1af40eb1f445c6706cfa5beb04c990`, BSD-2-Clause) failed configuration and then its native build. GraphHopper 11.0 (`69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4`, Apache-2.0) failed compilation and then the adapter's endpoint-mapping audit. No accepted performance or re-priced accuracy population exists for either engine. [Attempt records](results/industry.json) retain full pins, licences, run links and known error scope. These integration failures do not establish engine limitations. No Docker image or substituted OSM speed model was used.

## Stale traffic

The retained [eight-city traffic study](results/traffic.md) re-prices the route chosen on stale weights on the current metric. Mean extra travel time is 3.6–7.5% with the dataset's static road-class speed table, 0.2–0.8% between informative consecutive slots and 0.5–2.0% between observed slots. These are historical measurements, separate from the refreshed engine ranking. Nationwide retry [45690498142](https://github.com/symonyeal/Exact-Routing/actions/runs/45690498142) finished; its artifact still needs integration.

`default_speed_kmh` is a road-class table, not measured free flow. Four weekday slots are observed; four are derived, including afternoon equal to midday. Slot order follows the dataset and includes the overnight wrap. These request pairs do not represent observed travel demand.

## Reproduce

Heavy measurements run on GitHub Actions through `scale.yml`. Python dependencies: NumPy, pandas and PyArrow. Rust dependencies are locked in `Cargo.lock`. Local checks:

```sh
cargo fmt --check
cargo clippy --release --locked --all-targets -- -D warnings
cargo test --release --locked
python -m unittest discover -s scripts -p 'test_*.py' -v
python scripts/page.py
node --check site/app.js
```

Fetch and run one size in CI, then summarize its complete raw shards:

```sh
python scripts/scale.py fetch sejong
cargo build --release --locked --bin scale
python scripts/scale.py run sejong time results/scale/sejong-time.jsonl
python scripts/scale.py run sejong partial results/scale/sejong-partial.jsonl
python scripts/scale.py run sejong memory results/scale/sejong-memory.jsonl
# RoutingKit requires the pinned native build in scale.yml and both driver modes.
python scripts/scale.py report results/scale
python scripts/page.py
```

The workflow's `previous_run` input reuses completed shard artifacts. A changed timing contract requires fresh timing measurements; summaries reject mixed contracts. Raw rows stream to `.partial`; summaries and generated `site/data.js` use LF. The page reads that generated file.

Code: MIT. [Korea Road Network ETA](https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta), revision `b111710756ee5727a0fbe3f6203c0cc2f93de2be`: **CC BY-NC 4.0, non-commercial use**. Topology derives from OpenStreetMap (ODbL); observed speeds are from KTDB 2024. Data and build outputs are fetched or built rather than committed.
