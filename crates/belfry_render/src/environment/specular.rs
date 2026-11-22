use crate::environment::panorama::{Panorama, direction, unit};
use crate::environment::sample::{hammersley, importance};
use belfry_math::Vec3;

pub const LEVELS: u32 = 8;

pub struct Level {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<[f32; 4]>,
}

pub struct Specular {
    pub levels: Vec<Level>,
}

pub fn specular(panorama: &Panorama) -> Specular {
    let levels = (0..LEVELS)
        .map(|level| {
            let width = (crate::environment::SPECULAR_WIDTH >> level).max(1);
            let height = (width / 2).max(1);
            let roughness = level as f32 / (LEVELS - 1) as f32;
            let mut texels = Vec::with_capacity(width as usize * height as usize);
            for y in 0..height {
                for x in 0..width {
                    let u = (x as f32 + 0.5) / width as f32;
                    let v = (y as f32 + 0.5) / height as f32;
                    let radiance = prefiltered(panorama, direction(u, v), roughness);
                    texels.push([radiance.x, radiance.y, radiance.z, 1.0]);
                }
            }
            Level {
                width,
                height,
                texels,
            }
        })
        .collect();
    Specular { levels }
}

fn prefiltered(panorama: &Panorama, normal: Vec3, roughness: f32) -> Vec3 {
    if roughness == 0.0 {
        return panorama.sample(normal);
    }
    let view = unit(normal);
    let mut sum = Vec3::ZERO;
    let mut weight = 0.0;
    for index in 0..crate::environment::SPECULAR_SAMPLES {
        let half = importance(
            hammersley(index, crate::environment::SPECULAR_SAMPLES),
            roughness,
            view,
        );
        let light = unit(half * (2.0 * view.dot(half)) - view);
        let cosine = view.dot(light).max(0.0);
        if cosine > 0.0 {
            sum += panorama.sample(light) * cosine;
            weight += cosine;
        }
    }
    if weight == 0.0 {
        return panorama.sample(view);
    }
    sum / weight
}
