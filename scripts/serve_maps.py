"""Serve the local metro map UI, pinned road graphs, and certified routes."""

import argparse
import hashlib
import json
import math
import mimetypes
import os
import re
import subprocess
import sys
import threading
import urllib.parse
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from metro import BOUNDS, CITIES, KOREA_REVISION, WORLD_REVISION, validate

KOREA = ("sejong", "seoul", "busan", "daegu", "incheon", "gwangju", "daejeon", "ulsan")
WORLD = {"london-large": "london", "new_york-large": "new_york", "tokyo-large": "tokyo"}
LABELS = {
    "sejong": ("Sejong", "South Korea"), "seoul": ("Seoul", "South Korea"),
    "busan": ("Busan", "South Korea"), "daegu": ("Daegu", "South Korea"),
    "incheon": ("Incheon", "South Korea"), "gwangju": ("Gwangju", "South Korea"),
    "daejeon": ("Daejeon", "South Korea"), "ulsan": ("Ulsan", "South Korea"),
    "london-large": ("London, large snapshot", "United Kingdom"),
    "new_york-large": ("New York, large snapshot", "United States"),
    "tokyo-large": ("Tokyo, large snapshot", "Japan"),
}
STATIC = {
    "/": "maps/index.html",
    "/maps/index.html": "maps/index.html",
    "/index.html": "maps/index.html",
    "/maps/app.js": "maps/app.js",
    "/app.js": "maps/app.js",
    "/maps/renderer.js": "maps/renderer.js",
    "/renderer.js": "maps/renderer.js",
    "/maps/style.css": "maps/style.css",
    "/style.css": "maps/style.css",
    "/maps/assets/tokens.css": "maps/assets/tokens.css",
    "/assets/tokens.css": "maps/assets/tokens.css",
    "/favicon.ico": "maps/favicon.ico",
    "/insights.js": "maps/insights.js",
    "/maps/insights.js": "maps/insights.js",
}


class ApiError(Exception):
    def __init__(self, status, message):
        self.status, self.message = status, message


class MapsBackend:
    def __init__(self, root, results=None, no_fetch=False, route_binary=None, route_runner=None,
                 partition_binary=None, partition_runner=None):
        self.root = Path(root).resolve()
        self.roads = self.root / "data" / "roads"
        self.no_fetch = no_fetch
        self.route_binary = route_binary or self.root / "target" / "release" / ("route_path.exe" if os.name == "nt" else "route_path")
        self.partition_binary = partition_binary or self.root / "target" / "release" / ("partition_view.exe" if os.name == "nt" else "partition_view")
        self.route_runner = route_runner
        self.partition_runner = partition_runner
        self.results_source = Path(results).resolve() if results is not None else None
        self.results = self._read_results(results)
        self.results_hash = self._results_hash()
        self.locks = {city: threading.Lock() for city in CITIES}
        self.verified = {}
        self.partition_locks = {(city, bound, scale): threading.Lock()
                                for city in CITIES for bound in BOUNDS for scale in (0, 1000)}
        self.partition_gate = threading.Semaphore(1)
        self.results_lock = threading.Lock()

    def _results_hash(self):
        if self.results_source is None or not self.results_source.is_file():
            return None
        return hashlib.sha256(self.results_source.read_bytes()).hexdigest()

    def _read_results(self, source):
        grouped = {}
        if source is None or not Path(source).exists():
            return grouped
        source = Path(source)
        if source.is_dir():
            files = sorted(source.glob("*.jsonl"))
            for path in files:
                city = path.stem
                if city not in CITIES or city in grouped:
                    raise ValueError("benchmark directory has an unknown or duplicate city file")
                grouped[city] = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
        elif source.is_file():
            for number, line in enumerate(source.read_text(encoding="utf-8").splitlines(), 1):
                if not line.strip():
                    continue
                row = json.loads(line)
                city = row.get("city")
                if city not in CITIES:
                    raise ValueError(f"benchmark file has an unknown city on line {number}")
                grouped.setdefault(city, []).append(row)
        else:
            return grouped
        for city, records in grouped.items():
            validate(records, city)
        return grouped

    def _data_paths(self, city):
        return self.roads / f"{city}.bin", self.roads / f"{city}.meta.json"

    def _exists(self, city):
        binary, sidecar = self._data_paths(city)
        return binary.is_file() and sidecar.is_file()

    def _verified_data(self, city):
        binary, sidecar = self._data_paths(city)
        try:
            stat = binary.stat()
            side_stat = sidecar.stat()
            stamp = (stat.st_size, stat.st_mtime_ns, side_stat.st_size, side_stat.st_mtime_ns)
            metadata = json.loads(sidecar.read_text(encoding="utf-8"))
            if self.verified.get(city) == stamp:
                return binary, metadata
            digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        except (OSError, ValueError, json.JSONDecodeError) as exc:
            raise ApiError(503, "Road graph data is unavailable or invalid.") from exc
        if digest != metadata.get("output_sha256"):
            self.verified.pop(city, None)
            raise ApiError(503, "Road graph data failed its integrity check.")
        if city in WORLD:
            expected = ("https://huggingface.co/datasets/HBNU-NLP/InstructOSM", WORLD_REVISION, WORLD[city], "large")
            actual = (metadata.get("dataset"), metadata.get("revision"), metadata.get("city"), metadata.get("scale"))
            units = metadata.get("native_units")
        else:
            expected = ("CAMUS-LAB/data-road_network_eta", KOREA_REVISION, city)
            actual = (metadata.get("dataset"), metadata.get("revision"), metadata.get("region"))
            units = metadata.get("weight_units")
        if (actual != expected or units != "seconds" or not isinstance(metadata.get("node_count"), int)
                or metadata["node_count"] < 1 or not isinstance(metadata.get("arc_count"), int)
                or metadata["arc_count"] < 0):
            self.verified.pop(city, None)
            raise ApiError(503, "Road graph metadata does not match its pinned source.")
        self.verified[city] = stamp
        return binary, metadata

    def _ensure_data(self, city):
        with self.locks[city]:
            if not self._exists(city):
                if self.no_fetch:
                    raise ApiError(404, "Road graph data is not available on this server.")
                binary, sidecar = self._data_paths(city)
                original = binary.read_bytes() if binary.is_file() and not sidecar.exists() else None
                script = self.root / "scripts" / ("international.py" if city in WORLD else "roads.py")
                args = [sys.executable, str(script)]
                args.extend((WORLD[city], "large") if city in WORLD else (city,))
                try:
                    subprocess.run(args, cwd=self.root, check=True, capture_output=True, text=True, timeout=600)
                except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
                    if original is not None:
                        binary.write_bytes(original)
                        sidecar.unlink(missing_ok=True)
                    raise ApiError(502, "Road graph data could not be prepared.") from exc
                if original is not None:
                    try:
                        generated = json.loads(sidecar.read_text(encoding="utf-8"))
                    except (OSError, json.JSONDecodeError) as exc:
                        binary.write_bytes(original)
                        sidecar.unlink(missing_ok=True)
                        raise ApiError(502, "Pinned data could not be verified; existing graph was preserved.") from exc
                    if hashlib.sha256(original).hexdigest() != generated.get("output_sha256"):
                        binary.write_bytes(original)
                        sidecar.unlink(missing_ok=True)
                        raise ApiError(409, "Pinned data differs from the existing graph; existing graph was preserved.")
            return self._verified_data(city)

    def cities(self):
        values = []
        for city in sorted(CITIES):
            label, country = LABELS[city]
            source = f"https://huggingface.co/datasets/CAMUS-LAB/data-road_network_eta/tree/{KOREA_REVISION}" if city in KOREA else f"https://huggingface.co/datasets/HBNU-NLP/InstructOSM/tree/{WORLD_REVISION}"
            values.append({"id": city, "label": label, "country": country, "available": self._exists(city), "source_url": source})
        return {"cities": values, "benchmark": {"completed_cities": len(self.results), "total_cities": 11}}

    def benchmark(self, city, scale):
        records = self.results.get(city, [])
        rows = [r for r in records if r.get("scale") == scale]
        graph = next((r for r in rows if r.get("kind") == "graph"), None)
        if graph is not None and self._exists(city):
            _, metadata = self._verified_data(city)
            if graph.get("binary_sha256") != metadata.get("output_sha256"):
                raise ApiError(409, "Saved benchmark belongs to a different road graph.")
        return {
            "graph": graph,
            "rows": [r for r in rows if r.get("kind") == "latency"],
            "builds": [r for r in rows if r.get("kind") == "build"],
            "machine": graph.get("machine") if graph else None,
        }

    def _current_results(self):
        if self.results_source is None or not self.results_source.is_file():
            raise ApiError(503, "Partition views require the consolidated frozen results file.")
        raw = self.results_source.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        with self.results_lock:
            if digest != self.results_hash:
                self.results = self._read_results(self.results_source)
                self.results_hash = digest
            return self.results, digest

    def partition(self, city, bound, scale):
        key = (city, bound, scale)
        with self.partition_locks[key]:
            results, results_hash = self._current_results()
            if city not in results:
                raise ApiError(404, "No frozen benchmark is available for this city.")
            binary, metadata = self._ensure_data(city)
            graph = next(row for row in results[city] if row["kind"] == "graph" and row["scale"] == scale)
            if graph.get("binary_sha256") != metadata.get("output_sha256"):
                raise ApiError(409, "Frozen benchmark belongs to a different road graph.")
            if self.partition_runner is None and not self.partition_binary.is_file():
                raise ApiError(503, "Partition exporter is not available on this server.")
            executable_hash = (hashlib.sha256(self.partition_binary.read_bytes()).hexdigest()
                               if self.partition_runner is None else "test-exporter")
            cache_key = {
                "binary_sha256": metadata["output_sha256"],
                "executable_sha256": executable_hash,
                "results_sha256": results_hash,
                "city": city,
                "bound": bound,
                "scale": scale,
            }
            cache = self.root / "data" / "insights" / f"{city}-{bound}-{scale}.json"
            sidecar = cache.with_suffix(".key.json")
            try:
                saved_key = json.loads(sidecar.read_text(encoding="utf-8"))
                saved = json.loads(cache.read_text(encoding="utf-8"))
                if saved_key == cache_key:
                    self._validate_partition(saved, city, bound, scale, graph, results[city])
                    return saved
            except (OSError, ValueError, json.JSONDecodeError, ApiError):
                pass

            with self.partition_gate:
                result = self._export_partition(binary, city, bound, scale)
            self._validate_partition(result, city, bound, scale, graph, results[city])
            cache.parent.mkdir(parents=True, exist_ok=True)
            cache_partial, key_partial = cache.with_suffix(".partial"), sidecar.with_suffix(".partial")
            cache_partial.write_text(json.dumps(result, separators=(",", ":"), allow_nan=False) + "\n", encoding="utf-8")
            key_partial.write_text(json.dumps(cache_key, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
            cache_partial.replace(cache)
            key_partial.replace(sidecar)
            return result

    def _export_partition(self, binary, city, bound, scale):
        if self.partition_runner:
            try:
                return self.partition_runner(binary, self.results_source, city, bound, scale)
            except Exception as exc:
                raise ApiError(502, "Partition view generation failed.") from exc
        try:
            completed = subprocess.run(
                [str(self.partition_binary), str(binary), str(self.results_source), city, str(bound), str(scale)],
                cwd=self.root, check=True, capture_output=True, text=True, timeout=240,
            )
            return json.loads(completed.stdout)
        except subprocess.TimeoutExpired as exc:
            raise ApiError(504, "Partition view generation timed out.") from exc
        except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as exc:
            raise ApiError(502, "Partition view generation failed.") from exc

    @staticmethod
    def _validate_partition(data, city, bound, scale, graph, records):
        n, m = graph["nodes"], graph["arcs"]
        if (not isinstance(data, dict) or data.get("schema") != 1 or data.get("city") != city
                or data.get("bound") != bound or data.get("scale") != scale
                or data.get("nodes") != n or data.get("arcs") != m):
            raise ApiError(502, "Partition view metadata did not match the request.")
        tail, head = data.get("tail"), data.get("head")
        if (not isinstance(tail, list) or not isinstance(head, list) or len(tail) != m or len(head) != m
                or any(not isinstance(v, int) or isinstance(v, bool) or not 0 <= v < n for v in tail + head)):
            raise ApiError(502, "Partition view graph arrays were invalid.")
        builds = {row["method"]: row for row in records if row["kind"] == "build" and row["scale"] == scale}
        models = data.get("models")
        if not isinstance(models, dict) or set(models) != {"Graph", "Hypergraph"}:
            raise ApiError(502, "Partition view models were invalid.")
        for model in ("Graph", "Hypergraph"):
            method = f"{model}-{bound}"
            saved = builds[method]
            item = models[model]
            if not isinstance(item, dict):
                raise ApiError(502, "Partition view model record was invalid.")
            expected = {"cells": saved["cells"], "boundary": saved["boundary"],
                        "overlay_arcs": saved["arcs"], "array_bytes": saved["array_bytes"]}
            if any(item.get(key) != value for key, value in expected.items()):
                raise ApiError(502, "Partition view structure differs from frozen benchmark counts.")
            arc_cells, boundary_nodes = item.get("arc_cells"), item.get("boundary_nodes")
            if (not isinstance(arc_cells, list) or len(arc_cells) != m
                    or any(cell is not None and (not isinstance(cell, int) or isinstance(cell, bool) or not 0 <= cell < item["cells"])
                           for cell in arc_cells)
                    or not isinstance(boundary_nodes, list)
                    or len(boundary_nodes) != item["boundary"]
                    or any(not isinstance(v, int) or isinstance(v, bool) or not 0 <= v < n for v in boundary_nodes)
                    or len(set(boundary_nodes)) != len(boundary_nodes)):
                raise ApiError(502, "Partition view node and arc labels were invalid.")
        graph_row = next(row for row in records if row["kind"] == "graph" and row["scale"] == scale)
        pairs, references = graph_row["pairs"], graph_row["reference"]
        presets = (0, 1, 2, 3, 125, 126, 501, 502, 750, 999)
        queries = data.get("queries")
        if not isinstance(queries, list) or len(queries) != len(presets):
            raise ApiError(502, "Partition view query population was invalid.")
        for query, index in zip(queries, presets):
            if not isinstance(query, dict):
                raise ApiError(502, "Partition view query row was invalid.")
            source, target = pairs[index]
            category = "walk8" if index % 4 == 0 else "walk32" if index % 4 == 1 else "uniform"
            distance = references[index]
            if (query.get("index") != index or not isinstance(query.get("source"), int) or isinstance(query.get("source"), bool)
                    or not isinstance(query.get("target"), int) or isinstance(query.get("target"), bool)
                    or query.get("source") != source or query.get("target") != target
                    or query.get("category") != category or query.get("certified") is not True
                    or (distance is None) != (query.get("distance") is None)):
                raise ApiError(502, "Partition view query identities differ from frozen results.")
            if distance is not None and (not isinstance(query["distance"], (int, float)) or isinstance(query["distance"], bool)
                    or (query["distance"] != distance if scale == 1000 else
                        not math.isclose(query["distance"], distance, rel_tol=1e-9, abs_tol=1e-9))):
                raise ApiError(502, "Partition view query distance differs from frozen results.")
            dijkstra = query.get("dijkstra")
            if not isinstance(dijkstra, dict) or any(not isinstance(dijkstra.get(key), int) or isinstance(dijkstra[key], bool) or dijkstra[key] < 0
                                                     for key in ("settled", "scanned")):
                raise ApiError(502, "Partition view Dijkstra trace was invalid.")
            for model in ("Graph", "Hypergraph"):
                metrics = query.get(model)
                if not isinstance(metrics, dict) or any(not isinstance(metrics.get(key), int) or isinstance(metrics[key], bool) or metrics[key] < 0
                                                        for key in ("settled", "scanned", "boundary_settled", "interior_settled")):
                    raise ApiError(502, "Partition view query counts were invalid.")
                nodes = metrics.get("settled_nodes")
                boundary = models[model]["boundary_nodes"]
                if (not isinstance(nodes, list) or len(nodes) != metrics["settled"]
                        or any(not isinstance(v, int) or isinstance(v, bool) or not 0 <= v < n for v in nodes)
                        or len(set(nodes)) != len(nodes)
                        or metrics["boundary_settled"] != len(set(nodes).intersection(boundary))
                        or metrics["interior_settled"] + metrics["boundary_settled"] != metrics["settled"]):
                    raise ApiError(502, "Partition view settled-node list was invalid.")
        return data

    def route(self, city, source, target, scale):
        binary, metadata = self._ensure_data(city)
        n = metadata.get("node_count")
        if not isinstance(n, int) or not 0 <= source < n or not 0 <= target < n:
            raise ApiError(400, "Source or destination is outside this graph.")
        if self.route_runner:
            result = self.route_runner(binary, source, target, scale)
        else:
            try:
                completed = subprocess.run([str(self.route_binary), str(binary), str(source), str(target), str(scale)],
                                           cwd=self.root, check=True, capture_output=True, text=True, timeout=60)
                result = json.loads(completed.stdout)
            except subprocess.TimeoutExpired as exc:
                raise ApiError(504, "Route search timed out.") from exc
            except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as exc:
                raise ApiError(502, "Route search failed.") from exc
        self._validate_route(result, source, target, scale, n, metadata.get("arc_count"))
        return result

    @staticmethod
    def _validate_route(result, source, target, scale, n, source_arcs):
        units = "seconds" if scale == 0 else "milliseconds"
        if (not isinstance(result, dict) or result.get("source") != source or result.get("target") != target
                or result.get("scale") != scale or result.get("certified") is not True
                or result.get("units") != units or result.get("nodes") != n
                or not isinstance(result.get("arcs"), int) or not 0 <= result["arcs"] <= source_arcs):
            raise ApiError(502, "Route result did not match this graph request.")
        distance, path = result.get("distance"), result.get("path")
        if distance is not None and (not isinstance(distance, (int, float)) or not math.isfinite(distance) or distance < 0):
            raise ApiError(502, "Route result contained an invalid distance.")
        if not isinstance(path, list) or any(not isinstance(v, int) or not 0 <= v < n for v in path):
            raise ApiError(502, "Route result contained an invalid path.")
        if distance is None:
            if path:
                raise ApiError(502, "Unreachable route unexpectedly contained a path.")
        elif not path or path[0] != source or path[-1] != target:
            raise ApiError(502, "Route path endpoints were invalid.")

    def handler(self):
        backend = self

        class Handler(BaseHTTPRequestHandler):
            def _send(self, status, body, content_type="application/json; charset=utf-8"):
                payload = body if isinstance(body, bytes) else json.dumps(body, separators=(",", ":")).encode("utf-8")
                self.send_response(status)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(payload)))
                self.send_header("X-Content-Type-Options", "nosniff")
                self.end_headers()
                try:
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                    pass  # A city switch can cancel a graph download after response headers.

            def _query(self, required):
                params = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query, keep_blank_values=True)
                if set(params) != set(required) or any(len(params[key]) != 1 for key in required):
                    raise ApiError(400, "Request parameters are missing or duplicated.")
                return {key: params[key][0] for key in required}

            def do_GET(self):
                path = urllib.parse.urlsplit(self.path).path
                try:
                    if path in STATIC:
                        self._static(STATIC[path])
                    elif path == "/api/cities":
                        self._send(200, backend.cities())
                    elif path == "/api/benchmark":
                        params = self._query(("city", "scale"))
                        city, scale = params["city"], _scale(params["scale"])
                        _city(city)
                        self._send(200, backend.benchmark(city, scale))
                    elif path == "/api/partition":
                        params = self._query(("city", "bound", "scale"))
                        city, bound, scale = _city(params["city"]), _bound(params["bound"]), _scale(params["scale"])
                        self._send(200, backend.partition(city, bound, scale))
                    elif path == "/api/network":
                        params = self._query(("city",))
                        city = _city(params["city"])
                        binary, _ = backend._ensure_data(city)
                        self._send(200, binary.read_bytes(), "application/octet-stream")
                    elif path == "/api/route":
                        params = self._query(("city", "source", "target", "scale"))
                        city = _city(params["city"])
                        source, target = _integer(params["source"]), _integer(params["target"])
                        scale = _scale(params["scale"])
                        self._send(200, backend.route(city, source, target, scale))
                    else:
                        self._send(404, {"error": "Not found."})
                except ApiError as exc:
                    self._send(exc.status, {"error": exc.message})
                except Exception:
                    self._send(500, {"error": "The local map service encountered an error."})

            def _static(self, relative):
                path = backend.root / relative
                try:
                    payload = path.read_bytes()
                except OSError:
                    self._send(404, {"error": "Map interface file is not available."})
                    return
                kind = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
                if kind in ("text/html", "text/css", "application/javascript", "text/javascript"):
                    kind += "; charset=utf-8"
                self._send(200, payload, kind)

            def log_message(self, fmt, *args):
                print("maps: " + (fmt % args), file=sys.stderr)

        return Handler


def _city(value):
    if value not in CITIES:
        raise ApiError(400, "Unknown city.")
    return value


def _integer(value):
    if not re.fullmatch(r"[0-9]+", value):
        raise ApiError(400, "Node ids must be nonnegative integers.")
    try:
        return int(value)
    except ValueError as exc:
        raise ApiError(400, "Node id is invalid.") from exc


def _scale(value):
    if value not in ("0", "1000"):
        raise ApiError(400, "Scale must be 0 or 1000.")
    return int(value)


def _bound(value):
    if value not in tuple(str(bound) for bound in BOUNDS):
        raise ApiError(400, "Partition bound must be 256, 1024, or 4096.")
    return int(value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--no-fetch", action="store_true")
    parser.add_argument("--results", type=Path, default=Path("results/metro.jsonl"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if not 1 <= args.port <= 65535:
        parser.error("--port must be from 1 to 65535")
    executable = root / "target" / "release" / ("route_path.exe" if os.name == "nt" else "route_path")
    print("Building the route and partition tools once...", flush=True)
    subprocess.run(["cargo", "build", "--release", "--locked", "--bin", "route_path", "--bin", "partition_view"], cwd=root, check=True)
    results = args.results if args.results.is_absolute() else root / args.results
    backend = MapsBackend(root, results, args.no_fetch, executable)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), backend.handler())
    url = f"http://127.0.0.1:{server.server_port}/"
    print(f"Metro maps available at {url}", flush=True)
    if not args.no_browser:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
