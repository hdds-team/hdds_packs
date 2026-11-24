#!/usr/bin/env python3
"""Draw the arcade-shooter sprite set.

The sprites this demo renders are produced here, from geometry, so the
repository carries no third-party artwork and the provenance of every pixel
is this file. Output is deterministic: running the script again rewrites the
same bytes.

    python3 tools/generate-sprites.py

Writes RGBA PNGs into assets/sprites/, the directory the viewer serves as its
web root. Only the standard library is used, so no toolchain is needed beyond
a Python interpreter.
"""

import math
import os
import struct
import sys
import zlib

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                       os.pardir, "assets", "sprites")


class Canvas:
    """A small RGBA raster with the few primitives the sprites need."""

    def __init__(self, width, height):
        self.w = width
        self.h = height
        self.px = bytearray(width * height * 4)

    def blend(self, x, y, colour):
        """Draw one pixel, source-over, so overlapping shapes keep their edges."""
        x, y = int(x), int(y)
        if not (0 <= x < self.w and 0 <= y < self.h):
            return
        r, g, b, a = colour
        if a == 0:
            return
        i = (y * self.w + x) * 4
        dr, dg, db, da = self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]
        sa = a / 255.0
        out_a = sa + da / 255.0 * (1.0 - sa)
        if out_a <= 0.0:
            return
        self.px[i] = int(round((r * sa + dr / 255.0 * (da / 255.0) * (1 - sa) * 255) / out_a))
        self.px[i + 1] = int(round((g * sa + dg / 255.0 * (da / 255.0) * (1 - sa) * 255) / out_a))
        self.px[i + 2] = int(round((b * sa + db / 255.0 * (da / 255.0) * (1 - sa) * 255) / out_a))
        self.px[i + 3] = int(round(out_a * 255))

    def rect(self, x0, y0, x1, y1, colour):
        for y in range(int(y0), int(y1) + 1):
            for x in range(int(x0), int(x1) + 1):
                self.blend(x, y, colour)

    def ellipse(self, cx, cy, rx, ry, colour):
        if rx <= 0 or ry <= 0:
            return
        for y in range(int(cy - ry), int(cy + ry) + 1):
            for x in range(int(cx - rx), int(cx + rx) + 1):
                dx = (x + 0.5 - cx) / rx
                dy = (y + 0.5 - cy) / ry
                if dx * dx + dy * dy <= 1.0:
                    self.blend(x, y, colour)

    def ring(self, cx, cy, r_inner, r_outer, colour):
        for y in range(int(cy - r_outer) - 1, int(cy + r_outer) + 2):
            for x in range(int(cx - r_outer) - 1, int(cx + r_outer) + 2):
                d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
                if r_inner <= d <= r_outer:
                    self.blend(x, y, colour)

    def triangle(self, p0, p1, p2, colour):
        xs = [p0[0], p1[0], p2[0]]
        ys = [p0[1], p1[1], p2[1]]

        def edge(a, b, p):
            return (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])

        for y in range(int(min(ys)), int(max(ys)) + 1):
            for x in range(int(min(xs)), int(max(xs)) + 1):
                p = (x + 0.5, y + 0.5)
                w0, w1, w2 = edge(p0, p1, p), edge(p1, p2, p), edge(p2, p0, p)
                if (w0 >= 0 and w1 >= 0 and w2 >= 0) or (w0 <= 0 and w1 <= 0 and w2 <= 0):
                    self.blend(x, y, colour)

    def mirror_left_to_right(self):
        """Make the raster symmetric about its vertical axis."""
        for y in range(self.h):
            for x in range(self.w // 2):
                i = (y * self.w + x) * 4
                j = (y * self.w + (self.w - 1 - x)) * 4
                self.px[j:j + 4] = self.px[i:i + 4]

    def write(self, name):
        raw = bytearray()
        stride = self.w * 4
        for y in range(self.h):
            raw.append(0)  # filter type 0, none
            raw += self.px[y * stride:(y + 1) * stride]

        def chunk(tag, data):
            return (struct.pack(">I", len(data)) + tag + data
                    + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

        header = struct.pack(">IIBBBBB", self.w, self.h, 8, 6, 0, 0, 0)
        blob = (b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", header)
                + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
                + chunk(b"IEND", b""))
        path = os.path.join(OUT_DIR, name)
        with open(path, "wb") as handle:
            handle.write(blob)
        return path, len(blob)


# ---- Palette -------------------------------------------------------------

HULL = (214, 221, 230, 255)
HULL_DARK = (140, 150, 166, 255)
COCKPIT = (90, 220, 255, 255)
FLAME = (255, 170, 40, 255)
FLAME_HOT = (255, 240, 170, 255)
EYE = (20, 24, 32, 255)
GLINT = (255, 255, 255, 255)


def ship():
    """The player ship: nose up, two wings, two engines."""
    c = Canvas(48, 48)
    c.triangle((24, 3), (12, 34), (36, 34), HULL)          # hull
    c.triangle((24, 8), (18, 32), (30, 32), HULL_DARK)     # spine shading
    c.triangle((12, 22), (2, 38), (14, 36), HULL_DARK)     # left wing
    c.triangle((36, 22), (46, 38), (34, 36), HULL_DARK)    # right wing
    c.rect(14, 33, 33, 37, HULL)                           # tail block
    c.ellipse(24, 18, 4, 6, COCKPIT)
    c.ellipse(23, 16, 1.6, 2.2, GLINT)
    for ex in (18, 30):                                    # engine flames
        c.triangle((ex - 4, 38), (ex + 4, 38), (ex, 47), FLAME)
        c.triangle((ex - 2, 38), (ex + 2, 38), (ex, 43), FLAME_HOT)
    return c


def alien(body, trim, width, height, eyes, legs):
    """A generic alien: domed body, eyes, and a row of legs."""
    c = Canvas(48, 48)
    c.ellipse(24, 23, width, height, body)
    c.ellipse(24, 19, width * 0.72, height * 0.55, trim)
    span = width * 0.95
    for i in range(legs):                                  # legs
        t = (i + 0.5) / legs
        x = 24 - span + 2 * span * t
        c.rect(x - 1.5, 23 + height - 2, x + 1.5, 23 + height + 5, body)
    for sign in (-1, 1)[:eyes if eyes < 2 else 2]:         # eyes
        c.ellipse(24 + sign * width * 0.38, 20, 3.2, 3.6, GLINT)
        c.ellipse(24 + sign * width * 0.38, 20, 1.6, 1.9, EYE)
    if eyes == 3:
        c.ellipse(24, 14, 2.6, 2.8, GLINT)
        c.ellipse(24, 14, 1.3, 1.4, EYE)
    c.rect(24 - width * 0.4, 28, 24 + width * 0.4, 29, trim)
    return c


def bullet(width, height, core, glow):
    c = Canvas(width, height)
    c.ellipse(width / 2.0, height / 2.0, width * 0.42, height * 0.46, glow)
    c.ellipse(width / 2.0, height / 2.0, width * 0.20, height * 0.34, core)
    return c


def explosion(frame):
    """Four frames of an expanding shell, hot core fading to smoke."""
    c = Canvas(64, 64)
    outer = 10.0 + frame * 8.0
    alpha = 255 - frame * 42
    c.ring(32, 32, outer * 0.62, outer, (255, 110, 30, max(alpha - 40, 30)))
    c.ellipse(32, 32, outer * 0.70, outer * 0.70, (255, 170, 50, max(alpha - 20, 40)))
    c.ellipse(32, 32, outer * 0.42, outer * 0.42, (255, 230, 140, alpha))
    if frame < 2:
        c.ellipse(32, 32, outer * 0.20, outer * 0.20, (255, 255, 240, 255))
    for i in range(10):                                    # radial sparks
        ang = i * (2.0 * math.pi / 10.0) + frame * 0.21
        for step in range(3):
            d = outer + 1.5 + step * 2.0
            c.blend(32 + math.cos(ang) * d, 32 + math.sin(ang) * d,
                    (255, 200, 90, max(alpha - step * 60, 0)))
    return c


def powerup():
    """A capsule pickup: gold shell with a bright cross."""
    c = Canvas(32, 32)
    c.ellipse(16, 16, 13, 13, (255, 200, 60, 255))
    c.ellipse(16, 16, 10, 10, (255, 236, 150, 255))
    c.rect(14, 7, 17, 24, (190, 120, 10, 255))
    c.rect(7, 14, 24, 17, (190, 120, 10, 255))
    c.ellipse(12, 11, 2.5, 2.5, GLINT)
    return c


SPRITES = [
    ("player.png", ship),
    ("alien1.png", lambda: alien((80, 220, 120, 255), (40, 160, 80, 255), 13, 9, 2, 4)),
    ("alien2.png", lambda: alien((90, 210, 240, 255), (30, 140, 190, 255), 11, 8, 2, 6)),
    ("alien3.png", lambda: alien((230, 110, 220, 255), (150, 50, 150, 255), 15, 11, 2, 4)),
    ("boss.png", lambda: alien((170, 110, 240, 255), (96, 50, 170, 255), 20, 14, 3, 6)),
    ("bullet_player.png", lambda: bullet(16, 32, (255, 255, 220, 255), (255, 220, 60, 220))),
    ("bullet_alien.png", lambda: bullet(16, 24, (255, 230, 200, 255), (255, 80, 60, 220))),
    ("explosion1.png", lambda: explosion(0)),
    ("explosion2.png", lambda: explosion(1)),
    ("explosion3.png", lambda: explosion(2)),
    ("explosion4.png", lambda: explosion(3)),
    ("powerup.png", powerup),
]


def main():
    if not os.path.isdir(OUT_DIR):
        sys.exit("missing output directory: %s" % OUT_DIR)
    for name, build in SPRITES:
        canvas = build()
        if name.startswith(("player", "alien", "boss")):
            canvas.mirror_left_to_right()
        path, size = canvas.write(name)
        print("%-20s %3dx%-3d %6d bytes" % (name, canvas.w, canvas.h, size))


if __name__ == "__main__":
    main()
