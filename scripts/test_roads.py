import struct
import unittest

import numpy as np
import pandas as pd

from roads import encode


SLOT = "weekday_am_peak_p50"


def tables():
    nodes = pd.DataFrame({"node_id": [0, 1, 2], "lat": [1.0, 2.0, 3.0], "lon": [4.0, 5.0, 6.0]})
    edges = pd.DataFrame({
        "source": [0, 1], "target": [1, 2], "length_m": [100.0, 50.0],
        "oneway": [True, False], "default_speed_kmh": [20.0, 10.0], SLOT: [36.0, np.nan],
    })
    return nodes, edges


class EncodeTests(unittest.TestCase):
    def test_layout_direction_weight_and_fallback(self):
        data, n, m = encode(*tables(), SLOT)
        self.assertEqual((n, m), (3, 3))
        self.assertEqual(struct.unpack_from("<QQ", data), (3, 3))
        offset = 16 + 3 * 16
        self.assertEqual(np.frombuffer(data, "<u4", count=m, offset=offset).tolist(), [0, 1, 2])
        self.assertEqual(np.frombuffer(data, "<u4", count=m, offset=offset + 4*m).tolist(), [1, 2, 1])
        weights = np.frombuffer(data, "<f8", count=m, offset=offset + 8*m)
        np.testing.assert_allclose(weights, [10.0, 18.0, 18.0])

    def test_validation(self):
        bad_cases = [
            ("target", [1, 3]), ("target", [1, -1]), ("target", [1, 1.5]),
            ("lat", [1.0, np.nan, 3.0]), ("length_m", [100.0, -1.0]),
            ("length_m", [100.0, np.inf]), ("oneway", [True, None]),
        ]
        for column, values in bad_cases:
            with self.subTest(column=column, values=values):
                nodes, edges = tables()
                (nodes if column in ("lat", "lon", "node_id") else edges)[column] = values
                with self.assertRaises(ValueError):
                    encode(nodes, edges, SLOT)

        nodes, edges = tables()
        nodes.loc[1, "node_id"] = 2
        with self.assertRaises(ValueError):
            encode(nodes, edges, SLOT)

    def test_missing_default_speed_is_rejected(self):
        nodes, edges = tables()
        edges.loc[1, "default_speed_kmh"] = np.nan
        with self.assertRaises(ValueError):
            encode(nodes, edges, SLOT)


if __name__ == "__main__":
    unittest.main()
