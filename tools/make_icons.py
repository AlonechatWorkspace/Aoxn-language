"""Generate the Aoxn icon set from icons/icon.png.

The source mark is a portrait shield (329x419 inside a 383x545 transparent
canvas) with asymmetric padding, so every output is produced the same way:
crop to the alpha bounding box, then centre the mark on a square canvas.

    python tools/make_icons.py            # writes icons/ and ide/src-tauri/icons/

Kept in the repo because the icon set is a build artifact of the source mark:
regenerating after an artwork change should not mean hand-resizing 40 files.
"""

import os
import struct
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "icons", "icon.png")
# The source mark is read-only input: generated files never land beside it.
DESTS = [os.path.join(ROOT, "ide", "src-tauri", "icons")]

SLATE = (0x1A, 0x29, 0x39)  # the shield outline colour, sampled from the mark
MASTER = 1024  # square master every other size is derived from
BLEED = 0.84  # fraction of the square the mark fills


def square_master():
    """Crop the mark to its content and centre it on a 1024x1024 canvas."""
    src = Image.open(SRC).convert("RGBA")
    mark = src.crop(src.getchannel("A").getbbox())
    box = int(MASTER * BLEED)
    scale = box / max(mark.size)
    mark = mark.resize(
        (max(1, round(mark.width * scale)), max(1, round(mark.height * scale))),
        Image.LANCZOS,
    )
    canvas = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    canvas.paste(
        mark,
        ((MASTER - mark.width) // 2, (MASTER - mark.height) // 2),
        mark,
    )
    return canvas


def at(master, size):
    """The mark at `size` px, on transparency."""
    return master.resize((size, size), Image.LANCZOS)


def on_slate(master, size):
    """The mark at `size` px, flattened onto the slate colour.

    Windows Store logos and Android adaptive icons reject alpha; the slate
    matches the mark's own outline so the shield does not gain a visible plate.
    """
    flat = Image.new("RGBA", (size, size), SLATE + (255,))
    mark = at(master, size)
    flat.paste(mark, (0, 0), mark)
    return flat.convert("RGB")


def android_foreground(master, size):
    """Adaptive-icon foreground: the mark inside the inner 66% safe zone."""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    inner = round(size * 66 / 108)
    mark = at(master, inner)
    canvas.paste(mark, ((size - inner) // 2, (size - inner) // 2), mark)
    return canvas


def write(img, *paths):
    for path in paths:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        img.save(path, "PNG", optimize=True)


def write_icns(master, path):
    """A PNG-backed .icns (types icp4..ic14), assembled by hand.

    PIL cannot write icns on Windows, and the type table is small enough to
    build directly: 'icns' + total length, then length-prefixed chunks.
    """
    chunks = [
        (b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
        (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024),
        (b"ic11", 64), (b"ic12", 128), (b"ic13", 512), (b"ic14", 1024),
    ]
    import io

    body = b""
    for tag, size in chunks:
        buf = io.BytesIO()
        at(master, size).save(buf, "PNG", optimize=True)
        data = buf.getvalue()
        body += tag + struct.pack(">I", len(data) + 8) + data
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    if not os.path.exists(SRC):
        sys.exit("missing %s -- the source mark is the only input" % SRC)
    master = square_master()
    print("master %dx%d, mark fills %d%% of the square" % (MASTER, MASTER, BLEED * 100))

    # A square master next to the source, for the IDE frontend and the README.
    master.save(os.path.join(ROOT, "icons", "mark-1024.png"), "PNG", optimize=True)

    for dest in DESTS:
        if not os.path.isdir(dest):
            continue
        # Flat squares: the app icon and the Windows Store logos.
        for name, size in [
            ("icon.png", 512),
            ("32x32.png", 32),
            ("64x64.png", 64),
            ("128x128.png", 128),
            ("128x128@2x.png", 256),
            ("StoreLogo.png", 50),
            ("Square30x30Logo.png", 30),
            ("Square44x44Logo.png", 44),
            ("Square71x71Logo.png", 71),
            ("Square89x89Logo.png", 89),
            ("Square107x107Logo.png", 107),
            ("Square142x142Logo.png", 142),
            ("Square150x150Logo.png", 150),
            ("Square284x284Logo.png", 284),
            ("Square310x310Logo.png", 310),
        ]:
            target = os.path.join(dest, name)
            if os.path.exists(target):
                write(on_slate(master, size), target)

        ico = os.path.join(dest, "icon.ico")
        if os.path.exists(ico):
            at(master, 256).save(
                ico,
                "ICO",
                sizes=[(16, 16), (24, 24), (32, 32), (48, 48),
                       (64, 64), (128, 128), (256, 256)],
            )

        icns = os.path.join(dest, "icon.icns")
        if os.path.exists(icns):
            write_icns(master, icns)

        # Android: legacy square/round plus the adaptive foreground.
        for folder, density in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96),
                                ("xxhdpi", 144), ("xxxhdpi", 192)]:
            base = os.path.join(dest, "android", "mipmap-" + folder)
            if not os.path.isdir(base):
                continue
            on_slate(master, density).convert("RGB").save(
                os.path.join(base, "ic_launcher.png"), "PNG", optimize=True)
            on_slate(master, density).convert("RGB").save(
                os.path.join(base, "ic_launcher_round.png"), "PNG", optimize=True)
            android_foreground(master, round(density * 108 / 48)).save(
                os.path.join(base, "ic_launcher_foreground.png"), "PNG", optimize=True)

        # iOS: no alpha in the AppIcon set.
        ios_sizes = {
            "AppIcon-20x20@1x.png": 20, "AppIcon-20x20@2x.png": 40,
            "AppIcon-20x20@2x-1.png": 40, "AppIcon-20x20@3x.png": 60,
            "AppIcon-29x29@1x.png": 29, "AppIcon-29x29@2x.png": 58,
            "AppIcon-29x29@2x-1.png": 58, "AppIcon-29x29@3x.png": 87,
            "AppIcon-40x40@1x.png": 40, "AppIcon-40x40@2x.png": 80,
            "AppIcon-40x40@2x-1.png": 80, "AppIcon-40x40@3x.png": 120,
            "AppIcon-60x60@2x.png": 120, "AppIcon-60x60@3x.png": 180,
            "AppIcon-76x76@1x.png": 76, "AppIcon-76x76@2x.png": 152,
            "AppIcon-83.5x83.5@2x.png": 167,
            "AppIcon-512@2x.png": 1024,
        }
        ios_dir = os.path.join(dest, "ios")
        if os.path.isdir(ios_dir):
            for name, size in ios_sizes.items():
                on_slate(master, size).convert("RGB").save(
                    os.path.join(ios_dir, name), "PNG", optimize=True)

        print("wrote", dest)


if __name__ == "__main__":
    main()