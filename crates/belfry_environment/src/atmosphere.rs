use crate::Panorama;
use belfry_math::Vec3;
use std::f32::consts::{PI, TAU};

pub const MODEL_VERSION: u32 = 1;

const GROUND_RADIUS_KM: f32 = 6360.0;
const THICKNESS_KM: f32 = 60.0;
const RAYLEIGH_HEIGHT_KM: f32 = 8.0;
const MIE_HEIGHT_KM: f32 = 1.2;
const OZONE_CENTER_KM: f32 = 25.0;
const OZONE_HALF_WIDTH_KM: f32 = 15.0;
const RAYLEIGH_SCATTERING_KM: Vec3 = Vec3::new(0.005_802, 0.013_558, 0.033_1);
const MIE_SCATTERING_KM: f32 = 0.003_996;
const MIE_ABSORPTION_KM: f32 = 0.004_44;
const OZONE_ABSORPTION_KM: Vec3 = Vec3::new(0.000_65, 0.001_881, 0.000_085);
const SUN_ANGULAR_RADIUS: f32 = 0.004_675;
const TRANS_W: usize = 256;
const TRANS_H: usize = 64;
const MULTI_W: usize = 32;
const MULTI_H: usize = 32;
const MULTI_DIRECTIONS: usize = 64;
const TRANS_STEPS: usize = 40;
const MULTI_STEPS: usize = 20;
const SKY_STEPS: usize = 24;
const SEGMENT_OFFSET: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub sun_elevation: f32,
    pub sun_azimuth: f32,
    pub sun_intensity: f32,
    pub rayleigh: f32,
    pub mie: f32,
    pub mie_anisotropy: f32,
    pub ozone: f32,
    pub ground_albedo: Vec3,
    pub altitude: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            sun_elevation: 40.0,
            sun_azimuth: 0.0,
            sun_intensity: 3.0,
            rayleigh: 1.0,
            mie: 1.0,
            mie_anisotropy: 0.8,
            ozone: 1.0,
            ground_albedo: Vec3::splat(0.1),
            altitude: 0.0,
        }
    }
}

impl Atmosphere {
    pub fn sun(&self) -> Vec3 {
        let elevation = self.sun_elevation.to_radians();
        let azimuth = self.sun_azimuth.to_radians();
        Vec3::new(
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            -elevation.cos() * azimuth.cos(),
        )
    }

    pub fn panoramas(&self, width: u32) -> (Panorama, Panorama) {
        assert!(
            width >= 32 && width.is_multiple_of(2),
            "an atmosphere panorama spans {width} texels",
        );
        let params = self.parameters();
        let luts = Luts::new(&params);
        let sun = self.sun();
        let height = width / 2;
        let altitude = params.altitude.max(0.001);
        let observer = Vec3::new(0.0, GROUND_RADIUS_KM + altitude, 0.0);
        let disk = Disk {
            direction: sun,
            radiance: self.sun_disk(&luts, observer),
        };
        let mut radiance = Vec::with_capacity(height as usize * width as usize * 3);
        let mut background = Vec::with_capacity(height as usize * width as usize * 3);
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            for x in 0..width {
                let u = (x as f32 + 0.5) / width as f32;
                let direction = crate::panorama::direction(u, v);
                let lit = sky(&params, &luts, observer, direction, sun);
                assert!(
                    lit.x.is_finite() && lit.y.is_finite() && lit.z.is_finite(),
                    "an atmosphere renders the texel ({x}, {y}) as {lit:?}",
                );
                radiance.push(lit.x);
                radiance.push(lit.y);
                radiance.push(lit.z);
                let mut background_lit = lit;
                let angle = direction.dot(disk.direction).clamp(-1.0, 1.0).acos();
                let half = 0.5 * ((TAU / width as f32) * (v * PI).sin()).max(PI / height as f32);
                let coverage = ((SUN_ANGULAR_RADIUS + half - angle) / (2.0 * half)).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    background_lit += disk.radiance * coverage;
                }
                assert!(
                    background_lit.x.is_finite()
                        && background_lit.y.is_finite()
                        && background_lit.z.is_finite(),
                    "an atmosphere lights the sky texel ({x}, {y}) as {background_lit:?}",
                );
                background.push(background_lit.x);
                background.push(background_lit.y);
                background.push(background_lit.z);
            }
        }
        (
            Panorama::new(width, height, &radiance),
            Panorama::new(width, height, &background),
        )
    }

    fn sun_disk(&self, luts: &Luts, observer: Vec3) -> Vec3 {
        let solid = PI * SUN_ANGULAR_RADIUS * SUN_ANGULAR_RADIUS;
        let sun = self.sun();
        let transmittance = luts
            .transmittance
            .sun(observer.length() - GROUND_RADIUS_KM, sun);
        transmittance * (1.0 / solid)
    }

    fn parameters(&self) -> Parameters {
        assert!(
            self.sun_elevation.is_finite() && (-90.0..=90.0).contains(&self.sun_elevation),
            "an atmosphere holds the sun elevation {} beyond the sky",
            self.sun_elevation,
        );
        assert!(
            self.sun_azimuth.is_finite(),
            "an atmosphere holds the sun azimuth {}",
            self.sun_azimuth,
        );
        assert!(
            self.sun_intensity > 0.0 && self.sun_intensity.is_finite(),
            "an atmosphere of intensity {} lights nothing",
            self.sun_intensity,
        );
        assert!(
            self.rayleigh >= 0.0 && self.rayleigh.is_finite(),
            "an atmosphere scatters {} of its Rayleigh light",
            self.rayleigh,
        );
        assert!(
            self.mie >= 0.0 && self.mie.is_finite(),
            "an atmosphere scatters {} of its Mie light",
            self.mie,
        );
        assert!(
            self.mie_anisotropy.is_finite() && self.mie_anisotropy.abs() < 1.0,
            "an atmosphere scatters Mie light {} forward",
            self.mie_anisotropy,
        );
        assert!(
            self.ozone >= 0.0 && self.ozone.is_finite(),
            "an atmosphere absorbs {} of its ozone light",
            self.ozone,
        );
        assert!(
            self.ground_albedo.x >= 0.0
                && self.ground_albedo.x <= 1.0
                && self.ground_albedo.y >= 0.0
                && self.ground_albedo.y <= 1.0
                && self.ground_albedo.z >= 0.0
                && self.ground_albedo.z <= 1.0,
            "an atmosphere reflects its ground at {:?}",
            self.ground_albedo,
        );
        assert!(
            self.altitude >= 0.0
                && self.altitude.is_finite()
                && self.altitude < THICKNESS_KM * 1000.0,
            "an atmosphere sits {} meters above its ground",
            self.altitude,
        );
        assert!(
            self.rayleigh > 0.0 || self.mie > 0.0 || self.ozone > 0.0,
            "an atmosphere without Rayleigh, Mie or ozone light scatters nothing",
        );
        Parameters {
            rayleigh: RAYLEIGH_SCATTERING_KM * self.rayleigh,
            mie: Vec3::splat(MIE_SCATTERING_KM * self.mie),
            mie_absorption: Vec3::splat(MIE_ABSORPTION_KM * self.mie),
            ozone: OZONE_ABSORPTION_KM * self.ozone,
            mie_anisotropy: self.mie_anisotropy,
            ground_albedo: self.ground_albedo,
            altitude: self.altitude / 1000.0,
        }
    }
}

struct Disk {
    direction: Vec3,
    radiance: Vec3,
}

struct Parameters {
    rayleigh: Vec3,
    mie: Vec3,
    mie_absorption: Vec3,
    ozone: Vec3,
    mie_anisotropy: f32,
    ground_albedo: Vec3,
    altitude: f32,
}

struct Scattering {
    rayleigh: Vec3,
    mie: Vec3,
    extinction: Vec3,
}

struct Luts {
    transmittance: Lut,
    multiple: Lut,
}

impl Luts {
    fn new(params: &Parameters) -> Self {
        let transmittance = Lut::new(TRANS_W, TRANS_H, |u, v| {
            let height = v * THICKNESS_KM;
            let cosine = u * 2.0 - 1.0;
            transmittance(params, height, cosine)
        });
        let multiple = Lut::new(MULTI_W, MULTI_H, |u, v| {
            let height = v * THICKNESS_KM;
            let cosine = u * 2.0 - 1.0;
            multiple(params, &transmittance, height, cosine)
        });
        Self {
            transmittance,
            multiple,
        }
    }
}

struct Lut {
    width: usize,
    height: usize,
    texels: Vec<Vec3>,
}

impl Lut {
    fn new(width: usize, height: usize, mut texel: impl FnMut(f32, f32) -> Vec3) -> Self {
        let mut texels = Vec::with_capacity(width * height);
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            for x in 0..width {
                let u = (x as f32 + 0.5) / width as f32;
                texels.push(texel(u, v));
            }
        }
        Self {
            width,
            height,
            texels,
        }
    }

    fn sun(&self, height_km: f32, sun: Vec3) -> Vec3 {
        let cosine = sun.y.clamp(-1.0, 1.0);
        self.sample(
            0.5 + 0.5 * cosine,
            (height_km / THICKNESS_KM).clamp(0.0, 1.0),
        )
    }

    fn sample(&self, u: f32, v: f32) -> Vec3 {
        let x = (u.clamp(0.0, 1.0) * self.width as f32 - 0.5).max(0.0);
        let y = (v.clamp(0.0, 1.0) * self.height as f32 - 0.5).max(0.0);
        let left = x.floor() as usize;
        let low = y.floor() as usize;
        let right = (left + 1).min(self.width - 1);
        let high = (low + 1).min(self.height - 1);
        let amount_x = x - left as f32;
        let amount_y = y - low as f32;
        let low = self.texel(left, low).lerp(self.texel(right, low), amount_x);
        let high = self
            .texel(left, high)
            .lerp(self.texel(right, high), amount_x);
        low.lerp(high, amount_y)
    }

    fn texel(&self, x: usize, y: usize) -> Vec3 {
        self.texels[y * self.width + x]
    }
}

fn transmittance(params: &Parameters, height_km: f32, cosine: f32) -> Vec3 {
    let origin = Vec3::new(0.0, GROUND_RADIUS_KM + height_km, 0.0);
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    let direction = Vec3::new(0.0, cosine, -sine);
    if sphere(origin, direction, GROUND_RADIUS_KM).entry > 0.0 {
        return Vec3::ZERO;
    }
    let atmosphere = sphere(origin, direction, GROUND_RADIUS_KM + THICKNESS_KM);
    if atmosphere.exit <= 0.0 {
        return Vec3::ZERO;
    }
    march_transmittance(origin, direction, atmosphere.exit.max(0.0), params)
}

fn march_transmittance(origin: Vec3, direction: Vec3, distance: f32, params: &Parameters) -> Vec3 {
    let mut depth = Vec3::ZERO;
    let mut start = 0.0;
    let step = distance / TRANS_STEPS as f32;
    for _ in 0..TRANS_STEPS {
        let end = start + step;
        let at = origin + direction * (start + (end - start) * SEGMENT_OFFSET);
        depth += scattering(params, at.length() - GROUND_RADIUS_KM).extinction * (end - start);
        start = end;
    }
    exp(-depth)
}

fn multiple(params: &Parameters, lookup: &Lut, height_km: f32, cosine: f32) -> Vec3 {
    let origin = Vec3::new(0.0, GROUND_RADIUS_KM + height_km, 0.0);
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    let sun = Vec3::new(0.0, cosine, -sine);
    let golden = PI * (3.0 - 5.0_f32.sqrt());
    let mut luminance = Vec3::ZERO;
    let mut transfer = Vec3::ZERO;
    for index in 0..MULTI_DIRECTIONS {
        let y = 1.0 - 2.0 * (index as f32 + 0.5) / MULTI_DIRECTIONS as f32;
        let radius = (1.0 - y * y).max(0.0).sqrt();
        let angle = golden * index as f32;
        let direction = Vec3::new(radius * angle.cos(), y, radius * angle.sin());
        let atmosphere = sphere(origin, direction, GROUND_RADIUS_KM + THICKNESS_KM);
        if atmosphere.exit <= 0.0 {
            continue;
        }
        let ground = sphere(origin, direction, GROUND_RADIUS_KM);
        let hit = ground.entry > 0.0;
        let distance = if hit { ground.entry } else { atmosphere.exit };
        let ray = march_multiple(params, lookup, origin, direction, distance, sun);
        luminance += ray.luminance;
        transfer += ray.transfer;
        if hit {
            let at = origin + direction * distance;
            let normal = at.normalize();
            let sun_transmittance = lookup.sun(at.length() - GROUND_RADIUS_KM, sun);
            if normal.dot(sun) > 0.0 {
                luminance +=
                    ray.transmittance * params.ground_albedo * sun_transmittance * normal.dot(sun);
            }
        }
    }
    let scale = 1.0 / MULTI_DIRECTIONS as f32;
    let mut result = Vec3::ZERO;
    for axis in 0..3 {
        let luminance = axis_of(luminance, axis) * scale;
        let transfer = axis_of(transfer, axis) * scale;
        let held = luminance / (1.0 - transfer).max(1.0e-4);
        result = set_axis(result, axis, held);
    }
    result
}

struct Multiple {
    luminance: Vec3,
    transfer: Vec3,
    transmittance: Vec3,
}

fn march_multiple(
    params: &Parameters,
    lookup: &Lut,
    origin: Vec3,
    direction: Vec3,
    distance: f32,
    sun: Vec3,
) -> Multiple {
    let cosine = direction.dot(sun);
    let rayleigh = rayleigh_phase(cosine);
    let mie = mie_phase(cosine, params.mie_anisotropy);
    let mut luminance = Vec3::ZERO;
    let mut transfer = Vec3::ZERO;
    let mut view = Vec3::ONE;
    let mut start = 0.0;
    let step = distance / MULTI_STEPS as f32;
    for _ in 0..MULTI_STEPS {
        let end = start + step;
        let at = origin + direction * (start + (end - start) * SEGMENT_OFFSET);
        let height = at.length() - GROUND_RADIUS_KM;
        let held = scattering(params, height);
        let sun_transmittance = lookup.sun(height, sun);
        let in_scattering =
            held.rayleigh * (sun_transmittance * rayleigh) + held.mie * (sun_transmittance * mie);
        let local = exp(-held.extinction * (end - start));
        luminance += view * integrated(in_scattering, local, held.extinction);
        transfer += view * integrated(held.rayleigh + held.mie, local, held.extinction);
        view = view * local;
        start = end;
    }
    Multiple {
        luminance,
        transfer,
        transmittance: view,
    }
}

fn sky(params: &Parameters, luts: &Luts, observer: Vec3, direction: Vec3, sun: Vec3) -> Vec3 {
    let atmosphere = sphere(observer, direction, GROUND_RADIUS_KM + THICKNESS_KM);
    if atmosphere.exit <= 0.0 {
        return Vec3::ZERO;
    }
    let ground = sphere(observer, direction, GROUND_RADIUS_KM);
    let hit = ground.entry > 0.0;
    let distance = if hit { ground.entry } else { atmosphere.exit };
    if distance <= 0.0 {
        return Vec3::ZERO;
    }
    let cosine = direction.dot(sun);
    let rayleigh = rayleigh_phase(cosine);
    let mie = mie_phase(cosine, params.mie_anisotropy);
    let mut radiance = Vec3::ZERO;
    let mut view = Vec3::ONE;
    let mut start = 0.0;
    let step = distance / SKY_STEPS as f32;
    for _ in 0..SKY_STEPS {
        let end = start + step;
        let at = observer + direction * (start + (end - start) * SEGMENT_OFFSET);
        let height = at.length() - GROUND_RADIUS_KM;
        let held = scattering(params, height);
        let sun_transmittance = luts.transmittance.sun(height, sun);
        let psi = luts.multiple.sun(height, sun);
        let in_scattering = held.rayleigh * (sun_transmittance * rayleigh + psi)
            + held.mie * (sun_transmittance * mie + psi);
        let local = exp(-held.extinction * (end - start));
        radiance += view * integrated(in_scattering, local, held.extinction);
        view = view * local;
        start = end;
    }
    if !hit {
        return radiance;
    }
    let at = observer + direction * distance;
    let normal = at.normalize();
    let sun_transmittance = luts.transmittance.sun(at.length() - GROUND_RADIUS_KM, sun);
    let lit = params.ground_albedo * sun_transmittance * normal.dot(sun).max(0.0);
    radiance + view * lit
}

fn scattering(params: &Parameters, height_km: f32) -> Scattering {
    let rayleigh = params.rayleigh * (-height_km / RAYLEIGH_HEIGHT_KM).exp();
    let mie = params.mie * (-height_km / MIE_HEIGHT_KM).exp();
    let mie_absorption = params.mie_absorption * (-height_km / MIE_HEIGHT_KM).exp();
    let ozone =
        params.ozone * (1.0 - (height_km - OZONE_CENTER_KM).abs() / OZONE_HALF_WIDTH_KM).max(0.0);
    Scattering {
        rayleigh,
        mie,
        extinction: rayleigh + mie + mie_absorption + ozone,
    }
}

fn integrated(inscattering: Vec3, local: Vec3, extinction: Vec3) -> Vec3 {
    Vec3::new(
        (inscattering.x - inscattering.x * local.x) / extinction.x.max(1.0e-12),
        (inscattering.y - inscattering.y * local.y) / extinction.y.max(1.0e-12),
        (inscattering.z - inscattering.z * local.z) / extinction.z.max(1.0e-12),
    )
}

fn rayleigh_phase(cosine: f32) -> f32 {
    (3.0 / (16.0 * PI)) * (1.0 + cosine * cosine)
}

fn mie_phase(cosine: f32, anisotropy: f32) -> f32 {
    let g = anisotropy;
    let numerator = (1.0 - g * g) * (1.0 + cosine * cosine);
    let denominator = (2.0 + g * g) * (1.0 + g * g - 2.0 * g * cosine).powf(1.5);
    (3.0 / (8.0 * PI)) * numerator / denominator
}

fn sphere(origin: Vec3, direction: Vec3, radius: f32) -> Hit {
    let b = origin.dot(direction);
    let c = origin.dot(origin) - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return Hit {
            entry: -1.0,
            exit: -1.0,
        };
    }
    let root = discriminant.sqrt();
    Hit {
        entry: -b - root,
        exit: -b + root,
    }
}

struct Hit {
    entry: f32,
    exit: f32,
}

fn exp(value: Vec3) -> Vec3 {
    Vec3::new(value.x.exp(), value.y.exp(), value.z.exp())
}

fn axis_of(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}

fn set_axis(mut value: Vec3, axis: usize, held: f32) -> Vec3 {
    match axis {
        0 => value.x = held,
        1 => value.y = held,
        _ => value.z = held,
    }
    value
}
