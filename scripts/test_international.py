"""Tests for the international GraphML encoder; no network access is used."""

import struct
import unittest

from international import encode


def graph(nodes, edges, *, directed="directed"):
    return f'''<?xml version="1.0"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns">
  <key id="lon" for="node" attr.name="x" attr.type="double"/>
  <key id="lat" for="node" attr.name="y" attr.type="double"/>
  <key id="time" for="edge" attr.name="travel_time_min" attr.type="double"/>
  <key id="isdir" for="edge" attr.name="directed" attr.type="boolean"/>
  <graph id="G" edgedefault="{directed}">
    {nodes}
    {edges}
  </graph>
</graphml>'''.encode()


class InternationalTests(unittest.TestCase):
    def test_reindexes_and_preserves_directed_parallel_arcs(self):
        raw = graph(
            '<node id="osm:8"><data key="lon">-1.5</data><data key="lat">51</data></node>'
            '<node id="osm:3"><data key="lon">2</data><data key="lat">48</data></node>',
            '<edge source="osm:8" target="osm:3"><data key="time">2</data></edge>'
            '<edge source="osm:8" target="osm:3"><data key="time">0</data></edge>'
            '<edge source="osm:3" target="osm:8"><data key="time">1.5</data></edge>',
        )
        binary, n, m = encode(raw)
        self.assertEqual((n, m), (2, 3))
        self.assertEqual(struct.unpack_from("<QQ", binary), (2, 3))
        self.assertEqual(struct.unpack_from("<2d", binary, 16), (51.0, 48.0))
        self.assertEqual(struct.unpack_from("<2d", binary, 32), (-1.5, 2.0))
        tail_at = 16 + 16 * n
        self.assertEqual(struct.unpack_from("<3I", binary, tail_at), (0, 0, 1))
        self.assertEqual(struct.unpack_from("<3I", binary, tail_at + 4 * m), (1, 1, 0))
        self.assertEqual(struct.unpack_from("<3d", binary, tail_at + 8 * m), (120.0, 0.0, 90.0))
        self.assertEqual(len(binary), 16 + 16 * n + 8 * m + 8 * m)

    def test_rejects_undirected_graph_or_edge(self):
        nodes = '<node id="a"><data key="lon">0</data><data key="lat">0</data></node>'
        edge = '<edge source="a" target="a"><data key="time">1</data></edge>'
        with self.assertRaisesRegex(ValueError, "directed graph"):
            encode(graph(nodes, edge, directed="undirected"))
        undirected = '<edge source="a" target="a" directed="false"><data key="time">1</data></edge>'
        with self.assertRaisesRegex(ValueError, "undirected edge"):
            encode(graph(nodes, undirected))
        flagged = '<edge source="a" target="a"><data key="time">1</data><data key="isdir">false</data></edge>'
        with self.assertRaisesRegex(ValueError, "undirected edge"):
            encode(graph(nodes, flagged))

    def test_rejects_missing_time_unknown_endpoint_and_nonfinite_values(self):
        node = '<node id="a"><data key="lon">0</data><data key="lat">0</data></node>'
        with self.assertRaisesRegex(ValueError, "travel_time_min"):
            encode(graph(node, '<edge source="a" target="a"/>'))
        with self.assertRaisesRegex(ValueError, "unknown node"):
            encode(graph(node, '<edge source="a" target="missing"><data key="time">1</data></edge>'))
        bad_time = '<edge source="a" target="a"><data key="time">NaN</data></edge>'
        with self.assertRaisesRegex(ValueError, "finite and nonnegative"):
            encode(graph(node, bad_time))
        bad_coord = '<node id="a"><data key="lon">inf</data><data key="lat">0</data></node>'
        with self.assertRaisesRegex(ValueError, "coordinates must be finite"):
            encode(graph(bad_coord, ""))


if __name__ == "__main__":
    unittest.main()
