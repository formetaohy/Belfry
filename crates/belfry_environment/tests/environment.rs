use belfry_environment::{LEVELS, Panorama, brdf, decode, encode, filter, irradiance, specular};
use std::f32::consts::PI;

const WIDTH: u32 = 32;
const HEIGHT: u32 = 16;

#[test]
fn a_uniform_sky_lights_every_direction_the_same() {
    let panorama = Panorama::new(WIDTH, HEIGHT, &vec![1.0; (WIDTH * HEIGHT * 3) as usize]);
    let held = irradiance(&panorama);
    assert!((held[0].x * 0.282_095 - PI).abs() < 1e-2);
    assert!((held[0].y - held[0].x).abs() < 1e-2);
    assert!((held[0].z - held[0].x).abs() < 1e-2);
    for coefficient in &held[1..] {
        assert!(coefficient.length() < 0.1, "{:?}", coefficient);
    }
    let prefiltered = specular(&panorama);
    assert_eq!(prefiltered.levels.len() as u32, LEVELS);
    for level in &prefiltered.levels {
        for texel in &level.texels {
            assert!((texel[0] - 1.0).abs() < 2e-2);
            assert!((texel[1] - 1.0).abs() < 2e-2);
            assert!((texel[2] - 1.0).abs() < 2e-2);
        }
    }
    for texel in brdf(16) {
        assert!((0.0..=1.0).contains(&texel[0]));
        assert!((0.0..=1.0).contains(&texel[1]));
        assert!(texel[0] + texel[1] <= 1.001);
    }
}

#[test]
fn a_prefiltered_environment_survives_its_cache() {
    let mut rgb = Vec::new();
    for _ in 0..4 {
        for x in 0..8 {
            if !(2..6).contains(&x) {
                rgb.extend_from_slice(&[2.0, 0.0, 0.0]);
            } else {
                rgb.extend_from_slice(&[0.0, 0.0, 2.0]);
            }
        }
    }
    let panorama = Panorama::new(8, 4, &rgb);
    let held = filter(&panorama);
    let bytes = encode(&held);
    let restored = decode(&bytes).expect("a cached environment loads its prefiltered sky");
    assert_eq!(restored.levels.len(), held.levels.len());
    for (left, right) in held.levels.iter().zip(&restored.levels) {
        assert_eq!((left.width, left.height), (right.width, right.height));
        assert_eq!(left.texels.len(), right.texels.len());
        for (left, right) in left.texels.iter().zip(&right.texels) {
            for channel in 0..4 {
                assert!((left[channel] - right[channel]).abs() < 2e-3);
            }
        }
    }
    assert_eq!(
        (held.sky.width, held.sky.height),
        (restored.sky.width, restored.sky.height)
    );
    for (left, right) in held.sky.texels.iter().zip(&restored.sky.texels) {
        for channel in 0..4 {
            assert!((left[channel] - right[channel]).abs() < 2e-3);
        }
    }
    for (left, right) in held.irradiance.iter().zip(&restored.irradiance) {
        assert!((left.x - right.x).abs() < 2e-3);
        assert!((left.y - right.y).abs() < 2e-3);
        assert!((left.z - right.z).abs() < 2e-3);
    }
    assert!(decode(&bytes[..bytes.len() - 1]).is_none());
    assert!(decode(&[0u8; 8]).is_none());
}

#[test]
fn an_atmosphere_paints_a_blue_day_and_a_red_sunset() {
    let day = belfry_environment::Atmosphere::default();
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

    let sunset = belfry_environment::Atmosphere {
        sun_elevation: 1.0,
        ..belfry_environment::Atmosphere::default()
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
