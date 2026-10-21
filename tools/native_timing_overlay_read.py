#!/usr/bin/env python3
"""Read a V3 1280x720 capture PNG as a TRANSCRIPTION AID, never review proof.
The reviewer must still look at the actual image. No request, upload or approval.
Pillow is the only non-stdlib dependency. Font/geometry come from production Rust.
"""
import argparse
import json
import re
from pathlib import Path
from PIL import Image, ImageDraw
from native_timing_overlay import parse_lines

SOURCE = Path(__file__).resolve().parents[1] / 'src/native_timing/overlay.rs'

def font():
    source = SOURCE.read_text()
    rows = re.findall(r"\(b'(.)', \[([0-9, ]+)\]\)", source)
    glyphs = {c: tuple(map(int, row.split(','))) for c, row in rows}
    if len(glyphs) != int(re.search(r'pub static GLYPHS: .*; (\d+)\]', source)[1]):
        raise ValueError('Rust font table could not be read completely')
    geometry = {key: int(re.search(r'pub const '+key+r': usize = (\d+);', source)[1])
                for key in ('PIXEL', 'ADVANCE', 'LEFT', 'TOP', 'LINE_HEIGHT', 'BOX_HEIGHT', 'MAX_COLUMNS')}
    return glyphs, geometry

def render(lines):
    glyphs, g = font()
    image = Image.new('L', (960, 544), 32)
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, 959, g['BOX_HEIGHT']-1), fill=0)
    for line, text in enumerate(lines):
        if len(text) > g['MAX_COLUMNS']:
            raise ValueError('overlay too wide')
        for col, c in enumerate(text):
            for y, row in enumerate(glyphs[c]):
                for x in range(4):
                    if row & (1 << (3-x)):
                        left = g['LEFT']+col*g['ADVANCE']+x*g['PIXEL']
                        top = g['TOP']+line*g['LINE_HEIGHT']+y*g['PIXEL']
                        draw.rectangle((left, top, left+g['PIXEL']-1, top+g['PIXEL']-1), fill=255)
    return image

def decode(image):
    if image.size != (1280, 720):
        raise ValueError('expected a 1280x720 driver capture PNG')
    glyphs, g = font()
    image = image.convert('L')
    pixels = image.load()
    sx, sy = 1280/960, 720/544
    def sample(col, line, ox, oy):
        return tuple(sum((1 << (3-x)) if pixels[
            round(ox+(g['LEFT']+col*g['ADVANCE']+(x+.5)*g['PIXEL'])*sx-.5),
            round(oy+(g['TOP']+line*g['LINE_HEIGHT']+(y+.5)*g['PIXEL'])*sy-.5)] >= 128 else 0
            for x in range(4)) for y in range(7))
    def distance(a, b):
        return sum(bin(x ^ y).count('1') for x, y in zip(a, b))
    # Locate the distinctive first four glyphs and opaque upper box, allowing
    # small capture offsets. A wrong placement must still pass every line CRC.
    candidates = []
    for oy in range(0, 17):
        for ox in range(0, 17):
            score = sum(distance(sample(i, 0, ox, oy), glyphs[c]) for i, c in enumerate('run='))
            if score <= 4:
                candidates.append((score, ox, oy))
    if not candidates:
        raise ValueError('V3 overlay box/grid not located')
    failures = []
    for score, ox, oy in sorted(candidates):
        if oy+g['BOX_HEIGHT']*sy > 720 or ox+953*sx > 1280:
            continue
        # All sampled left-margin background cells must belong to the black box.
        if any(pixels[ox+1, round(oy+y*sy)] > 80 for y in range(0, g['BOX_HEIGHT'], 3)):
            continue
        try:
            lines = []
            for line, count in enumerate((51, 52, 73, 73)):
                text = ''
                for col in range(count):
                    row = sample(col, line, ox, oy)
                    ranked = sorted((distance(row, bitmap), c) for c, bitmap in glyphs.items())
                    if ranked[0][0] > 2 or ranked[1][0] - ranked[0][0] < 2:
                        raise ValueError(f'line {line+1}, column {col+1}: ambiguous glyph')
                    text += ranked[0][1]
                lines.append(text)
            parse_lines(lines)
            return dict(lines=lines, checks_valid=True, box=[ox, oy, 1280, round(oy+g['BOX_HEIGHT']*sy)],
                        status='TRANSCRIPTION AID ONLY; reviewer must inspect the actual image')
        except ValueError as error:
            failures.append(str(error))
    raise ValueError('overlay decode/CRC refused: '+('; '.join(dict.fromkeys(failures)) or 'opaque box not found'))

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('capture', type=Path)
    a = p.parse_args()
    try:
        with Image.open(a.capture) as capture:
            if capture.format != 'PNG':
                raise ValueError('expected PNG input')
            result = decode(capture)
        print(json.dumps(result, indent=2))
    except (OSError, ValueError) as error:
        p.error(str(error))

if __name__ == '__main__':
    main()
