"""Renders Sabine's icon masters in assets/icon into the files its crates ship.

Needs ImageMagick 7 built with librsvg. Run it again after changing a master.
"""

import struct
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MASTERS = ROOT / "assets/icon"
NAME = "com.lantharos.sabine"
HICOLOR = ROOT / "crates/sabine-service/icons/hicolor"
ICO = ROOT / "crates/sabine-service/icons/sabine.ico"
ICNS = ROOT / "crates/sabine-host/shared/mac/sabine.icns"

HICOLOR_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_TYPES = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"ic11", 32),
    (b"icp6", 64),
    (b"ic12", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic13", 256),
    (b"ic09", 512),
    (b"ic14", 512),
    (b"ic10", 1024),
]
SMALL_UP_TO = 32


def render(master, size, scratch):
    output = scratch / f"{master.stem}-{size}.png"
    if not output.exists():
        source_size = 1024 if master.stem.endswith("macos") else 64
        subprocess.run(
            [
                "magick",
                "-background",
                "none",
                "-density",
                str(96 * size / source_size),
                str(master),
                "-resize",
                f"{size}x{size}",
                "-depth",
                "8",
                "-define",
                "png:color-type=6",
                "-strip",
                str(output),
            ],
            check=True,
        )
    return output.read_bytes()


def tile(size, scratch):
    master = "sabine-small.svg" if size <= SMALL_UP_TO else "sabine.svg"
    return render(MASTERS / master, size, scratch)


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def ico(images):
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries = b""
    for size, png in images:
        dimension = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dimension, dimension, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
    return header + entries + b"".join(png for _, png in images)


def icns(images):
    body = b"".join(kind + struct.pack(">I", len(png) + 8) + png for kind, png in images)
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def main():
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        write(HICOLOR / f"scalable/apps/{NAME}.svg", (MASTERS / "sabine.svg").read_bytes())
        for size in HICOLOR_SIZES:
            write(HICOLOR / f"{size}x{size}/apps/{NAME}.png", tile(size, scratch))
        write(ICO, ico([(size, tile(size, scratch)) for size in ICO_SIZES]))
        macos = MASTERS / "sabine-macos.svg"
        write(ICNS, icns([(kind, render(macos, size, scratch)) for kind, size in ICNS_TYPES]))


if __name__ == "__main__":
    main()
