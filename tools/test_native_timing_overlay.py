"""V3 transcriptions, camera-scale decoding and actual production drawing parity."""
import json
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
from PIL import Image, ImageFilter
from native_timing_overlay import crc16, display_lines, parse_lines
from native_timing_overlay_read import decode, font, render

RUN = '206052db4ecc5124280c723515338d28'
AUTH = '086cebda'*4
TOKEN = [0x608cebd, 0x860bdec, 0x806cdeb, 0x68cebd, 0x80bdec]
START = 0x608060

def camera(image, blur=0, noise=0, seed=1):
    image = image.resize((960, 544), Image.Resampling.LANCZOS).resize((1280, 720), Image.Resampling.BILINEAR)
    if blur:
        image = image.filter(ImageFilter.GaussianBlur(blur))
    if noise:
        rng = random.Random(seed)
        image.putdata([max(0, min(255, v+rng.randint(-noise, noise))) for v in image.tobytes()])
    return image

class Transcription(unittest.TestCase):
    def setUp(self):
        self.lines = display_lines(RUN, TOKEN, AUTH, START)

    def invoke(self, lines, output, fields=False):
        command = [sys.executable, str(Path(__file__).with_name('native_timing_review_request.py')), '--output', str(output)]
        if fields:
            command += ['--run-id', RUN, '--token', *map(str, TOKEN), '--auth', AUTH, '--start-us', str(START),
                        '--checks', *[line.rsplit(' ck=', 1)[1] for line in lines]]
        else:
            for line in lines:
                command += ['--line', line]
        return subprocess.run(command, capture_output=True, text=True)

    def test_crc_known_answer(self):
        self.assertEqual(crc16('123456789'), 0x29b1)

    def test_exact_and_fields_accepted_exclusive(self):
        request = parse_lines(self.lines)
        self.assertEqual(request['run_id'], list(bytes.fromhex(RUN)))
        self.assertEqual(list(request['token'].values())[:5], TOKEN)
        with tempfile.TemporaryDirectory() as temp:
            for fields in (False, True):
                output = Path(temp)/f'{fields}.json'
                result = self.invoke(self.lines, output, fields)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(output.read_text()), request)
                before = output.read_bytes()
                self.assertEqual(self.invoke(self.lines, output, fields).returncode, 2)
                self.assertEqual(output.read_bytes(), before)

    def test_every_confusable_single_substitution_refused(self):
        confusions = {'0':'68', '6':'08', '8':'06', 'c':'e', 'e':'c', 'b':'d', 'd':'b'}
        count = 0
        # Mutate only data, not labels, checksum or group spacing. Each CLI
        # invocation must name its bad line and leave no request file behind.
        with tempfile.TemporaryDirectory() as temp:
            for line_index, line in enumerate(self.lines):
                body = line.rsplit(' ck=', 1)[0]
                after_equals = False
                for i, char in enumerate(body):
                    if char == '=':
                        after_equals = True
                        continue
                    if char == '|':
                        after_equals = False
                    if after_equals:
                        for replacement in confusions.get(char, ''):
                            mutated = self.lines[:]
                            mutated[line_index] = line[:i]+replacement+line[i+1:]
                            with self.subTest(line=line_index+1, column=i, replacement=replacement):
                                with self.assertRaisesRegex(ValueError, f'line {line_index+1} .*CRC mismatch'):
                                    parse_lines(mutated)
                                output = Path(temp)/f'{count}.json'
                                result = self.invoke(mutated, output)
                                self.assertEqual(result.returncode, 2)
                                self.assertIn(f'line {line_index+1}', result.stderr)
                                self.assertFalse(output.exists())
                            count += 1
        self.assertGreater(count, 200)

    def test_lines_file_accepted(self):
        with tempfile.TemporaryDirectory() as temp:
            source, output = Path(temp)/'lines.txt', Path(temp)/'request.json'
            source.write_text('\n'.join(self.lines)+'\n')
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('native_timing_review_request.py')),
                                     '--lines-file', str(source), '--output', str(output)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(output.read_text()), parse_lines(self.lines))

    def test_fields_copied_checks_refuse_bad_data(self):
        with tempfile.TemporaryDirectory() as temp:
            for index in range(4):
                copied = self.lines[:]
                copied[index] = copied[index][:-1]+('0' if copied[index][-1] != '0' else '1')
                output = Path(temp)/f'{index}.json'
                result = self.invoke(copied, output, fields=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn(f'line {index+1}', result.stderr)
                self.assertFalse(output.exists())

    def test_fields_checks_required_and_no_output(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)/'request.json'
            command = [sys.executable, str(Path(__file__).with_name('native_timing_review_request.py')),
                       '--run-id', RUN, '--token', *map(str, TOKEN), '--auth', AUTH, '--output', str(output)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertFalse(output.exists())

    def test_malformed_missing_swapped_and_mixed_frames(self):
        for lines in [self.lines[:3], [self.lines[1], self.lines[0], *self.lines[2:]],
                      [self.lines[0]+' ', *self.lines[1:]],
                      [self.lines[0].replace('ck=', 'ck=0'), *self.lines[1:]]]:
            with self.subTest(lines=lines), self.assertRaises(ValueError):
                parse_lines(lines)
        # CRC is not authentication: a whole valid line copied from another
        # image can pass CRC. The native HMAC/identity checks remain essential.
        mixed = self.lines[:]
        mixed[1] = display_lines(RUN, TOKEN, 'ab'*16, START)[1]
        self.assertEqual(parse_lines(mixed)['token']['auth'], [0xab]*16)

class Reader(unittest.TestCase):
    def test_rust_table_distinct_and_geometry(self):
        glyphs, geometry = font()
        self.assertEqual(len(set(glyphs.values())), len(glyphs))
        self.assertEqual(geometry['PIXEL'], 3)

    def test_synthetic_scale_blur_noise(self):
        count = 0
        for values in [TOKEN, [2**64-1]*5, [1, 2, 1000000, 1, 0]]:
            lines = display_lines(RUN, values, AUTH, START)
            for blur, noise in [(0,0), (.45,8), (.7,16), (.9,24)]:
                with self.subTest(values=values, blur=blur, noise=noise):
                    result = decode(camera(render(lines), blur, noise))
                    self.assertEqual(result['lines'], lines)
                    self.assertTrue(result['checks_valid'])
                    count += 1
        self.assertEqual(count, 12)

    def test_locates_shifted_opaque_box(self):
        lines = display_lines(RUN, TOKEN, AUTH, START)
        image = camera(render(lines), .7, 16)
        shifted = Image.new('L', image.size, 80)
        shifted.paste(image, (5, 7))
        self.assertEqual(decode(shifted)['lines'], lines)

    def test_actual_rust_drawing_and_proof_png(self):
        root = Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])
        lines = (root/'overlay-rust.txt').read_text().splitlines()
        with Image.open(root/'overlay-rust.pgm') as image:
            self.assertEqual(image.tobytes(), render(lines).tobytes())
            scaled = camera(image, .7, 16)
        scaled.save(root/'overlay-rust-camera-v3.png')
        result = decode(scaled)
        self.assertEqual(result['lines'], lines)
        (root/'overlay-reader-proof.json').write_text(json.dumps(result, indent=2)+'\n')
        render(lines).save(root/'overlay-rust-native-v3.png')

    def test_corruption_and_missing_overlay_refused(self):
        lines = display_lines(RUN, TOKEN, AUTH, START)
        altered = lines[:]
        altered[0] = altered[0].replace('2060', '2660', 1)
        with self.assertRaisesRegex(ValueError, 'CRC'):
            decode(camera(render(altered), .5, 8))
        with self.assertRaises(ValueError):
            decode(Image.new('L', (1280, 720), 0))
        with self.assertRaisesRegex(ValueError, '1280x720'):
            decode(render(lines))

if __name__ == '__main__':
    unittest.main()
