"""Convert SALSA-CLRS test sets into plain text graphs for `cargo run --bin salsa`.

Source: https://huggingface.co/datasets/SALSA-CLRS/SALSA-CLRS (Apache-2.0), one zip per algorithm of
PyTorch Geometric samples. The zip is read by HTTP range requests, so only the requested members are
transferred, and each `.pt` member is decoded by a restricted unpickler into numpy arrays: no torch.

Usage: python scripts/salsa.py ALGO [GRAPHS_PER_SET]   (ALGO: bfs or dijkstra; default all 1,000;
a smaller count takes the first members in archive order, which are contiguous and fast to fetch)

Writes data/salsa/ALGO/FAMILY_N.txt. Each graph is a block
    n m s
    m lines "u v" (bfs) or "u v w" (dijkstra; w the stored 32-bit weight, shortest round-trip form)
    one line of n reference parents pi
"""

import io
import pickle
import re
import ssl
import sys
import urllib.request
import zipfile
from collections import OrderedDict
from pathlib import Path

import numpy as np

URL = "https://huggingface.co/datasets/SALSA-CLRS/SALSA-CLRS/resolve/main/{}.zip"
DATA = Path(__file__).resolve().parent.parent / "data" / "salsa"
DTYPES = {"LongStorage": np.int64, "IntStorage": np.int32, "FloatStorage": np.float32,
          "DoubleStorage": np.float64, "BoolStorage": np.bool_, "ByteStorage": np.uint8}

ctx = ssl.create_default_context()
# Some proxies re-sign with chains lacking a Subject Key Identifier; chain and hostname checks remain.
ctx.verify_flags &= ~ssl.VERIFY_X509_STRICT


class Remote(io.RawIOBase):
    """A seekable read-only file over HTTP range requests."""

    def __init__(self, url):
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD"), timeout=60, context=ctx) as r:
            self.url, self.size = r.url, int(r.headers["Content-Length"])
        self.pos = 0

    def seekable(self):
        return True

    def readable(self):
        return True

    def seek(self, off, whence=0):
        self.pos = (off, self.pos + off, self.size + off)[whence]
        return self.pos

    def tell(self):
        return self.pos

    def readinto(self, b):
        end = min(self.pos + len(b), self.size)
        if self.pos >= end:
            return 0
        req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{end - 1}"})
        with urllib.request.urlopen(req, timeout=120, context=ctx) as r:
            data = r.read()
        b[: len(data)] = data
        self.pos += len(data)
        return len(data)


class Record(dict):
    """Stand-in for the pickled PyG classes: keeps their state as a dict."""

    def __setstate__(self, state):
        self.update(state)


def load_pt(blob):
    """The tensor mapping of one torch.save'd PyG sample, as numpy arrays."""
    z = zipfile.ZipFile(io.BytesIO(blob))
    root = z.namelist()[0].split("/")[0]

    def rebuild(storage, offset, size, stride, *_):
        if not size:
            return storage[offset]
        step = storage.itemsize
        return np.lib.stride_tricks.as_strided(storage[offset:], size, [s * step for s in stride]).copy()

    class Unpickler(pickle.Unpickler):
        def find_class(self, module, name):
            if (module, name) == ("torch._utils", "_rebuild_tensor_v2"):
                return rebuild
            if module == "torch" and name in DTYPES:
                return DTYPES[name]
            if (module, name) == ("collections", "OrderedDict"):
                return OrderedDict
            if module.startswith(("torch_geometric.", "salsaclrs.")):
                return Record
            raise pickle.UnpicklingError(f"refusing {module}.{name}")

        def persistent_load(self, pid):
            _, dtype, key, _, _ = pid
            return np.frombuffer(z.read(f"{root}/data/{key}"), dtype=dtype)

    return Unpickler(io.BytesIO(z.read(f"{root}/data.pkl"))).load()["_store"]["_mapping"]


def parents(sample, n):
    """Reference parent of each node from the edge-located pointer output `pi`."""
    src, dst = sample["edge_index"]
    pi = np.arange(n)
    hot = sample["pi"].reshape(-1) > 0.5
    pi[src[hot]] = dst[hot]
    return pi


def main():
    algo = sys.argv[1]
    per_set = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
    z = zipfile.ZipFile(io.BufferedReader(Remote(URL.format(algo)), 1 << 20))
    sets = {}
    for info in z.infolist():
        m = re.search(r"/test/graphgenerator=(\w+?)_.*n=(\d+)-nohints/processed/data_(\d+)\.pt$", info.filename)
        if m:
            sets.setdefault(f"{m[1]}_{m[2]}", []).append((int(m[3]), info))
    out = DATA / algo
    out.mkdir(parents=True, exist_ok=True)
    for name, members in sorted(sets.items()):
        members.sort(key=lambda x: x[1].header_offset)
        blocks = {}
        for i, info in members[:per_set]:
            g = load_pt(z.read(info))
            n = len(g["pos"])
            src, dst = g["edge_index"]
            s = int(np.argmax(g["s"]))
            lines = [f"{n} {len(src)} {s}"]
            if "weights" in g:
                w = [np.format_float_positional(x, unique=True) for x in g["weights"].reshape(-1)]
                lines += [f"{u} {v} {x}" for u, v, x in zip(src.tolist(), dst.tolist(), w)]
            else:
                lines += [f"{u} {v}" for u, v in zip(src.tolist(), dst.tolist())]
            lines.append(" ".join(map(str, parents(g, n).tolist())))
            blocks[i] = "\n".join(lines)
        (out / f"{name}.txt").write_text("\n".join(blocks[i] for i in sorted(blocks)) + "\n", encoding="utf-8")
        print(f"{out / name}.txt: {len(blocks)} graphs")


if __name__ == "__main__":
    main()
