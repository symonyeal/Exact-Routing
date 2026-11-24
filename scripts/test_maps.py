import hashlib
import http.client
import json
import shutil
import struct
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

from metro import CITIES, KOREA_REVISION
from serve_maps import ApiError, MapsBackend
from test_metro import fixture


ROOT = Path(__file__).resolve().parents[1]
TEST_ROOT = ROOT / "target" / "maps-test"


def remove_fixture(path):
    base, target = TEST_ROOT.resolve(), path.resolve()
    if not base.is_relative_to(ROOT) or target == base or not target.is_relative_to(base):
        raise ValueError("refusing to remove a path outside the map test fixture")
    shutil.rmtree(target)


class MapsHttpTests(unittest.TestCase):
    def setUp(self):
        self.root = TEST_ROOT / self._testMethodName
        if self.root.exists():
            remove_fixture(self.root)
        (self.root / "maps" / "assets").mkdir(parents=True)
        for name, text in (("index.html", "<h1>map test</h1>"), ("app.js", "console.log('ok')"),
                           ("insights.js", "window.ROUTING_INSIGHTS = {};"),
                           ("renderer.js", "window.PartitionMaps = {};"),
                           ("style.css", "body{color:#000}"), ("assets/tokens.css", ":root{--x:1}")):
            (self.root / "maps" / name).write_text(text, encoding="utf-8")
        (self.root / "scripts").mkdir()
        (self.root / "scripts" / "metro.py").write_text("secret", encoding="utf-8")
        self.results_path = self.root / "benchmark.jsonl"
        rows = fixture("sejong")
        self.results_path.write_text("".join(json.dumps(row, allow_nan=False) + "\n" for row in rows), encoding="utf-8")
        self.backend = MapsBackend(self.root, self.results_path, no_fetch=True)
        self.server = MapsBackendServer(self.backend)
        self.connection = http.client.HTTPConnection("127.0.0.1", self.server.port, timeout=5)

    def tearDown(self):
        self.connection.close()
        self.server.close()
        remove_fixture(self.root)
        try:
            TEST_ROOT.rmdir()
        except OSError:
            pass

    def get(self, path):
        self.connection.request("GET", path)
        response = self.connection.getresponse()
        body = response.read()
        content_type = response.getheader("Content-Type", "")
        return response.status, body, content_type

    def install_graph(self, city="sejong"):
        binary = struct.pack("<QQ", 3, 3) + b"fixture graph bytes"
        roads = self.root / "data" / "roads"
        roads.mkdir(parents=True, exist_ok=True)
        (roads / f"{city}.bin").write_bytes(binary)
        (roads / f"{city}.meta.json").write_text(json.dumps({
            "dataset": "CAMUS-LAB/data-road_network_eta", "revision": KOREA_REVISION, "region": city,
            "weight_units": "seconds", "output_sha256": hashlib.sha256(binary).hexdigest(),
            "node_count": 3, "arc_count": 3,
        }), encoding="utf-8")
        return binary

    def test_whitelisted_static_and_path_traversal(self):
        status, body, kind = self.get("/")
        self.assertEqual((status, body, kind.split(";")[0]), (200, b"<h1>map test</h1>", "text/html"))
        self.assertEqual(self.get("/assets/tokens.css")[0], 200)
        self.assertEqual(self.get("/insights.js")[0], 200)
        self.assertEqual(self.get("/renderer.js")[0], 200)
        self.assertEqual(self.get("/scripts/metro.py")[0], 404)
        self.assertEqual(self.get("/%2e%2e/scripts/metro.py")[0], 404)

    def test_city_inventory_and_benchmark_population(self):
        status, body, _ = self.get("/api/cities")
        payload = json.loads(body)
        self.assertEqual(status, 200)
        self.assertEqual({row["id"] for row in payload["cities"]}, CITIES)
        self.assertEqual(payload["benchmark"], {"completed_cities": 1, "total_cities": 11})
        status, body, _ = self.get("/api/benchmark?city=sejong&scale=0")
        benchmark = json.loads(body)
        self.assertEqual(status, 200)
        self.assertIsNotNone(benchmark["graph"])
        self.assertEqual(len(benchmark["rows"]), 8)
        status, body, _ = self.get("/api/benchmark?city=daegu&scale=1000")
        self.assertEqual(status, 200)
        self.assertIsNone(json.loads(body)["graph"])
        self.assertEqual(json.loads(self.get("/api/benchmark?city=bad&scale=0")[1])["error"], "Unknown city.")
        self.assertEqual(self.get("/api/benchmark?city=sejong&scale=7")[0], 400)

    def test_missing_data_no_fetch_and_invalid_route_params(self):
        self.assertEqual(self.get("/api/network?city=daegu")[0], 404)
        self.assertEqual(self.get("/api/network?city=../../scripts/metro.py")[0], 400)
        self.assertEqual(self.get("/api/route?city=sejong&source=abc&target=1&scale=0")[0], 400)
        self.assertEqual(self.get("/api/route?city=sejong&source=1&target=2&scale=20")[0], 400)

    def test_network_hash_and_routed_json(self):
        binary = self.install_graph()
        self.backend.route_runner = lambda file, source, target, scale: {
            "source": source, "target": target, "scale": scale, "distance": 4.5,
            "path": [source, target], "certified": True,
            "units": "seconds" if scale == 0 else "milliseconds", "nodes": 3, "arcs": 3,
        }
        status, body, kind = self.get("/api/network?city=sejong")
        self.assertEqual((status, body, kind), (200, binary, "application/octet-stream"))
        status, body, _ = self.get("/api/route?city=sejong&source=0&target=2&scale=1000")
        route = json.loads(body)
        self.assertEqual(status, 200)
        self.assertEqual(route["units"], "milliseconds")
        self.assertTrue(route["certified"])

    def test_bad_hash_and_wrong_route_contract_are_rejected(self):
        self.install_graph()
        sidecar = self.root / "data" / "roads" / "sejong.meta.json"
        sidecar.write_text(json.dumps({"output_sha256": "bad", "node_count": 3, "arc_count": 3}), encoding="utf-8")
        self.assertEqual(self.get("/api/network?city=sejong")[0], 503)
        self.assertEqual(self.get("/api/benchmark?city=sejong&scale=0")[0], 503)
        binary = self.install_graph()
        self.backend.route_runner = lambda *_: {"source": 0, "target": 2, "scale": 0, "distance": 1,
                                                "path": [0, 2], "certified": False, "units": "seconds",
                                                "nodes": 3, "arcs": 3}
        self.assertEqual(self.get("/api/route?city=sejong&source=0&target=2&scale=0")[0], 502)

    def test_fetch_mismatch_restores_existing_binary_without_sidecar(self):
        binary_dir = self.root / "data" / "roads"
        binary_dir.mkdir(parents=True, exist_ok=True)
        path = binary_dir / "sejong.bin"
        original = b"retained original graph"
        path.write_bytes(original)
        backend = MapsBackend(self.root, no_fetch=False)

        def fake_run(args, **kwargs):
            generated = b"different pinned graph"
            path.write_bytes(generated)
            path.with_suffix(".meta.json").write_text(json.dumps({"output_sha256": hashlib.sha256(generated).hexdigest()}), encoding="utf-8")

        with patch("serve_maps.subprocess.run", side_effect=fake_run):
            with self.assertRaises(ApiError) as error:
                backend._ensure_data("sejong")
        self.assertEqual(error.exception.status, 409)
        self.assertEqual(path.read_bytes(), original)
        self.assertFalse(path.with_suffix(".meta.json").exists())

    def test_matching_source_hash_allows_saved_benchmark(self):
        binary = self.install_graph()
        graph = next(r for r in self.backend.results["sejong"] if r["kind"] == "graph")
        digest = hashlib.sha256(binary).hexdigest()
        graph["binary_sha256"] = digest
        graph["source"]["output_sha256"] = digest
        status, body, _ = self.get("/api/benchmark?city=sejong&scale=0")
        self.assertEqual(status, 200)
        self.assertIsNotNone(json.loads(body)["graph"])

    def install_partition_fixture(self, bad_structure=False):
        binary = self.install_graph()
        digest = hashlib.sha256(binary).hexdigest()
        rows = fixture("sejong")
        q = 1000
        pairs = [[i % 3, (i + 1) % 3] for i in range(q)]
        references = [float(i % 3) for i in range(q)]
        for row in rows:
            if row["kind"] == "graph":
                row.update(nodes=3, arcs=3, queries=q, certified_queries=q, pairs=pairs,
                           reference=[distance * (1000 if row["scale"] else 1) for distance in references],
                           binary_sha256=digest)
                row["source"].update(output_sha256=digest, node_count=3, arc_count=3)
            elif row["kind"] == "build" and row["method"] != "fast_paths-CH":
                row.update(parity_queries=q, cells=2, boundary=2, arcs=3, array_bytes=128)
            elif row["kind"] == "latency":
                row["queries"] = q
            elif row["kind"] == "throughput":
                row["queries"] = q
        self.results_path.write_text("".join(json.dumps(row, allow_nan=False) + "\n" for row in rows), encoding="utf-8")
        calls = []

        def export(_binary, _results, city, bound, scale):
            calls.append((city, bound, scale))
            selected = [row for row in self.backend.results[city] if row["scale"] == scale]
            graph = next(row for row in selected if row["kind"] == "graph")
            builds = {row["method"]: row for row in selected if row["kind"] == "build"}
            models = {}
            for model in ("Graph", "Hypergraph"):
                build = builds[f"{model}-{bound}"]
                models[model] = {"cells": build["cells"], "boundary": build["boundary"],
                                 "overlay_arcs": build["arcs"], "array_bytes": build["array_bytes"],
                                 "arc_cells": [0, 1, None], "boundary_nodes": [0, 1]}
            queries = []
            for index in (0, 1, 2, 3, 125, 126, 501, 502, 750, 999):
                source, target = graph["pairs"][index]
                graph_boundary = sum(node in (0, 1) for node in (source, target))
                hyper_boundary = int(source in (0, 1))
                queries.append({"index": index, "source": source, "target": target,
                                "category": "walk8" if index % 4 == 0 else "walk32" if index % 4 == 1 else "uniform",
                                "distance": graph["reference"][index], "certified": True,
                                "dijkstra": {"settled": 2, "scanned": 3},
                                "Graph": {"settled": 2, "scanned": 3, "boundary_settled": graph_boundary,
                                          "interior_settled": 2 - graph_boundary, "settled_nodes": [source, target]},
                                "Hypergraph": {"settled": 1, "scanned": 2, "boundary_settled": hyper_boundary,
                                               "interior_settled": 1 - hyper_boundary, "settled_nodes": [source]}})
            if bad_structure:
                models["Graph"]["overlay_arcs"] += 1
            return {"schema": 1, "city": city, "bound": bound, "scale": scale, "nodes": 3, "arcs": 3,
                    "tail": [0, 1, 2], "head": [1, 2, 0], "models": models, "queries": queries}

        self.backend.partition_runner = export
        self.backend.results = self.backend._read_results(self.results_path)
        self.backend.results_hash = self.backend._results_hash()
        return calls

    def test_partition_export_checks_counts_and_caches_by_input_key(self):
        calls = self.install_partition_fixture()
        status, body, _ = self.get("/api/partition?city=sejong&bound=256&scale=0")
        payload = json.loads(body)
        self.assertEqual(status, 200)
        self.assertEqual(payload["models"]["Graph"]["arc_cells"], [0, 1, None])
        self.assertEqual(len(payload["queries"]), 10)
        self.assertEqual(len(calls), 1)
        self.assertEqual(self.get("/api/partition?city=sejong&bound=256&scale=0")[0], 200)
        self.assertEqual(len(calls), 1)
        with self.results_path.open("a", encoding="utf-8") as result_file:
            result_file.write("\n")
        self.assertEqual(self.get("/api/partition?city=sejong&bound=256&scale=0")[0], 200)
        self.assertEqual(len(calls), 2)
        self.assertEqual(self.get("/api/partition?city=sejong&bound=200&scale=0")[0], 400)

    def test_partition_rejects_structure_mismatch_without_caching(self):
        self.install_partition_fixture(bad_structure=True)
        self.assertEqual(self.get("/api/partition?city=sejong&bound=256&scale=0")[0], 502)
        cache = self.root / "data" / "insights" / "sejong-256-0.json"
        self.assertFalse(cache.exists())


class MapsBackendServer:
    def __init__(self, backend):
        from http.server import ThreadingHTTPServer
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), backend.handler())
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.server.shutdown()
        self.thread.join(timeout=5)
        self.server.server_close()


if __name__ == "__main__":
    unittest.main()
