"""Directed PBF conversion, workload preservation and rounded-router re-pricing."""

import tempfile
import unittest
from pathlib import Path

import osmium

import industry

ROOT = Path(__file__).resolve().parents[1]


class Conversion(unittest.TestCase):
    def test_pbf_readback_preserves_every_directed_arc_and_metric(self):
        work = ROOT / "data" / "industry-tests"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as name:
            folder = Path(name)
            industry.fixture(folder / "input.bin")
            g = industry.read_input(folder / "input.bin")
            pbf = industry.export(folder / "input.bin", folder, slot=3)["pbf"]

            class Read(osmium.SimpleHandler):
                def __init__(self):
                    super().__init__()
                    self.nodes, self.ways = {}, {}

                def node(self, n):
                    self.nodes[n.id] = (n.location.lon, n.location.lat)

                def way(self, w):
                    self.ways[w.id] = ([n.ref for n in w.nodes], dict(w.tags))

            loaded = Read()
            loaded.apply_file(str(pbf))
            self.assertEqual(len(loaded.nodes), g["n"])
            self.assertEqual(len(loaded.ways), g["m"])
            for i, (lon, lat) in loaded.nodes.items():
                self.assertAlmostEqual(lon, float(g["lon"][i - 1]), delta=1e-7)
                self.assertAlmostEqual(lat, float(g["lat"][i - 1]), delta=1e-7)
            for a in range(g["m"]):
                nodes, tags = loaded.ways[a + 1]
                self.assertEqual(nodes, [int(g["tail"][a]) + 1, int(g["head"][a]) + 1])
                self.assertEqual(tags["oneway"], "yes")
                self.assertEqual(int(tags["arc_ms"]), g["metrics"][3, a])
                self.assertEqual([int(tags[f"arc_ms_{j}"]) for j in range(8)], g["metrics"][:, a].tolist())
            del g  # Release memory-mapped input before Windows removes the fixture directory.

    def test_invalid_input_is_rejected(self):
        work = ROOT / "data" / "industry-tests"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as name:
            path = Path(name) / "input.bin"
            industry.fixture(path)
            raw = path.read_bytes()
            for bad in (b"", raw[:20], raw[:250], raw[:16] + b"\xff" * 4 + raw[20:]):
                path.write_bytes(bad)
                with self.assertRaises(ValueError):
                    industry.read_input(path)

    def test_workload_has_one_timed_query_per_pass_and_does_not_repeat_order(self):
        orders = []
        for p in range(3):
            plan = industry.schedule(1000, p)
            orders.append([s for s, _ in plan])
            self.assertEqual(sorted(i for _, mine in plan for i in mine), list(range(1000)))
            self.assertTrue(all((i + p) % 8 == s for s, mine in plan for i in mine))
        self.assertNotEqual(orders[0], orders[1])


if __name__ == "__main__":
    unittest.main()
