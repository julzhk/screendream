#!/usr/bin/env python3
"""Generate the placeholder app icon PNGs (no third-party dependencies).

Draws a rounded-square with a violet-to-teal gradient and a glowing ring,
at the sizes cargo-bundle turns into an .icns file.

    python3 scripts/make_icon.py
"""

import math
import struct
import zlib
from pathlib import Path

OUT_DIR = Path(__file__).resolve().parent.parent / "resources" / "icon"
# (file name, pixel size); "@2x" marks retina variants for the .icns encoder.
SIZES = [
    ("icon_16x16.png", 16),
    ("icon_32x32.png", 32),
    ("icon_128x128.png", 128),
    ("icon_256x256.png", 256),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

TOP = (92, 40, 180)  # violet
BOTTOM = (18, 170, 190)  # teal
RING = (255, 240, 200)


def lerp(a, b, t):
    return a + (b - a) * t


def pixel(x, y, n):
    """Return RGBA for pixel (x, y) in an n x n icon, with 2x2 supersampling."""
    acc = [0.0, 0.0, 0.0, 0.0]
    for sx in (0.25, 0.75):
        for sy in (0.25, 0.75):
            u = (x + sx) / n
            v = (y + sy) / n
            # macOS-style rounded square with a margin.
            m, r = 0.1, 0.18
            dx = max(abs(u - 0.5) - (0.5 - m - r), 0.0)
            dy = max(abs(v - 0.5) - (0.5 - m - r), 0.0)
            if math.hypot(dx, dy) > r:
                continue
            col = [lerp(TOP[i], BOTTOM[i], v) for i in range(3)]
            # Glowing ring in the centre.
            d = abs(math.hypot(u - 0.5, v - 0.5) - 0.22)
            glow = math.exp(-((d / 0.03) ** 2))
            col = [lerp(col[i], RING[i], glow) for i in range(3)]
            for i in range(3):
                acc[i] += col[i]
            acc[3] += 255
    return bytes(int(c / 4) for c in acc)


def write_png(path, n):
    rows = b"".join(b"\x00" + b"".join(pixel(x, y, n) for x in range(n)) for y in range(n))

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(rows, 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


if __name__ == "__main__":
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for name, size in SIZES:
        target = OUT_DIR / name
        write_png(target, size)
        print(f"wrote {target}")
