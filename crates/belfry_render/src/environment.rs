use belfry_gpu_device::{Sampler, Texture, TextureFormat};
use belfry_math::Vec3;

#[derive(Clone)]
pub struct Environment {
    pub radiance: Texture,
    pub panorama: Texture,
    pub sampler: Sampler,
    pub levels: u32,
    pub irradiance: [Vec3; 9],
    pub intensity: f32,
    pub background: bool,
    pub local: bool,
    pub blend: f32,
}

impl Environment {
    pub fn new(
        radiance: Texture,
        panorama: Texture,
        sampler: Sampler,
        levels: u32,
        irradiance: [Vec3; 9],
    ) -> Self {
        assert!(
            levels > 0,
            "an environment of radiance prefiltered to no level lights nothing",
        );
        assert_eq!(
            radiance.format(),
            TextureFormat::Rgba16Float,
            "an environment of radiance holds half float texels",
        );
        assert_eq!(
            radiance.mip_count(),
            levels,
            "an environment of {levels} levels holds a texture of {} levels",
            radiance.mip_count(),
        );
        assert_eq!(
            panorama.format(),
            TextureFormat::Rgba16Float,
            "an environment of panorama holds half float texels",
        );
        Self {
            radiance,
            panorama,
            sampler,
            levels,
            irradiance,
            intensity: 1.0,
            background: true,
            local: false,
            blend: 1.0,
        }
    }

    pub fn lit(mut self, intensity: f32, background: bool) -> Self {
        assert!(
            intensity > 0.0 && intensity.is_finite(),
            "an environment of intensity {intensity} lights nothing",
        );
        self.intensity = intensity;
        self.background = background;
        self
    }

    pub fn local(mut self, blend: f32) -> Self {
        assert!(
            blend > 0.0 && blend.is_finite(),
            "a local environment of blend {blend} fades over no distance",
        );
        self.local = true;
        self.blend = blend;
        self
    }

    pub fn identity(&self) -> (usize, usize, usize) {
        (
            self.radiance.identity(),
            self.panorama.identity(),
            self.sampler.identity(),
        )
    }
}
