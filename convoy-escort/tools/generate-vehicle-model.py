#!/usr/bin/env python3
"""Build the escort vehicle model the convoy viewer loads.

The model is assembled here from boxes and prisms, so the repository carries
no third-party mesh and the provenance of the geometry is this file. Output is
deterministic: running the script again writes the same bytes.

    python3 tools/generate-vehicle-model.py

Writes a glTF 2.0 binary into convoy-viewer/public/assets/models/. Axes follow
the viewer: +X is forward along the vehicle, +Y is up, +Z is to its right. The
wheels rest on y = -0.75, which is where the viewer puts the terrain, and the
roof reaches y = 1.25, so the model spans the same two metres of height as the
scene expects. Only the standard library is used.
"""

import json
import math
import os
import struct
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir,
                   "convoy-viewer", "public", "assets", "models",
                   "escort-vehicle.glb")

# Material index -> (base colour RGBA, metallic, roughness)
MATERIALS = [
    ("Body", [0.42, 0.45, 0.33, 1.0], 0.15, 0.75),
    ("Glass", [0.10, 0.14, 0.18, 1.0], 0.60, 0.25),
    ("Rubber", [0.07, 0.07, 0.08, 1.0], 0.05, 0.95),
    ("Trim", [0.22, 0.23, 0.20, 1.0], 0.35, 0.60),
]


class Mesh:
    """Flat-shaded triangle soup, one instance per material."""

    def __init__(self):
        self.pos = []
        self.nrm = []
        self.idx = []

    def quad(self, a, b, c, d):
        """One flat quad, wound counter-clockwise seen from outside."""
        ux, uy, uz = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
        vx, vy, vz = (d[0] - a[0], d[1] - a[1], d[2] - a[2])
        nx, ny, nz = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx)
        length = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
        normal = (nx / length, ny / length, nz / length)
        base = len(self.pos)
        for corner in (a, b, c, d):
            self.pos.append(corner)
            self.nrm.append(normal)
        self.idx += [base, base + 1, base + 2, base, base + 2, base + 3]

    def box(self, x0, y0, z0, x1, y1, z1):
        self.quad((x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1))  # +Z
        self.quad((x1, y0, z0), (x0, y0, z0), (x0, y1, z0), (x1, y1, z0))  # -Z
        self.quad((x1, y0, z1), (x1, y0, z0), (x1, y1, z0), (x1, y1, z1))  # +X
        self.quad((x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0))  # -X
        self.quad((x0, y1, z1), (x1, y1, z1), (x1, y1, z0), (x0, y1, z0))  # +Y
        self.quad((x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1))  # -Y

    def wheel(self, cx, cy, cz, radius, half_width, sides=10):
        """A prism around the Z axis, standing in for a tyre."""
        rim = []
        for i in range(sides):
            ang = 2.0 * math.pi * i / sides
            rim.append((cx + math.cos(ang) * radius, cy + math.sin(ang) * radius))
        for i in range(sides):
            x0, y0 = rim[i]
            x1, y1 = rim[(i + 1) % sides]
            self.quad((x0, y0, cz + half_width), (x1, y1, cz + half_width),
                      (x1, y1, cz - half_width), (x0, y0, cz - half_width))
        for sign in (1, -1):                       # the two faces of the tyre
            z = cz + sign * half_width
            for i in range(1, sides - 1):
                x0, y0 = rim[0]
                x1, y1 = rim[i if sign > 0 else i + 1]
                x2, y2 = rim[i + 1 if sign > 0 else i]
                base = len(self.pos)
                for corner in ((x0, y0, z), (x1, y1, z), (x2, y2, z)):
                    self.pos.append(corner)
                    self.nrm.append((0.0, 0.0, float(sign)))
                self.idx += [base, base + 1, base + 2]


def build():
    body, glass, rubber, trim = Mesh(), Mesh(), Mesh(), Mesh()

    # Chassis, cargo bed and cabin. The vehicle points along +X.
    body.box(-2.30, -0.25, -1.05, 2.30, 0.35, 1.05)      # hull
    body.box(-2.15, 0.35, -1.00, 0.05, 0.95, 1.00)       # cargo bed walls
    body.box(0.05, 0.35, -1.00, 1.75, 1.25, 1.00)        # cabin
    body.box(1.75, 0.05, -1.00, 2.30, 0.75, 1.00)        # bonnet

    # Glass: windscreen and the two side windows of the cabin.
    glass.box(1.70, 0.80, -0.92, 1.78, 1.18, 0.92)
    glass.box(0.30, 0.80, -1.02, 1.60, 1.18, -0.94)
    glass.box(0.30, 0.80, 0.94, 1.60, 1.18, 1.02)

    # Trim: bumpers, roof rail and the step below each door.
    trim.box(2.30, -0.10, -1.10, 2.48, 0.25, 1.10)
    trim.box(-2.48, -0.10, -1.10, -2.30, 0.25, 1.10)
    trim.box(0.10, 1.25, -0.90, 1.70, 1.33, 0.90)
    trim.box(0.05, -0.32, -1.18, 1.60, -0.18, -1.05)
    trim.box(0.05, -0.32, 1.05, 1.60, -0.18, 1.18)

    # Four wheels, resting on y = -0.75.
    for ax in (1.45, -1.45):
        for az in (1.08, -1.08):
            rubber.wheel(ax, 0.0, az, 0.75, 0.22)

    return [(body, 0), (glass, 1), (rubber, 2), (trim, 3)]


def pack(parts):
    """Lay every attribute out in one buffer and describe it in glTF JSON."""
    blob = bytearray()
    views, accessors, primitives = [], [], []

    def add_view(data):
        while len(blob) % 4:
            blob.append(0)
        views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(data)})
        blob.extend(data)
        return len(views) - 1

    for mesh, material in parts:
        pos = bytearray()
        lo = [min(p[i] for p in mesh.pos) for i in range(3)]
        hi = [max(p[i] for p in mesh.pos) for i in range(3)]
        for p in mesh.pos:
            pos += struct.pack("<fff", *p)
        nrm = bytearray()
        for n in mesh.nrm:
            nrm += struct.pack("<fff", *n)
        idx = bytearray()
        for i in mesh.idx:
            idx += struct.pack("<H", i)

        a_pos = len(accessors)
        accessors.append({"bufferView": add_view(pos), "componentType": 5126,
                          "count": len(mesh.pos), "type": "VEC3",
                          "min": lo, "max": hi})
        a_nrm = len(accessors)
        accessors.append({"bufferView": add_view(nrm), "componentType": 5126,
                          "count": len(mesh.nrm), "type": "VEC3"})
        a_idx = len(accessors)
        accessors.append({"bufferView": add_view(idx), "componentType": 5123,
                          "count": len(mesh.idx), "type": "SCALAR"})
        primitives.append({"attributes": {"POSITION": a_pos, "NORMAL": a_nrm},
                           "indices": a_idx, "material": material, "mode": 4})

    gltf = {
        "asset": {
            "version": "2.0",
            "generator": "convoy-escort tools/generate-vehicle-model.py",
            "copyright": "Licensed under Apache-2.0 OR MIT, as the rest of this repository",
        },
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "EscortVehicle"}],
        "meshes": [{"name": "EscortVehicle", "primitives": primitives}],
        "materials": [
            {"name": name,
             "pbrMetallicRoughness": {"baseColorFactor": colour,
                                      "metallicFactor": metal,
                                      "roughnessFactor": rough},
             "doubleSided": False}
            for name, colour, metal, rough in MATERIALS
        ],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(blob)}],
    }
    return gltf, bytes(blob)


def main():
    gltf, blob = pack(build())
    body = json.dumps(gltf, separators=(",", ":"), sort_keys=True).encode("utf-8")
    body += b" " * ((4 - len(body) % 4) % 4)
    blob += b"\x00" * ((4 - len(blob) % 4) % 4)
    total = 12 + 8 + len(body) + 8 + len(blob)
    out = (struct.pack("<III", 0x46546C67, 2, total)
           + struct.pack("<II", len(body), 0x4E4F534A) + body
           + struct.pack("<II", len(blob), 0x004E4942) + blob)
    directory = os.path.dirname(OUT)
    if not os.path.isdir(directory):
        sys.exit("missing output directory: %s" % directory)
    with open(OUT, "wb") as handle:
        handle.write(out)
    triangles = sum(len(m.idx) for m, _ in build()) // 3
    print("%s: %d bytes, %d triangles" % (os.path.basename(OUT), total, triangles))


if __name__ == "__main__":
    main()
