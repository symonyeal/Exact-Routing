# GraphHopper adapter

The benchmark pins [GraphHopper 11.0](https://github.com/graphhopper/graphhopper/releases/tag/11.0),
commit `69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4`,
[Apache-2.0](https://github.com/graphhopper/graphhopper/blob/69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4/LICENSE.txt).
Maven resolves the published `graphhopper-core:11.0` artifact; no Docker image is used.

Build with Java 17 or newer and `mvn -B -f external/graphhopper/pom.xml package`.
Run `python scripts/industry_graphhopper.py INPUT GRAPH.osm.pbf OUTPUT --passes 0,1,2`.
The PBF must be exported by `scripts/industry.py`: node IDs are original IDs plus one;
way IDs are original directed arc IDs plus one, with tags `arc_ms_0` through `arc_ms_7`.

The parser replaces the OSM access and speed rules. Every way is forward-only; speed is
`edge_distance * 3600 / arc_ms_SLOT` km/h. GraphHopper's upstream car speed encoding rounds
to 2 km/h steps and permits 2 through 254 km/h. Out-of-range speeds are clamped and counted
in each row. The imported graph must contain exactly one edge per original arc, with consistent
original endpoints; a split, omission or duplicate aborts the run. No default OSM speed is used.

Each metric is a fresh native import and CH preparation, with import, subnetworks and CH threads
set to one. `update_s` includes that complete operation, including PBF reads and GraphHopper's
location index; `prepare_s` is zero because the importer is rebuilt for every metric. Queries call
`CHRoutingAlgorithmFactory` directly by graph-node ID and unpack every found base-edge path.
Timing includes per-request algorithm allocation and path extraction; re-pricing is outside it.
There is no coordinate snapping, HTTP request, turn penalty, or distance preference.

Rows retain all original references and re-priced path costs as `reference_ms` and `price_ms`,
with null for unreachable requests. Exact optimality shares use all reachable requests, including identical endpoints;
unreachable requests remain in query timing and are checked for matching reachability. These are
re-priced answers, not certified GraphHopper answers. `memory_bytes` is the entire JVM
process peak, including the retained original input and certificates, and is not an index-only size.
The timed query subset and order match `scale`'s pass/slot schedule; every remaining request is
checked afterwards without including it in query timing. Original isolated nodes have no native
OSM edge: their identity requests return a zero-length path and other requests return unreachable.
For conversion fixtures of at most 64 arcs, `imported_units` retains one row per arc:
`[arc, original_tail, original_head, original_ms, stored_distance_m, stored_speed_kmh, native_ms]`.
The stored speed and rounded travel time are read from the imported native edge.

The parser and native interfaces are based on the pinned source:
[OSMReader](https://github.com/graphhopper/graphhopper/blob/69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4/core/src/main/java/com/graphhopper/reader/osm/OSMReader.java),
[speed encoding](https://github.com/graphhopper/graphhopper/blob/69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4/core/src/main/java/com/graphhopper/routing/ev/DefaultImportRegistry.java),
[native CH queries](https://github.com/graphhopper/graphhopper/blob/69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4/core/src/main/java/com/graphhopper/routing/ch/CHRoutingAlgorithmFactory.java).
