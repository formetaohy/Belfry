use belfry_gpu_device::{AddressMode, Backend, Filter, GpuContext, GpuRequest};
use belfry_render::environment::{LEVELS, Panorama, SPECULAR_WIDTH};
use belfry_render::{Renderer, SamplerDesc, TextureFormat};
use std::f32::consts::PI;

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;

#[test]
fn a_baked_environment_lights_a_uniform_sky_evenly_and_holds_its_hemispheres_apart() {
    for backend in belfry_gpu_device::PREFERENCE {
        baked(*backend);
    }
}

fn baked(backend: Backend) {
    let context = GpuContext::open(&GpuRequest {
        backend: Some(backend),
        ..Default::default()
    })
    .unwrap_or_else(|error| panic!("no {backend:?} device bakes an environment: {error}"));
    let mut renderer = Renderer::new(&context, TextureFormat::Rgba8Unorm);
    let sampler = renderer.sampler(SamplerDesc {
        min_filter: Filter::Linear,
        mag_filter: Filter::Linear,
        mip_filter: Filter::Linear,
        address_u: AddressMode::Repeat,
        address_v: AddressMode::ClampToEdge,
    });
    let uniform = Panorama::new(WIDTH, HEIGHT, &vec![1.0; (WIDTH * HEIGHT * 3) as usize]);
    let texture = uniform.texture(&renderer, "uniform sky");
    let held = renderer.bake(&texture, &sampler);
    assert_eq!(held.levels, LEVELS);
    assert_eq!(held.radiance.format(), TextureFormat::Rgba16Float);
    assert_eq!(held.radiance.mip_count(), LEVELS);
    assert_eq!(
        (held.radiance.width(), held.radiance.height()),
        (SPECULAR_WIDTH, SPECULAR_WIDTH / 2),
    );
    let irradiance = held.irradiance;
    assert!(
        (irradiance[0].x * 0.282_095 - PI).abs() < 1e-2,
        "a uniform sky of one bakes the ambient {irradiance:?}",
    );
    assert!((irradiance[0].y - irradiance[0].x).abs() < 1e-2);
    assert!((irradiance[0].z - irradiance[0].x).abs() < 1e-2);
    for coefficient in &irradiance[1..] {
        assert!(
            coefficient.length() < 0.1,
            "{backend:?} bakes the uniform lobe {coefficient:?}",
        );
    }

    let mut rgb = Vec::new();
    for y in 0..HEIGHT {
        let texel = if y < HEIGHT / 2 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        for _ in 0..WIDTH {
            rgb.extend_from_slice(&texel);
        }
    }
    let split = Panorama::new(WIDTH, HEIGHT, &rgb);
    let texture = split.texture(&renderer, "split sky");
    let held = renderer.bake(&texture, &sampler);
    assert!(
        held.irradiance[1].x > 0.0 && held.irradiance[1].z < 0.0,
        "{backend:?} bakes a red northern and a blue southern hemisphere into {:?}",
        held.irradiance[1],
    );
}

#[test]
fn an_atmosphere_paints_a_blue_day_and_a_red_sunset() {
    let day = belfry_render::environment::Atmosphere::default();
    let (radiance, background) = day.panoramas(128);
    let zenith = radiance.texel(64, 2);
    assert!(zenith.z > zenith.y && zenith.y > zenith.x, "{zenith:?}");
    assert!(zenith.z > 0.0);
    let mut lit = 0.0f32;
    let mut disk = 0.0f32;
    for y in 0..radiance.height() {
        for x in 0..radiance.width() {
            let held = radiance.texel(x, y);
            lit = lit.max(held.x.max(held.y).max(held.z));
            let held = background.texel(x, y);
            disk = disk.max(held.x.max(held.y).max(held.z));
        }
    }
    assert!(lit < 10.0, "the indirect sky keeps no sun disk: {lit}");
    assert!(disk > 1000.0, "the visible sky holds its sun disk: {disk}");

    let sunset = belfry_render::environment::Atmosphere {
        sun_elevation: 1.0,
        ..belfry_render::environment::Atmosphere::default()
    };
    let (radiance, _) = sunset.panoramas(128);
    let horizon = radiance.texel(64, 62);
    let zenith = radiance.texel(64, 2);
    assert!(
        horizon.x > horizon.z * 4.0,
        "a setting sun reddens the horizon {horizon:?}",
    );
    assert!(
        zenith.z > zenith.x * 1.5,
        "a setting sun leaves the zenith blue {zenith:?}",
    );
    for y in 0..radiance.height() {
        for x in 0..radiance.width() {
            let held = radiance.texel(x, y);
            assert!(
                held.x.is_finite() && held.y.is_finite() && held.z.is_finite(),
                "an atmosphere holds {held:?} at ({x}, {y})",
            );
        }
    }
}
