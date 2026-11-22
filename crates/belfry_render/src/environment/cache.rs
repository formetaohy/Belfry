use crate::environment::{LEVELS, Level, Prefiltered, SKY_WIDTH, SPECULAR_WIDTH, Sky};
use belfry_math::Vec3;
use half::f16;

pub const CACHE_VERSION: u32 = 2;

const MAGIC: [u8; 8] = *b"BELFENV\0";
const MAX_TEXELS: u64 = 1 << 24;

pub fn encode(prefiltered: &Prefiltered) -> Vec<u8> {
    assert_eq!(
        prefiltered.levels.len(),
        LEVELS as usize,
        "a cached environment holds {LEVELS} radiance levels",
    );
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC);
    push_u32(&mut bytes, CACHE_VERSION);
    push_u32(&mut bytes, LEVELS);
    for (level, held) in prefiltered.levels.iter().enumerate() {
        let Level {
            width,
            height,
            texels,
        } = held;
        let expected = level_extent(level as u32);
        assert_eq!(
            (*width, *height),
            expected,
            "the cached level {level} spans {width} by {height} where {expected:?} stands",
        );
        assert_eq!(
            texels.len() as u64,
            u64::from(*width) * u64::from(*height),
            "the cached level {level} holds {} texels where {} stand",
            texels.len(),
            u64::from(*width) * u64::from(*height),
        );
        push_u32(&mut bytes, *width);
        push_u32(&mut bytes, *height);
        push_texels(&mut bytes, texels);
    }
    let Sky {
        width,
        height,
        texels,
    } = &prefiltered.sky;
    assert!(
        *width > 0 && *height > 0 && *width <= SKY_WIDTH,
        "a cached sky spans {width} by {height}",
    );
    assert_eq!(
        texels.len() as u64,
        u64::from(*width) * u64::from(*height),
        "a cached sky of {width} by {height} holds {} texels",
        texels.len(),
    );
    push_u32(&mut bytes, *width);
    push_u32(&mut bytes, *height);
    push_texels(&mut bytes, texels);
    for coefficient in &prefiltered.irradiance {
        push_f32(&mut bytes, coefficient.x);
        push_f32(&mut bytes, coefficient.y);
        push_f32(&mut bytes, coefficient.z);
    }
    bytes
}

pub fn decode(bytes: &[u8]) -> Option<Prefiltered> {
    let mut cursor = Cursor::new(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return None;
    }
    if cursor.u32()? != CACHE_VERSION {
        return None;
    }
    if cursor.u32()? != LEVELS {
        return None;
    }
    let mut levels = Vec::with_capacity(LEVELS as usize);
    for level in 0..LEVELS {
        let width = cursor.u32()?;
        let height = cursor.u32()?;
        if (width, height) != level_extent(level) {
            return None;
        }
        let texels = cursor.texels(u64::from(width) * u64::from(height))?;
        levels.push(Level {
            width,
            height,
            texels,
        });
    }
    let width = cursor.u32()?;
    let height = cursor.u32()?;
    if width == 0 || height == 0 || width > SKY_WIDTH {
        return None;
    }
    let count = u64::from(width).checked_mul(u64::from(height))?;
    let sky = Sky {
        width,
        height,
        texels: cursor.texels(count)?,
    };
    let mut irradiance = [Vec3::ZERO; 9];
    for coefficient in &mut irradiance {
        *coefficient = Vec3::new(cursor.f32()?, cursor.f32()?, cursor.f32()?);
        if !finite(*coefficient) {
            return None;
        }
    }
    if cursor.remaining() != 0 {
        return None;
    }
    Some(Prefiltered {
        levels,
        sky,
        irradiance,
    })
}

fn level_extent(level: u32) -> (u32, u32) {
    let width = (SPECULAR_WIDTH >> level).max(1);
    (width, (width / 2).max(1))
}

fn push_texels(bytes: &mut Vec<u8>, texels: &[[f32; 4]]) {
    for texel in texels {
        assert!(
            texel.iter().all(|value| value.is_finite()),
            "a cached texel holds {texel:?}",
        );
        for channel in texel {
            bytes.extend_from_slice(&f16::from_f32(*channel).to_bits().to_le_bytes());
        }
    }
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    assert!(value.is_finite(), "a cached coefficient holds {value}");
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let held = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(held)
    }

    fn u16(&mut self) -> Option<u16> {
        let held = self.take(2)?;
        Some(u16::from_le_bytes([held[0], held[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let held = self.take(4)?;
        Some(u32::from_le_bytes([held[0], held[1], held[2], held[3]]))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }

    fn texels(&mut self, count: u64) -> Option<Vec<[f32; 4]>> {
        if count > MAX_TEXELS {
            return None;
        }
        let count = usize::try_from(count).ok()?;
        let bytes = count.checked_mul(8)?;
        if self.remaining() < bytes {
            return None;
        }
        let mut texels = Vec::with_capacity(count);
        for _ in 0..count {
            let red = f16::from_bits(self.u16()?).to_f32();
            let green = f16::from_bits(self.u16()?).to_f32();
            let blue = f16::from_bits(self.u16()?).to_f32();
            let alpha = f16::from_bits(self.u16()?).to_f32();
            if !red.is_finite() || !green.is_finite() || !blue.is_finite() || !alpha.is_finite() {
                return None;
            }
            texels.push([red, green, blue, alpha]);
        }
        Some(texels)
    }
}
