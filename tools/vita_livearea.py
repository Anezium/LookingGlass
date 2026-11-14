#!/usr/bin/env python3
"""Generate and check original geometric LiveArea art; no game assets or fonts.

Generation is reproducible with Pillow 11.3.0. The optional --vpk check verifies
the real package paths/bytes as well as local image and XML constraints.
"""
import argparse
from io import BytesIO
from pathlib import Path
import xml.etree.ElementTree as ET
import zipfile

import PIL
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'vita'
IMAGES = {'sce_sys/icon0.png': (128, 128),
          'sce_sys/livearea/contents/bg.png': (840, 500),
          'sce_sys/livearea/contents/startup.png': (280, 158)}
TEMPLATE = 'sce_sys/livearea/contents/template.xml'
NAVY, INK, IVORY, RED, BLUE = '#18232f', '#10171f', '#eee6d2', '#bb3548', '#547e93'


def picture(size, kind):
    scale = 4
    w, h = size
    image = Image.new('RGB', (w * scale, h * scale), NAVY)
    draw = ImageDraw.Draw(image)

    def line(points, fill=IVORY, width=2):
        draw.line([(round(x * scale), round(y * scale)) for x, y in points],
                  fill=fill, width=width * scale, joint='curve')

    def polygon(points, fill):
        draw.polygon([(round(x * scale), round(y * scale)) for x, y in points], fill=fill)

    def ellipse(box, fill):
        draw.ellipse(tuple(round(v * scale) for v in box), fill=fill)

    def keyhole(cx, cy, radius):
        # An original doorway/keyhole, with a small red diamond beyond it.
        ellipse((cx - radius, cy - radius, cx + radius, cy + radius), IVORY)
        polygon([(cx - radius * .55, cy), (cx + radius * .55, cy),
                 (cx + radius * .8, cy + radius * 2.25),
                 (cx - radius * .8, cy + radius * 2.25)], IVORY)
        polygon([(cx, cy - radius * .6), (cx + radius * .35, cy),
                 (cx, cy + radius * .6), (cx - radius * .35, cy)], RED)

    def title(x, y, unit):
        # Original monoline letter construction; no embedded or rendered font.
        glyphs = [([(0, 6), (2, 0), (4, 6)], [(1, 4), (3, 4)]),
                  ([(0, 0), (0, 6), (4, 6)],),
                  ([(0, 0), (4, 0)], [(2, 0), (2, 6)], [(0, 6), (4, 6)]),
                  ([(4, 0), (0, 0), (0, 6), (4, 6)],),
                  ([(4, 0), (0, 0), (0, 6), (4, 6)], [(0, 3), (3, 3)])]
        for i, strokes in enumerate(glyphs):
            for stroke in strokes:
                line([(x + (i * 6 + a) * unit, y + b * unit) for a, b in stroke],
                     width=max(1, round(unit / 2)))

    if kind == 'icon':
        ellipse((7, 7, 121, 121), BLUE)
        ellipse((11, 11, 117, 117), INK)
        keyhole(64, 43, 23)
    elif kind == 'startup':
        line([(12, 12), (268, 12), (268, 146), (12, 146), (12, 12)], BLUE)
        keyhole(140, 39, 14)
        title(84, 108, 4)
    else:
        for x in range(-400, 1100, 100):
            line([(x, h), (x + 500, 0)], '#213140', 1)
        polygon([(0, h), (w, h), (w, h - 40), (0, h - 15)], INK)
        line([(30, 28), (810, 28), (810, 472), (30, 472), (30, 28)], BLUE)
        title(72, 70, 10)
        keyhole(190, 256, 42)
        # Small card-like diamonds are pure geometry, not copied game symbols.
        for cx, cy, r in [(610, 110, 14), (680, 160, 9), (725, 85, 6)]:
            polygon([(cx, cy-r), (cx+r*.65, cy), (cx, cy+r), (cx-r*.65, cy)], RED)
    return image.resize(size, Image.Resampling.LANCZOS).quantize(colors=64)


def generate():
    if PIL.__version__ != '11.3.0':
        raise ValueError('Regenerate with the pinned Pillow 11.3.0')
    for relative, size in IMAGES.items():
        destination = ASSETS / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        kind = 'icon' if 'icon0' in relative else 'startup' if 'startup' in relative else 'background'
        picture(size, kind).save(destination, optimize=False)
    xml = ('<?xml version="1.0" encoding="utf-8"?>\n'
           '<livearea style="a1" format-ver="01.00" content-rev="1">\n'
           '  <livearea-background><image>bg.png</image></livearea-background>\n'
           '  <gate><startup-image>startup.png</startup-image></gate>\n'
           '</livearea>\n')
    (ASSETS / TEMPLATE).write_bytes(xml.replace('\n', '\r\n').encode('utf-8'))


def check(vpk=None):
    expected = set(IMAGES) | {TEMPLATE}
    actual = {str(p.relative_to(ASSETS)) for p in ASSETS.rglob('*') if p.is_file()}
    if actual != expected:
        raise ValueError(f'Unexpected static files: {actual ^ expected}')
    for relative, size in IMAGES.items():
        data = (ASSETS / relative).read_bytes()
        with Image.open(BytesIO(data)) as im:
            if im.format != 'PNG' or im.mode != 'P' or im.size != size:
                raise ValueError(f'Invalid LiveArea image: {relative}')
            im.load()
            if len(im.getcolors() or []) > 128 or len(data) > 420 * 1024:
                raise ValueError(f'Palette or size exceeds LiveArea bound: {relative}')
    xml = (ASSETS / TEMPLATE).read_bytes()
    other = xml.replace(b'\r\n', b'')
    if len(xml) > 32 * 1024 or b'\n' in other or b'\r' in other:
        raise ValueError('Template must be UTF-8 CRLF and below32KiB')
    tree = ET.fromstring(xml.decode('utf-8'))
    if tree.tag != 'livearea' or tree.attrib != {'style': 'a1', 'format-ver': '01.00', 'content-rev': '1'}:
        raise ValueError('Unexpected LiveArea template')
    if (tree.findtext('livearea-background/image') != 'bg.png' or
            tree.findtext('gate/startup-image') != 'startup.png'):
        raise ValueError('Template image reference mismatch')
    if vpk:
        with zipfile.ZipFile(vpk) as archive:
            names = archive.namelist()
            if set(names) != expected | {'eboot.bin', 'sce_sys/param.sfo'} or len(names) != len(set(names)):
                raise ValueError('VPK has missing, extra or duplicate members')
            for relative in expected:
                if archive.read(relative) != (ASSETS / relative).read_bytes():
                    raise ValueError(f'VPK asset differs: {relative}')
    print('PASS LiveArea images, palette, dimensions, XML' + (' and actual VPK member bytes' if vpk else '') +
          f' (Pillow {PIL.__version__})')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--generate', action='store_true')
    parser.add_argument('--vpk', type=Path)
    args = parser.parse_args()
    if args.generate:
        generate()
    check(args.vpk)


if __name__ == '__main__':
    main()
