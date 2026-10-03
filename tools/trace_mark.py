"""One-shot: trace the mark's regions into simplified polygons.

Not part of the build -- this only exists to recover faithful geometry for the
GDI redraw in src/setup/ui.rs. Run it, paste the emitted arrays into Rust, then
delete it.
"""

import os

from PIL import Image

SRC = "icons/icon.png"

ORANGE = lambda p: p[3] > 128 and p[0] > 140 and p[1] < 170 and p[2] < 110
WHITE = lambda p: p[3] > 128 and p[0] > 195 and p[1] > 195 and p[2] > 195
DARK = lambda p: p[3] > 128 and p[0] < 90 and p[1] < 100 and p[2] < 120


def trace(mask):
    """Moore-neighbourhood boundary trace -> list of contours (pixel coords)."""
    w, h = mask.size
    px = mask.load()
    seen = [[False] * h for _ in range(w)]
    contours = []

    def on(x, y):
        return 0 <= x < w and 0 <= y < h and px[x, y]

    for sx in range(w):
        for sy in range(h):
            if not on(sx, sy) or seen[sx][sy]:
                continue
            contour = []
            cx, cy = sx, sy
            bdir = 4  # start searching west
            start = (sx, sy)
            while True:
                contour.append((cx, cy))
                seen[cx][cy] = True
                moved = False
                for k in range(8):
                    d = (bdir + k) % 8
                    nx, ny = cx + [(-1, -1), (0, -1), (1, -1), (1, 0),
                                   (1, 1), (0, 1), (-1, 1), (-1, 0)][d][0], \
                         cy + [(-1, -1), (0, -1), (1, -1), (1, 0),
                               (1, 1), (0, 1), (-1, 1), (-1, 0)][d][1]
                    if on(nx, ny):
                        bdir = (d + 5) % 8
                        cx, cy = nx, ny
                        moved = True
                        break
                if not moved or (cx, cy) == start:
                    break
            if len(contour) > 12:
                contours.append(contour)
    return contours


def simplify(points, tol):
    """Douglas-Peucker."""
    if len(points) < 3:
        return points
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    x0, y0 = points[0]
    x1, y1 = points[-1]
    dx, dy = x1 - x0, y1 - y0
    norm = (dx * dx + dy * dy) ** 0.5
    worst, idx = 0.0, 0
    for i, (x, y) in enumerate(points[1:-1], 1):
        d = abs(dy * x - dx * y + x1 * y0 - y1 * x0) / norm if norm else \
            ((x - x0) ** 2 + (y - y0) ** 2) ** 0.5
        if d > worst:
            worst, idx = d, i
    if worst > tol:
        a = simplify(points[:idx + 1], tol)
        b = simplify(points[idx:], tol)
        return a[:-1] + b
    return [points[0], points[-1]]


def area(c):
    a = 0.0
    for i in range(len(c)):
        x0, y0 = c[i]
        x1, y1 = c[(i + 1) % len(c)]
        a += x0 * y1 - x1 * y0
    return abs(a) / 2.0


def main():
    src = Image.open(SRC).convert("RGBA")
    full = src.crop(src.getchannel("A").getbbox())
    w, h = full.size
    print("content %dx%d" % (w, h))

    px = full.load()
    for name, test in [("SILHOUETTE", lambda p: p[3] > 128),
                       ("ORANGE", ORANGE), ("WHITE", WHITE), ("DARK", DARK)]:
        mask = Image.new("1", (w, h), 0)
        mp = mask.load()
        for y in range(h):
            for x in range(w):
                if test(px[x, y]):
                    mp[x, y] = 1
        contours = trace(mask)
        contours.sort(key=area, reverse=True)
        print("\n// %s -- %d contour(s)" % (name, len(contours)))
        for i, c in enumerate(contours[:3]):
            pts = simplify(c, 2.0)
            print("//  #%d raw=%d simplified=%d area=%.0f" % (i, len(c), len(pts), area(c)))
            print("    &[" + ", ".join("(%.4f, %.4f)" % (x / w, y / h) for x, y in pts) + "],")


if __name__ == "__main__":
    main()