use belfry_math::Vec3;
use std::f32::consts::{PI, TAU};

pub struct Panorama {
    width: u32,
    height: u32,
    texels: Vec<Vec3>,
}

impl Panorama {
    pub fn new(width: u32, height: u32, rgb: &[f32]) -> Self {
        assert!(
            width > 0 && height > 0,
            "a panorama spans no texel of its {width} by {height} picture",
        );
        assert_eq!(
            rgb.len() as u64,
            u64::from(width) * u64::from(height) * 3,
            "a {width} by {height} panorama holds {} channels where its picture holds {}",
            rgb.len(),
            u64::from(width) * u64::from(height) * 3,
        );
        Self {
            width,
            height,
            texels: rgb
                .as_chunks::<3>()
                .0
                .iter()
                .map(|texel| Vec3::new(texel[0], texel[1], texel[2]))
                .collect(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn texel(&self, x: u32, y: u32) -> Vec3 {
        self.texels[(y * self.width + x) as usize]
    }

    pub fn sample(&self, direction: Vec3) -> Vec3 {
        let unit = unit(direction);
        let u = unit.x.atan2(-unit.z) * (0.5 / PI) + 0.5;
        let v = unit.y.clamp(-1.0, 1.0).acos() / PI;
        self.filtered(u, v)
    }

    pub(crate) fn filtered(&self, u: f32, v: f32) -> Vec3 {
        let x = u * self.width as f32 - 0.5;
        let y = (v * self.height as f32 - 0.5).clamp(0.0, (self.height - 1) as f32);
        let first_x = x.floor();
        let first_y = y.floor();
        let amount_x = x - first_x;
        let amount_y = y - first_y;
        let left = wrap(first_x, self.width);
        let right = wrap(first_x + 1.0, self.width);
        let low = first_y as u32;
        let high = (low + 1).min(self.height - 1);
        let low = self.texel(left, low).lerp(self.texel(right, low), amount_x);
        let high = self
            .texel(left, high)
            .lerp(self.texel(right, high), amount_x);
        low.lerp(high, amount_y)
    }
}

pub(crate) fn direction(u: f32, v: f32) -> Vec3 {
    let phi = (u - 0.5) * TAU;
    let theta = v * PI;
    let sine = theta.sin();
    Vec3::new(sine * phi.sin(), theta.cos(), -sine * phi.cos())
}

pub(crate) fn unit(value: Vec3) -> Vec3 {
    let length = value.length();
    if length == 0.0 {
        return Vec3::Z;
    }
    value / length
}

fn wrap(value: f32, width: u32) -> u32 {
    let width = width as i64;
    (((value as i64) % width) + width) as u32 % width as u32
}
