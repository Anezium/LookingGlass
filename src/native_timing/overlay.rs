//! Camera-readable review text. All font pixels are 3x3 screen pixels.
//! This table is the sole font source; the host reader parses it directly.
use super::{schedule::Token, AnchorText};
use std::fmt::Write;
pub const PIXEL: usize = 3;
pub const ADVANCE: usize = 13;
pub const LEFT: usize = 4;
pub const TOP: usize = 4;
pub const LINE_HEIGHT: usize = 30;
pub const BOX_HEIGHT: usize = 120;
pub const MAX_COLUMNS: usize = 73;
pub static GLYPHS: [(u8, [u8; 7]); 40] = [
    (b'0', [6, 9, 11, 9, 13, 9, 6]),
    (b'1', [2, 6, 2, 2, 2, 2, 7]),
    (b'2', [6, 9, 1, 2, 4, 8, 15]),
    (b'3', [14, 1, 1, 6, 1, 1, 14]),
    (b'4', [2, 6, 10, 10, 15, 2, 2]),
    (b'5', [15, 8, 8, 14, 1, 1, 14]),
    (b'6', [7, 8, 8, 14, 9, 9, 6]),
    (b'7', [15, 1, 2, 2, 4, 4, 4]),
    (b'8', [6, 9, 9, 6, 9, 9, 6]),
    (b'9', [6, 9, 9, 7, 1, 1, 14]),
    (b'a', [0, 0, 6, 1, 7, 9, 7]),
    (b'b', [8, 8, 14, 9, 9, 9, 14]),
    (b'c', [0, 0, 7, 8, 8, 8, 7]),
    (b'd', [1, 1, 7, 9, 9, 9, 7]),
    (b'e', [0, 0, 6, 9, 15, 8, 7]),
    (b'f', [3, 4, 4, 14, 4, 4, 4]),
    (b'g', [0, 0, 7, 9, 7, 1, 14]),
    (b'h', [8, 8, 14, 9, 9, 9, 9]),
    (b'i', [2, 0, 6, 2, 2, 2, 7]),
    (b'j', [1, 0, 1, 1, 1, 9, 6]),
    (b'k', [8, 8, 9, 10, 12, 10, 9]),
    (b'l', [6, 2, 2, 2, 2, 2, 7]),
    (b'm', [0, 0, 10, 15, 15, 9, 9]),
    (b'n', [0, 0, 14, 9, 9, 9, 9]),
    (b'o', [0, 0, 6, 9, 9, 9, 6]),
    (b'p', [0, 14, 9, 9, 14, 8, 8]),
    (b'q', [0, 7, 9, 9, 7, 1, 1]),
    (b'r', [0, 0, 11, 12, 8, 8, 8]),
    (b's', [0, 0, 7, 8, 6, 1, 14]),
    (b't', [4, 4, 14, 4, 4, 4, 3]),
    (b'u', [0, 0, 9, 9, 9, 9, 7]),
    (b'v', [0, 0, 9, 9, 9, 6, 6]),
    (b'w', [0, 0, 9, 9, 15, 15, 6]),
    (b'x', [0, 0, 9, 9, 6, 9, 9]),
    (b'y', [0, 9, 9, 9, 7, 1, 14]),
    (b'z', [0, 0, 15, 1, 6, 8, 15]),
    (b'=', [0, 0, 15, 0, 15, 0, 0]),
    (b'|', [2, 2, 2, 2, 2, 2, 2]),
    (b'-', [0, 0, 0, 15, 0, 0, 0]),
    (b' ', [0, 0, 0, 0, 0, 0, 0]),
];
pub const STATIC_BYTES: usize = std::mem::size_of::<[(u8, [u8; 7]); 40]>();

pub fn crc16(text: &str) -> u16 {
    let mut crc = 0xffffu16;
    for b in text.bytes().filter(|b| *b != b' ') {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn hex(text: &mut AnchorText, bytes: &[u8]) -> std::fmt::Result {
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 2 == 0 {
            text.write_char(' ')?;
        }
        write!(text, "{b:02x}")?;
    }
    Ok(())
}
fn check(text: &mut AnchorText, start: usize, newline: bool) -> std::fmt::Result {
    let crc = crc16(&text.as_str()[start..]);
    write!(text, " ck={crc:04x}")?;
    if newline {
        text.write_char('\n')?;
    }
    Ok(())
}
/// Fixed four lines, including every u64 value, fit in 953x120 pixels.
/// Numeric display is hexadecimal; the ACK remains the unchanged JSON u64 format.
pub fn review(run: [u8; 16], t: Token, start_us: u64) -> AnchorText {
    let mut text = AnchorText {
        bytes: [0; 384],
        len: 0,
    };
    let result = (|| -> std::fmt::Result {
        text.write_str("run=")?;
        hex(&mut text, &run)?;
        check(&mut text, 0, true)?;
        let start = text.len;
        text.write_str("auth=")?;
        hex(&mut text, &t.auth)?;
        check(&mut text, start, true)?;
        for (i, fields) in [
            [("t", t.ordinal), ("f", t.viewer_seq), ("u", t.submitted_us)],
            [("g", t.generation), ("o", t.operation), ("s", start_us)],
        ]
        .into_iter()
        .enumerate()
        {
            let start = text.len;
            for (j, (label, value)) in fields.into_iter().enumerate() {
                if j > 0 {
                    text.write_char('|')?;
                }
                write!(text, "{label}=")?;
                hex(&mut text, &value.to_be_bytes())?;
            }
            check(&mut text, start, i == 0)?;
        }
        Ok(())
    })();
    assert!(result.is_ok(), "fixed overlay buffer");
    text
}
/// Production drawing geometry, independent of a graphics context. No heap use.
pub fn rectangles(text: &str, mut fill: impl FnMut(usize, usize, usize, usize)) {
    for (line, content) in text.lines().enumerate() {
        for (col, byte) in content.bytes().enumerate() {
            if let Some((_, rows)) = GLYPHS.iter().find(|(c, _)| *c == byte) {
                for (y, row) in rows.iter().enumerate() {
                    for x in 0..4 {
                        if row & (1 << (3 - x)) != 0 {
                            fill(
                                LEFT + col * ADVANCE + x * PIXEL,
                                TOP + line * LINE_HEIGHT + y * PIXEL,
                                PIXEL,
                                PIXEL,
                            );
                        }
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_standard_vector_and_grouping() {
        assert_eq!(crc16("123456789"), 0x29b1);
        assert_eq!(crc16("run=0123 4567"), crc16("run=01234567"));
        assert_eq!(STATIC_BYTES, std::mem::size_of_val(&GLYPHS));
        for (i, (c, rows)) in GLYPHS.iter().enumerate() {
            assert!(rows.iter().all(|r| *r < 16));
            assert!(GLYPHS[..i]
                .iter()
                .all(|(other, bitmap)| other != c && bitmap != rows));
        }
    }
    #[test]
    fn full_width_review_fits_and_checks() {
        let text = review(
            [0xff; 16],
            Token {
                ordinal: u64::MAX,
                viewer_seq: u64::MAX,
                submitted_us: u64::MAX,
                generation: u64::MAX,
                operation: u64::MAX,
                auth: [0xff; 16],
            },
            u64::MAX,
        );
        assert_eq!(text.as_str().lines().count(), 4);
        for line in text.as_str().lines() {
            assert!(line.len() <= MAX_COLUMNS);
            let (body, check) = line.rsplit_once(" ck=").unwrap();
            assert_eq!(format!("{:04x}", crc16(body)), check);
        }
        rectangles(text.as_str(), |x, y, w, h| {
            assert!(x + w <= 960 && y + h <= BOX_HEIGHT);
            assert_eq!((w, h), (3, 3));
        });
    }
    #[test]
    fn emit_actual_drawing_for_host_reader() {
        let Some(root) = std::env::var_os("ALICE_TIMING_TEST_OUTPUT") else {
            return;
        };
        let run = [
            0x20, 0x60, 0x52, 0xdb, 0x4e, 0xcc, 0x51, 0x24, 0x28, 0x0c, 0x72, 0x35, 0x15, 0x33,
            0x8d, 0x28,
        ];
        let text = review(
            run,
            Token {
                ordinal: 449,
                viewer_seq: 449,
                submitted_us: 124234492,
                generation: 1,
                operation: 9,
                auth: [0xce; 16],
            },
            405633091,
        );
        let mut pixels = vec![32u8; 960 * 544];
        pixels[..960 * BOX_HEIGHT].fill(0);
        rectangles(text.as_str(), |x, y, w, h| {
            for dy in 0..h {
                for dx in 0..w {
                    pixels[(y + dy) * 960 + x + dx] = 255;
                }
            }
        });
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        let mut pgm = b"P5\n960 544\n255\n".to_vec();
        pgm.extend_from_slice(&pixels);
        std::fs::write(root.join("overlay-rust.pgm"), pgm).unwrap();
        std::fs::write(root.join("overlay-rust.txt"), text.as_str()).unwrap();
    }
}
