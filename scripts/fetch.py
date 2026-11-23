"""Download the pinned Sapient Maze-Hard CSVs into data/ and verify their SHA-256 hashes.

Source: https://huggingface.co/datasets/sapientinc/maze-30x30-hard-1k at the revision below.
The data carry no declared licence, so the repository keeps this fetcher instead of the files.
"""

import hashlib
import ssl
import urllib.request
from pathlib import Path

REV = "549754de3d67eebe57b6b22c7d226b9412d55b07"
SHA256 = {
    "train": "25f9e39b5aee16967da41ba63b2d1d75cf608c2360d8a1419d827f9859dbafa0",
    "test": "5120d52088fb3f8036e62e90103808a79ba58847d8601f9451f40991b0903a62",
}
DATA = Path(__file__).resolve().parent.parent / "data"


def main():
    DATA.mkdir(exist_ok=True)
    context = ssl.create_default_context()
    # Some proxies re-sign with chains lacking a Subject Key Identifier; chain and hostname checks remain.
    context.verify_flags &= ~ssl.VERIFY_X509_STRICT
    for split, digest in SHA256.items():
        path = DATA / f"{split}.csv"
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            url = f"https://huggingface.co/datasets/sapientinc/maze-30x30-hard-1k/resolve/{REV}/{split}.csv"
            with urllib.request.urlopen(url, timeout=60, context=context) as response:
                body = response.read()
            if hashlib.sha256(body).hexdigest() != digest:
                raise SystemExit(f"{split}.csv: SHA-256 mismatch")
            path.write_bytes(body)
        print(f"{path} ok")


if __name__ == "__main__":
    main()
