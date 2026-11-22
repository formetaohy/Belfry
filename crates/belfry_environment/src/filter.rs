use crate::{Level, Panorama, Sky, irradiance, sky, specular};
use belfry_math::Vec3;

pub struct Prefiltered {
    pub levels: Vec<Level>,
    pub sky: Sky,
    pub irradiance: [Vec3; 9],
}

pub fn filter(panorama: &Panorama) -> Prefiltered {
    filter_with_sky(panorama, panorama)
}

pub fn filter_with_sky(radiance: &Panorama, background: &Panorama) -> Prefiltered {
    Prefiltered {
        levels: specular(radiance).levels,
        sky: sky(background),
        irradiance: irradiance(radiance),
    }
}
