use crate::environment::panorama::Panorama;

pub const SKY_WIDTH: u32 = 2048;

pub struct Sky {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<[f32; 4]>,
}

pub fn sky(panorama: &Panorama) -> Sky {
    let width = panorama.width().min(SKY_WIDTH);
    let height = if width == panorama.width() {
        panorama.height()
    } else {
        (u64::from(panorama.height()) * u64::from(width) / u64::from(panorama.width())).max(1)
            as u32
    };
    let mut texels = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let texel = panorama.filtered(u, v);
            texels.push([texel.x, texel.y, texel.z, 1.0]);
        }
    }
    Sky {
        width,
        height,
        texels,
    }
}
