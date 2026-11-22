use crate::radiance::value;

const RUN: u8 = 128;
const CHANNELS: usize = 4;

pub(crate) fn decode(bytes: &[u8], width: u32, height: u32, what: &str) -> Vec<[f32; 3]> {
    let mut cursor = Cursor {
        bytes,
        offset: 0,
        what,
    };
    let mut texels = Vec::with_capacity(width as usize * height as usize);
    let mut row = vec![[0u8; 4]; width as usize];
    for _ in 0..height {
        if (8..=0x7fff).contains(&width) {
            let start = cursor.offset;
            let head = cursor.take(4);
            let scans = head[0] == 2 && head[1] == 2;
            if scans {
                let declared = u32::from(head[2]) << 8 | u32::from(head[3]);
                assert_eq!(
                    declared, width,
                    "{what} holds a scanline of {declared} texels where its picture holds {width}",
                );
                cursor.scanline(&mut row);
            } else {
                cursor.offset = start;
                flat(&mut cursor, &mut row);
            }
        } else {
            flat(&mut cursor, &mut row);
        }
        texels.extend(row.iter().copied().map(value));
    }
    texels
}

fn flat(cursor: &mut Cursor<'_>, row: &mut [[u8; 4]]) {
    for texel in row.iter_mut() {
        let head = cursor.take(4);
        *texel = [head[0], head[1], head[2], head[3]];
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    what: &'a str,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> &[u8] {
        let end = self.offset + count;
        assert!(
            end <= self.bytes.len(),
            "{} ends inside a scanline",
            self.what,
        );
        let held = &self.bytes[self.offset..end];
        self.offset = end;
        held
    }

    fn byte(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn scanline(&mut self, row: &mut [[u8; 4]]) {
        for channel in 0..CHANNELS {
            let mut at = 0;
            while at < row.len() {
                let count = self.byte();
                if count > RUN {
                    let value = self.byte();
                    let run = usize::from(count - RUN);
                    assert!(
                        at + run <= row.len(),
                        "{} overruns a scanline of {} texels",
                        self.what,
                        row.len(),
                    );
                    for texel in &mut row[at..at + run] {
                        texel[channel] = value;
                    }
                    at += run;
                } else {
                    let run = usize::from(count);
                    assert!(
                        run > 0 && at + run <= row.len(),
                        "{} overruns a scanline of {} texels",
                        self.what,
                        row.len(),
                    );
                    let values = self.take(run);
                    for (texel, value) in row[at..at + run].iter_mut().zip(values) {
                        texel[channel] = *value;
                    }
                    at += run;
                }
            }
        }
    }
}
