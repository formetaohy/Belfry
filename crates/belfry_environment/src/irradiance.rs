use crate::panorama::{Panorama, direction};
use belfry_math::Vec3;
use std::f32::consts::{PI, TAU};

const LAMBERT: [f32; 3] = [PI, 2.0 * PI / 3.0, PI / 4.0];
const BAND: [usize; 9] = [0, 1, 1, 1, 2, 2, 2, 2, 2];

pub fn irradiance(panorama: &Panorama) -> [Vec3; 9] {
    let width = panorama.width();
    let height = panorama.height();
    let mut coefficients = [Vec3::ZERO; 9];
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        let solid = (TAU / width as f32) * (PI / height as f32) * (v * PI).sin();
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let basis = basis(direction(u, v));
            let radiance = panorama.texel(x, y) * solid;
            for (at, held) in coefficients.iter_mut().enumerate() {
                *held += radiance * (basis[at] * LAMBERT[BAND[at]]);
            }
        }
    }
    coefficients
}

fn basis(direction: Vec3) -> [f32; 9] {
    let x = direction.x;
    let y = direction.y;
    let z = direction.z;
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}
