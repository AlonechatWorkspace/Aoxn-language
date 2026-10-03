"""Trace the mark's regions out of icons/icon.png.

Two consumers, one trace:
  * icons/mark.svg      -- the vector master, for the README and the IDE
                           frontend. Scales cleanly where the rasters turn to
                           mush at 16 px.
  * the MARK_* arrays in src/setup/ui.rs -- the installer's GDI redraw, pasted
                           in by hand.

Not part of the build. Run it after changing the artwork, then paste whatever
it prints.

SVG's default `nonzero` fill is the equivalent of PSO_WINDING, which is what
the GDI side has to use: these contours self-intersect where Douglas-Peucker
cut corners, and under PSO_ALTERNATE their interior cancels to nothing --
CreatePolygonRgn then returns NULL with no error set and FillRgn silently
draws nothing.

    python tools/trace_mark.py
"""

import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "icons", "icon.png")
SVG_OUT = os.path.join(ROOT, "icons", "mark.svg")

SILHOUETTE = lambda p: p[3] > 128  # noqa: E731
ORANGE = lambda p: p[3] > 128 and p[0] > 140 and p[1] < 170 and p[2] < 110
RED = lambda p: p[3] > 128 and p[0] > 140 and p[1] < 95 and p[2] < 80
WHITE = lambda p: p[3] > 128 and p[0] > 195 and p[1] > 195 and p[2] > 195

# name, fill colour, mask test, which of its contours, how many.
# WHITE holds both the arrow (largest) and the little hook above its arm, so
# the hook is picked by skipping the arrow rather than by its own colour.
LAYERS = [
    ("SILHOUETTE", "#1A2939", SILHOUETTE, 0, 1),
    ("ORANGE", "#EE6227", ORANGE, 0, 1),
    ("RED", "#DD3D22", RED, 0, 1),
    ("ARROW", "#ECEBEF", WHITE, 0, 1),
    ("HOOK", "#ECEBEF", WHITE, 1, 1),
]

NEIGHBOURS = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)]


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
                    nx, ny = cx + NEIGHBOURS[d][0], cy + NEIGHBOURS[d][1]
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


def denoise(points, eps):
    """Drop vertices hugging the line through their neighbours.

    Traced antialiased edges leave runs of near-duplicate points a pixel
    apart. Douglas-Peucker keeps the ones it judged locally significant, which
    makes the vector file noisy without changing the shape.
    """
    if len(points) < 5:
        return points
    out = [points[0]]
    for i in range(1, len(points) - 1):
        ax, ay = out[-1]
        bx, by = points[i]
        cx, cy = points[(i + 1) % len(points)]
        ux, uy = cx - ax, cy - ay
        norm = (ux * ux + uy * uy) ** 0.5
        d = abs(ux * (ay - by) - uy * (ax - bx)) / norm if norm else \
            ((bx - ax) ** 2 + (by - ay) ** 2) ** 0.5
        if d >= eps:
            out.append((bx, by))
    out.append(points[-1])
    return out if len(out) >= 3 else points


def area(c):
    a = 0.0
    for i in range(len(c)):
        x0, y0 = c[i]
        x1, y1 = c[(i + 1) % len(c)]
        a += x0 * y1 - x1 * y0
    return abs(a) / 2.0


def extract(full, test, offset, take):
    """`take` contours of `test`, starting at `offset`, simplified and denoised."""
    w, h = full.size
    px = full.load()
    mask = Image.new("1", (w, h), 0)
    mp = mask.load()
    for y in range(h):
        for x in range(w):
            if test(px[x, y]):
                mp[x, y] = 1
    contours = trace(mask)
    contours.sort(key=area, reverse=True)
    picked = []
    for c in contours[offset:offset + take]:
        pts = denoise(simplify(c, 2.0), 1.1)
        if area(pts) > 200:  # drop antialiasing specks
            picked.append(pts)
    return picked


def write_svg(groups, w, h):
    paths = []
    for _name, colour, polys in groups:
        for poly in polys:
            d = " ".join("%s%.1f,%.1f" % ("M" if i == 0 else "L", x, y)
                         for i, (x, y) in enumerate(poly))
            paths.append('  <path fill="%s" d="%sZ"/>' % (colour, d))
    svg = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        "<!-- The Aoxn mark. Traced from icons/icon.png by tools/trace_mark.py;\n"
        "     do not hand-edit, re-run the tracer. -->\n"
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d"\n'
        '     width="%d" height="%d" role="img" aria-label="Aoxn">\n'
        "%s\n</svg>\n"
    ) % (w, h, w, h, "\n".join(paths))
    with open(SVG_OUT, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(svg)
    print("wrote %s (%d paths)" % (SVG_OUT, len(paths)))


def main():
    src = Image.open(SRC).convert("RGBA")
    full = src.crop(src.getchannel("A").getbbox())
    w, h = full.size
    print("content %dx%d" % (w, h))

    groups = []
    for name, colour, test, offset, take in LAYERS:
        groups.append((name, colour, extract(full, test, offset, take)))

    print()
    for name, _colour, polys in groups:
        for i, poly in enumerate(polys):
            print("const MARK_%s%d: &[(f64, f64)] = &[" % (name, i))
            for j in range(0, len(poly), 4):
                print("    " + " ".join("(%.4f, %.4f)" % (x / w, y / h)
                                        for x, y in poly[j:j + 4]))
            print("];")
            print()

    write_svg(groups, w, h)


if __name__ == "__main__":
    main()