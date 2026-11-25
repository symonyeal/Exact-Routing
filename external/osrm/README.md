OSRM adapter for the benchmark's directed graph and integer-millisecond metrics.

Pin: [v5.27.1](https://github.com/Project-OSRM/osrm-backend/releases/tag/v5.27.1),
commit `4f3ee609ec1af40eb1f445c6706cfa5beb04c990`,
[BSD-2-Clause](https://github.com/Project-OSRM/osrm-backend/blob/4f3ee609ec1af40eb1f445c6706cfa5beb04c990/LICENSE.TXT).
No container image is used.

Build on the same Ubuntu 22.04 runner as the paired Rust benchmark. Dependencies are CMake,
Boost date-time/chrono/filesystem/iostreams/program-options/regex/system/thread/test,
TBB, Expat, BZip2, Lua 5.4 and zlib development packages.

```sh
git clone --branch v5.27.1 --depth 1 https://github.com/Project-OSRM/osrm-backend external/osrm/source
test "$(git -C external/osrm/source rev-parse HEAD)" = 4f3ee609ec1af40eb1f445c6706cfa5beb04c990
cmake -S external/osrm -B external/osrm/build -DOSRM_SOURCE="$PWD/external/osrm/source" -DCMAKE_BUILD_TYPE=Release
cmake --build external/osrm/build --parallel 2 --target osrm-bench osrm-extract osrm-contract osrm-partition osrm-customize
python scripts/industry_osrm.py run data/scale/sejong.rk.bin data/scale/osrm results/industry/sejong-osrm.jsonl --build external/osrm/build
```

`scripts/industry.py` writes one two-node, one-way OSM way per directed arc, node
IDs offset by one, original coordinates, `arc_ms` and `arc_ms_0` through `arc_ms_7`.
The Lua profile sets duration and weight directly from `arc_ms`, with no turn
penalties or turn restrictions. `osrm-bench segments` uses OSRM's own coordinate
quantization and `greatCircleDistance` routine to write eight segment-speed feeds.
There are three CSV fields (source OSM ID, target OSM ID, km/h); OSRM's fourth
field is an optional weight **rate**, not a duration. Each slot updates with
`osrm-contract --segment-speed-file` or `osrm-customize --segment-speed-file`.
Both use one thread. Preparation includes extraction and, for MLD, partitioning;
update includes the native executable's I/O and process startup. Native query
loading is reported separately as `load_s`.

The driver calls `OSRM::Route` directly, requests node annotations and unpacks
every returned path. Query time includes that native API's response construction
and node extraction. The timed workload is `(query_index + pass) % 8 == slot`,
matching `scale`'s population; all remaining pairs are checked without contributing
to query time. Paths are re-priced on the original integer-millisecond metric.
Rows retain the per-request reference and re-priced costs, with `null` for
unreachable requests, to recompute the optimal share and excess distribution.

OSRM stores deciseconds, rounded to the nearest 100 ms with a 100 ms minimum.
The read-back audit checks every directed segment's stored duration and weight
against that rule after each update. These answers are re-priced, never certified.
OSRM's public API chooses snapped edge endpoints; the driver requires the
requested source and destination IDs to be the unpacked path's endpoints and requires
reachability to match the original graph. A mismatch, lost arc, parallel directed
arc ambiguity or zero-length geometry fails the attempt, rather than substituting
another graph or speed model.

Sources for the API and units:
[native example](https://github.com/Project-OSRM/osrm-backend/blob/4f3ee609ec1af40eb1f445c6706cfa5beb04c990/example/example.cpp),
[updater](https://github.com/Project-OSRM/osrm-backend/blob/4f3ee609ec1af40eb1f445c6706cfa5beb04c990/src/updater/updater.cpp),
[profile contract](https://github.com/Project-OSRM/osrm-backend/blob/4f3ee609ec1af40eb1f445c6706cfa5beb04c990/docs/profiles.md).
