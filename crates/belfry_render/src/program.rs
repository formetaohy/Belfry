use crate::abi::{
    AtlasRecord, BakeLevelRecord, BloomRecord, BloomUpRecord, CascadeRecord, DebugLineRecord,
    DrawRecord, HzbLevelRecord, HzbStepRecord, IRRADIANCE_PARTIALS, InstanceRecord,
    IrradianceRecord, IrradianceSetupRecord, JointRecord, LIGHT_CAPACITY, LIGHT_SLICES,
    LIGHT_STRIDE, LIGHT_TILE, LightRecord, MaterialRecord, MotionRecord, ProbeRecord,
    SHADOW_CASCADES, SkinVertexRecord, SkyRecord, TemporalRecord, TonemapRecord, TransparentRecord,
    TransparentSetupRecord, VERTEX_FLOATS, ViewRecord, WORKGROUP,
};
use belfry_gpu::AtomicU32;
use belfry_gpu::{ComputeProgram, Fvec2, Fvec4, GraphicsProgram, Read, ReadWrite, Uvec3};

pub(crate) const CULL_VIEW: u32 = 0;
pub(crate) const CULL_INSTANCES: u32 = 1;
pub(crate) const CULL_DRAWN: u32 = 2;
pub(crate) const CULL_ARGUMENTS: u32 = 3;
pub(crate) const CULL_LEVELS: u32 = 4;
pub(crate) const CULL_DEPTHS: u32 = 5;

pub(crate) const DEPTH_VIEW: u32 = 0;
pub(crate) const DEPTH_PALETTE: u32 = 1;
pub(crate) const DEPTH_SKINS: u32 = 2;
pub(crate) const DEPTH_DRAWN: u32 = 3;
pub(crate) const DEPTH_VERTICES: u32 = 4;
pub(crate) const DEPTH_MATERIALS: u32 = 5;

pub(crate) const HZB_STEP_BINDING: u32 = 0;
pub(crate) const HZB_SOURCE_BINDING: u32 = 0;
pub(crate) const HZB_SAMPLER_BINDING: u32 = 1;

pub(crate) const DRAW_LIGHT: u32 = 0;
pub(crate) const DRAW_CASCADES: u32 = 1;
pub(crate) const DRAW_DRAWN: u32 = 2;
pub(crate) const DRAW_MATERIALS: u32 = 3;
pub(crate) const DRAW_VIEW: u32 = 4;
pub(crate) const DRAW_PALETTE: u32 = 5;
pub(crate) const DRAW_SKINS: u32 = 6;
pub(crate) const DRAW_FRAGMENTS: u32 = 7;
pub(crate) const DRAW_HEADS: u32 = 8;
pub(crate) const DRAW_COUNTER: u32 = 9;
pub(crate) const DRAW_SETTINGS: u32 = 10;
pub(crate) const DRAW_PROBES: u32 = 11;
pub(crate) const DRAW_VERTICES: u32 = 12;
pub(crate) const DRAW_LIGHTS: u32 = 13;

pub(crate) const PICK_VIEW: u32 = 0;
pub(crate) const PICK_PALETTE: u32 = 1;
pub(crate) const PICK_SKINS: u32 = 2;
pub(crate) const PICK_DRAWN: u32 = 3;
pub(crate) const PICK_VERTICES: u32 = 4;

pub(crate) const CLUSTER_VIEW: u32 = 0;
pub(crate) const CLUSTER_POINTS: u32 = 1;
pub(crate) const CLUSTER_GRID: u32 = 2;

pub(crate) const SHADOW_PALETTE: u32 = 0;
pub(crate) const SHADOW_SKINS: u32 = 1;
pub(crate) const SHADOW_DRAWN: u32 = 2;
pub(crate) const SHADOW_VERTICES: u32 = 3;
pub(crate) const SHADOW_MATERIALS: u32 = 4;

pub(crate) const SHADE_TEXTURE: u32 = 0;
pub(crate) const SHADE_SAMPLER: u32 = 1;
pub(crate) const PROBE_TEXTURES: u32 = 6;
pub(crate) const PROBE_SAMPLERS: u32 = 7;
pub(crate) const SHADE_RADIANCE: u32 = 2;
pub(crate) const SHADE_RADIANCE_SAMPLER: u32 = 3;
pub(crate) const SHADE_BRDF: u32 = 4;
pub(crate) const SHADE_BRDF_SAMPLER: u32 = 5;

pub(crate) const SKY_VIEW: u32 = 0;
pub(crate) const SKY_RADIANCE: u32 = 0;
pub(crate) const SKY_SAMPLER: u32 = 1;

pub(crate) const PREFILTER_LEVEL: u32 = 0;
pub(crate) const PREFILTER_PANORAMA: u32 = 0;
pub(crate) const PREFILTER_SAMPLER: u32 = 1;
pub(crate) const CAPTURE_SETUP: u32 = 0;
pub(crate) const CAPTURE_ATLAS: u32 = 0;
pub(crate) const CAPTURE_SAMPLER: u32 = 1;
pub(crate) const INTEGRATION_SETUP: u32 = 0;
pub(crate) const INTEGRATION_PARTIALS: u32 = 1;
pub(crate) const INTEGRATION_PANORAMA: u32 = 0;
pub(crate) const INTEGRATION_SAMPLER: u32 = 1;
pub(crate) const REDUCTION_PARTIALS: u32 = 0;
pub(crate) const REDUCTION_TOTAL: u32 = 1;

pub(crate) const BLOOM_SETTINGS: u32 = 0;
pub(crate) const EXTRACT_SOURCE: u32 = 0;
pub(crate) const EXTRACT_FILTER: u32 = 1;
pub(crate) const DOWN_SOURCE: u32 = 0;
pub(crate) const DOWN_FILTER: u32 = 1;
pub(crate) const UP_LOW: u32 = 0;
pub(crate) const UP_HIGH: u32 = 1;
pub(crate) const UP_FILTER: u32 = 2;
pub(crate) const TONEMAP_SETTINGS: u32 = 0;
pub(crate) const TONEMAP_LIT: u32 = 0;
pub(crate) const TONEMAP_BLOOM: u32 = 1;
pub(crate) const TONEMAP_FILTER: u32 = 2;

pub(crate) const SHADOW_SHADOW: u32 = 0;
pub(crate) const SHADOW_SAMPLER: u32 = 1;

pub(crate) const RESOLVE_SETTINGS: u32 = 0;
pub(crate) const RESOLVE_FRAGMENTS: u32 = 1;
pub(crate) const RESOLVE_HEADS: u32 = 2;
pub(crate) const RESOLVE_SOURCE: u32 = 0;
pub(crate) const RESOLVE_FILTER: u32 = 1;

pub(crate) const LINE_VIEW: u32 = 0;
pub(crate) const LINE_SEGMENTS: u32 = 1;

pub(crate) const MOTION_VIEW: u32 = 0;
pub(crate) const MOTION_DRAWN: u32 = 1;
pub(crate) const MOTION_PALETTE: u32 = 2;
pub(crate) const MOTION_SKINS: u32 = 3;
pub(crate) const MOTION_MATERIALS: u32 = 4;
pub(crate) const MOTION_PREVIOUS_PALETTE: u32 = 5;
pub(crate) const MOTION_VERTICES: u32 = 6;
pub(crate) const MOTION_TEXTURE: u32 = 0;
pub(crate) const MOTION_SAMPLER: u32 = 1;

pub(crate) const TEMPORAL_FRAME: u32 = 0;
pub(crate) const TEMPORAL_SOURCE: u32 = 0;
pub(crate) const TEMPORAL_HISTORY: u32 = 1;
pub(crate) const TEMPORAL_FILTER: u32 = 2;
pub(crate) const TEMPORAL_MOTION: u32 = 0;
pub(crate) const TEMPORAL_PREVIOUS: u32 = 1;
pub(crate) const TEMPORAL_NEAREST: u32 = 2;

const _: () = assert!(
    WORKGROUP == 64,
    "the culling kernel runs the workgroup its attribute declares",
);
const _: () = assert!(
    VERTEX_FLOATS == 8,
    "the instancing stage reads the vertex layout of a render vertex",
);
const _: () = assert!(
    SHADOW_CASCADES == 4,
    "the shadow receiver selects one of four cascades",
);
const _: () = assert!(
    crate::abi::HZB_SCALE == 4,
    "the base reduction samples the block the scale names",
);
const _: () = assert!(
    LIGHT_STRIDE == LIGHT_CAPACITY + 1,
    "a light cluster holds its count beside every light it names",
);
const _: () = assert!(
    LIGHT_STRIDE == 33 && LIGHT_TILE == 64 && LIGHT_SLICES == 24 && LIGHT_CAPACITY == 32,
    "the clustered lighting program walks the tile, slice and light counts its records declare",
);
const _: () = assert!(
    LIGHT_TILE > 0 && LIGHT_SLICES > 0,
    "a light cluster covers a tile of at least one pixel and one depth slice",
);
const _: () = assert!(
    IRRADIANCE_PARTIALS == 1024 && crate::abi::IRRADIANCE_WORDS == 36,
    "the irradiance reduction walks the partials and record words its shader counts",
);
const _: () = assert!(
    crate::environment::SPECULAR_SAMPLES == 128,
    "the prefilter samples the half vectors its shader counts",
);

#[belfry_gpu::kernel(workgroup_size = 64)]
mod cull {
    #[kernel]
    fn cull(
        global: Uvec3,
        view: Read<ViewRecord>,
        instances: Read<InstanceRecord>,
        drawn: ReadWrite<DrawRecord>,
        arguments: ReadWrite<AtomicU32>,
        levels: Read<HzbLevelRecord>,
        depths: Read<f32>,
    ) {
        let index = global.x;
        let held = view[0u32];
        if index < held.instances {
            let instance = instances[index];
            let bounds = instance.bounds;
            if inside(held.plane0, bounds)
                && inside(held.plane1, bounds)
                && inside(held.plane2, bounds)
                && inside(held.plane3, bounds)
                && inside(held.plane4, bounds)
                && inside(held.plane5, bounds)
            {
                let mut occluded = false;
                if held.hzb_levels > 0u32 && instance.hzb > 0u32 {
                    let relative = fvec4(
                        bounds.x - held.eye.x,
                        bounds.y - held.eye.y,
                        bounds.z - held.eye.z,
                        0.0,
                    );
                    let reach = relative.x * held.forward.x
                        + relative.y * held.forward.y
                        + relative.z * held.forward.z;
                    let nearest = reach - bounds.w;
                    if nearest > held.near && reach < held.far {
                        let center = fvec4(bounds.x, bounds.y, bounds.z, 1.0);
                        let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                        if clip.w > 0.0 {
                            let inverse = 1.0 / clip.w;
                            let middle = fvec2(
                                clip.x * inverse * 0.5 + 0.5,
                                clip.y * inverse * held.vertical + 0.5,
                            );
                            let half_y = bounds.w / (nearest * held.tan_vertical) * 0.5;
                            let half_x = half_y / held.aspect;
                            let pad = fvec2(2.0 / held.view_width, 2.0 / held.view_height);
                            let low = fvec2(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                            let high = fvec2(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                            if low.x >= 0.0 && high.x <= 1.0 && low.y >= 0.0 && high.y <= 1.0 {
                                let base = levels[0u32];
                                let width = base.width as f32;
                                let height = base.height as f32;
                                let low_x = low.x * width;
                                let low_y = low.y * height;
                                let high_x = high.x * width;
                                let high_y = high.y * height;
                                let mut level = 0u32;
                                let mut factor = 1.0;
                                let mut extent = max(high_x - low_x, high_y - low_y);
                                while extent > 4.0 && level + 1u32 < held.hzb_levels {
                                    extent *= 0.5;
                                    factor *= 2.0;
                                    level += 1u32;
                                }
                                let chosen = levels[level];
                                let chosen_width = chosen.width as f32;
                                let chosen_height = chosen.height as f32;
                                let first_x = max(floor(low_x / factor), 0.0) as u32;
                                let last_x = min(floor(high_x / factor), chosen_width - 1.0) as u32;
                                let first_y = max(floor(low_y / factor), 0.0) as u32;
                                let last_y =
                                    min(floor(high_y / factor), chosen_height - 1.0) as u32;
                                let mut deepest = 0.0;
                                for y in stride(first_y, last_y + 1u32, 1u32) {
                                    for x in stride(first_x, last_x + 1u32, 1u32) {
                                        deepest = max(
                                            deepest,
                                            depths[chosen.offset + y * chosen.stride + x],
                                        );
                                    }
                                }
                                occluded = deepest < (nearest - held.near) / (held.far - held.near);
                            }
                        }
                    }
                }
                if !occluded {
                    let batch = instance.batch;
                    let slot = atomic_add(&arguments[batch * 4u32 + 1u32], 1u32);
                    let at = instance.base + slot;
                    drawn[at].clip0 = column(
                        held.clip0,
                        held.clip1,
                        held.clip2,
                        held.clip3,
                        instance.model0,
                    );
                    drawn[at].clip1 = column(
                        held.clip0,
                        held.clip1,
                        held.clip2,
                        held.clip3,
                        instance.model1,
                    );
                    drawn[at].clip2 = column(
                        held.clip0,
                        held.clip1,
                        held.clip2,
                        held.clip3,
                        instance.model2,
                    );
                    drawn[at].clip3 = column(
                        held.clip0,
                        held.clip1,
                        held.clip2,
                        held.clip3,
                        instance.model3,
                    );
                    drawn[at].model0 = instance.model0;
                    drawn[at].model1 = instance.model1;
                    drawn[at].model2 = instance.model2;
                    drawn[at].model3 = instance.model3;
                    drawn[at].previous0 = instance.previous0;
                    drawn[at].previous1 = instance.previous1;
                    drawn[at].previous2 = instance.previous2;
                    drawn[at].previous3 = instance.previous3;
                    drawn[at].normal0 = instance.normal0;
                    drawn[at].normal1 = instance.normal1;
                    drawn[at].normal2 = instance.normal2;
                    drawn[at].tint = instance.tint;
                    drawn[at].material = instance.material;
                    drawn[at].pick = index;
                    drawn[at].palette = instance.palette;
                    drawn[at].joints = instance.joints;
                    drawn[at].previous_palette = instance.previous_palette;
                }
            }
        }
    }

    fn inside(plane: Fvec4, sphere: Fvec4) -> bool {
        plane.x * sphere.x + plane.y * sphere.y + plane.z * sphere.z + plane.w > -sphere.w
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}

#[belfry_gpu::kernel(workgroup_size = 64)]
mod cluster {
    #[kernel]
    fn cluster(
        group: Uvec3,
        lid: u32,
        view: Read<ViewRecord>,
        points: Read<u32>,
        clusters: ReadWrite<AtomicU32>,
    ) {
        let held = view[0u32];
        let tiles = held.cluster_x * held.cluster_y;
        if tiles == 0u32 || held.point_lights == 0u32 {
            return;
        }
        let tile = group.x;
        let slice = group.y;
        if tile >= tiles || slice >= 24u32 {
            return;
        }
        let tile_x = tile % held.cluster_x;
        let tile_y = tile / held.cluster_x;
        let index = (slice * held.cluster_y + tile_y) * held.cluster_x + tile_x;
        let cluster_base = index * 33u32;
        let low_x = tile_x as f32 * 64.0;
        let high_x = low_x + 64.0;
        let low_y = tile_y as f32 * 64.0;
        let high_y = low_y + 64.0;
        let ratio = held.far / held.near;
        let slice_near = held.near * pow(ratio, slice as f32 / 24.0);
        let slice_far = held.near * pow(ratio, (slice + 1u32) as f32 / 24.0);
        for light in stride(lid, held.point_lights, 64u32) {
            let at = light * 8u32;
            let point_x = bitcast_f32(points[at]);
            let point_y = bitcast_f32(points[at + 1u32]);
            let point_z = bitcast_f32(points[at + 2u32]);
            let point_range = bitcast_f32(points[at + 3u32]);
            let relative = fvec4(
                point_x - held.eye.x,
                point_y - held.eye.y,
                point_z - held.eye.z,
                0.0,
            );
            let reach = relative.x * held.forward.x
                + relative.y * held.forward.y
                + relative.z * held.forward.z;
            let nearest = reach - point_range;
            let deepest = reach + point_range;
            if deepest >= slice_near && nearest <= slice_far {
                let mut covers = nearest <= held.near;
                if !covers {
                    let center = fvec4(point_x, point_y, point_z, 1.0);
                    let clip = column(held.clip0, held.clip1, held.clip2, held.clip3, center);
                    if clip.w > 0.0 {
                        let inverse = 1.0 / clip.w;
                        let middle = fvec2(
                            clip.x * inverse * 0.5 + 0.5,
                            clip.y * inverse * held.vertical + 0.5,
                        );
                        let half_y = point_range / (nearest * held.tan_vertical) * 0.5;
                        let half_x = half_y / held.aspect;
                        let pad = fvec2(1.0 / held.view_width, 1.0 / held.view_height);
                        let low = fvec2(middle.x - half_x - pad.x, middle.y - half_y - pad.y);
                        let high = fvec2(middle.x + half_x + pad.x, middle.y + half_y + pad.y);
                        covers = high.x * held.view_width >= low_x
                            && low.x * held.view_width <= high_x
                            && high.y * held.view_height >= low_y
                            && low.y * held.view_height <= high_y;
                    } else {
                        covers = true;
                    }
                }
                if covers {
                    let slot = atomic_add(&clusters[cluster_base], 1u32);
                    if slot < 32u32 {
                        atomic_store(&clusters[cluster_base + 1u32 + slot], light);
                    }
                }
            }
        }
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}

#[belfry_gpu::graphics]
mod instanced {
    #[binding]
    fn light(light: Read<LightRecord>) {}

    #[binding]
    fn cascade(cascades: Read<CascadeRecord>) {}

    #[binding]
    fn drawn(drawn: Read<DrawRecord>) {}

    #[binding]
    fn materials(materials: Read<MaterialRecord>) {}

    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn shade(
        shadows: Texture2D,
        shadower: Sampler,
        radiance: Texture2D,
        radiance_sampler: Sampler,
        brdf: Texture2D,
        brdf_sampler: Sampler,
    ) {
    }

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    #[binding]
    fn transparent(
        fragments: ReadWrite<TransparentRecord>,
        heads: ReadWrite<AtomicU32>,
        counter: ReadWrite<AtomicU32>,
        settings: Read<TransparentSetupRecord>,
    ) {
    }

    #[binding]
    fn probe(
        probes: Read<ProbeRecord>,
        probe_textures: TextureTable,
        probe_samplers: SamplerTable,
    ) {
    }

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        vertices: Read<f32>,
        position: Out<Position>,
        placed: Out<Fvec4>,
        oriented: Out<Fvec4>,
        tinted: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let resting = held.normal0 * vertices[base + 3u32]
            + held.normal1 * vertices[base + 4u32]
            + held.normal2 * vertices[base + 5u32];
        let material = held.material as f32;
        tinted = fvec4(
            held.tint.x,
            held.tint.y,
            held.tint.z,
            select(material, -(material + 1.0), held.pick == camera.highlight),
        );
        if held.joints > 0u32 {
            let skin = skins[vertex];
            let first = palette[held.palette + skin.joints.x];
            let second = palette[held.palette + skin.joints.y];
            let third = palette[held.palette + skin.joints.z];
            let fourth = palette[held.palette + skin.joints.w];
            let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                    * skin.weights.w;
            let normal = deformed(first.col0, first.col1, first.col2, first.col3, resting)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, resting)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, resting)
                    * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, resting)
                    * skin.weights.w;
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            let projected = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = projected;
            placed = fvec4(moved.x, moved.y, moved.z, vertices[base + 6u32]);
            oriented = fvec4(normal.x, normal.y, normal.z, vertices[base + 7u32]);
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            let projected = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = projected;
            placed = fvec4(moved.x, moved.y, moved.z, vertices[base + 6u32]);
            oriented = fvec4(resting.x, resting.y, resting.z, vertices[base + 7u32]);
        }
    }

    #[fragment]
    fn fragment(
        textures: TextureTable,
        samplers: SamplerTable,
        lights: Read<u32>,
        world: In<Fvec4>,
        surface: In<Fvec4>,
        shaded: In<Fvec4>,
        target: Out<Fvec4>,
    ) {
        let position = world;
        let direction = surface;
        let tint = shaded;
        let stored = tint.w;
        let marked = stored < 0.0;
        let material = (abs(stored) + 0.5) as u32 - select(0u32, 1u32, marked);
        let camera = view[0u32];
        let projected = camera.clip0 * position.x
            + camera.clip1 * position.y
            + camera.clip2 * position.z
            + camera.clip3;
        let screen = fvec2(
            projected.x / projected.w * 0.5 + 0.5,
            projected.y / projected.w * camera.vertical + 0.5,
        );
        let reach = (position.x - camera.eye.x) * camera.forward.x
            + (position.y - camera.eye.y) * camera.forward.y
            + (position.z - camera.eye.z) * camera.forward.z;
        let entry = materials[material];
        let lamp = light[0u32];
        let unit = normalized(direction);
        let eye_direction = normalized(fvec4(
            camera.eye.x - position.x,
            camera.eye.y - position.y,
            camera.eye.z - position.z,
            0.0,
        ));
        let uv = fvec2(position.w, direction.w);
        let sampled = sample(
            textures[entry.base_color_image],
            samplers[entry.base_color_sampler],
            uv,
        );
        let base = fvec4(
            sampled.x * tint.x * entry.base_color.x,
            sampled.y * tint.y * entry.base_color.y,
            sampled.z * tint.z * entry.base_color.z,
            sampled.w * entry.base_color.w,
        );
        let coverage = clamped(base.w, 0.0, 1.0);
        let properties = sample(
            textures[entry.metallic_roughness_image],
            samplers[entry.metallic_roughness_sampler],
            uv,
        );
        let metallic = clamped(entry.metallic * properties.z, 0.0, 1.0);
        let roughness = clamped(entry.roughness * properties.y, 0.045, 1.0);
        let position_dx = dpdx(position);
        let position_dy = dpdy(position);
        let surface_dx = dpdx(surface);
        let surface_dy = dpdy(surface);
        let uv_dx = fvec2(position_dx.w, surface_dx.w);
        let uv_dy = fvec2(position_dy.w, surface_dy.w);
        let perpendicular_y = cross3(
            fvec4(position_dy.x, position_dy.y, position_dy.z, 0.0),
            unit,
        );
        let perpendicular_x = cross3(
            unit,
            fvec4(position_dx.x, position_dx.y, position_dx.z, 0.0),
        );
        let tangent = normalized(perpendicular_y * uv_dx.x + perpendicular_x * uv_dy.x);
        let bitangent = normalized(perpendicular_y * uv_dx.y + perpendicular_x * uv_dy.y);
        let mapped = sample(
            textures[entry.normal_image],
            samplers[entry.normal_sampler],
            uv,
        );
        let scaled = fvec4(
            (mapped.x * 2.0 - 1.0) * entry.normal_scale,
            (mapped.y * 2.0 - 1.0) * entry.normal_scale,
            mapped.z * 2.0 - 1.0,
            0.0,
        );
        let normal = normalized(tangent * scaled.x + bitangent * scaled.y + unit * scaled.z);
        if entry.alpha_mode == 1u32 && coverage < entry.alpha_cutoff {
            discard();
        }
        let albedo = fvec4(
            base.x * (1.0 - metallic),
            base.y * (1.0 - metallic),
            base.z * (1.0 - metallic),
            1.0,
        );
        let f0 = fvec4(
            0.04 + (base.x - 0.04) * metallic,
            0.04 + (base.y - 0.04) * metallic,
            0.04 + (base.z - 0.04) * metallic,
            1.0,
        );
        let travel = lamp.direction;
        let sun_direction = fvec4(-travel.x, -travel.y, -travel.z, 0.0);
        let incidence = max(dot3(normal, sun_direction), 0.0);
        let mut cascade = 0u32;
        if reach > cascades[0u32].split {
            cascade = 1u32;
        }
        if reach > cascades[1u32].split {
            cascade = 2u32;
        }
        if reach > cascades[2u32].split {
            cascade = 3u32;
        }
        let shadow = cascades[cascade];
        let offset = shadow.texel * 2.0;
        let offset_position = fvec4(
            position.x + unit.x * offset,
            position.y + unit.y * offset,
            position.z + unit.z * offset,
            1.0,
        );
        let placed = shadow.clip0 * offset_position.x
            + shadow.clip1 * offset_position.y
            + shadow.clip2 * offset_position.z
            + shadow.clip3 * offset_position.w;
        let inverse = 1.0 / placed.w;
        let local = fvec2(
            placed.x * inverse * 0.5 + 0.5,
            placed.y * inverse * lamp.vertical + 0.5,
        );
        let atlas = shadow.atlas;
        let low = fvec2(atlas.x + lamp.texel * 0.5, atlas.y + lamp.texel * 0.5);
        let high = fvec2(
            atlas.x + atlas.z - lamp.texel * 0.5,
            atlas.y + atlas.z - lamp.texel * 0.5,
        );
        let at = fvec2(atlas.x + local.x * atlas.z, atlas.y + local.y * atlas.z);
        let slope = sqrt(max(0.0, 1.0 - incidence * incidence)) / max(incidence, 0.1);
        let depth = placed.z * inverse - 0.0005 - shadow.texel / shadow.depth_span * slope;
        let sampled_shadow = shadowed(shadows, shadower, at, depth, lamp.texel, low, high);
        let beyond = reach > cascades[3u32].split;
        let visibility = select(sampled_shadow, 1.0, beyond);
        let direct = pbr(
            normal,
            eye_direction,
            sun_direction,
            base,
            metallic,
            roughness,
        );
        let sun = fvec4(
            direct.x * lamp.radiance.x * incidence * visibility,
            direct.y * lamp.radiance.y * incidence * visibility,
            direct.z * lamp.radiance.z * incidence * visibility,
            0.0,
        );
        let occlusion = sample(
            textures[entry.occlusion_image],
            samplers[entry.occlusion_sampler],
            uv,
        );
        let openness = 1.0 + entry.occlusion_strength * (occlusion.x - 1.0);
        let view_dot = dot3(normal, eye_direction);
        let reflected = normalized(fvec4(
            normal.x * 2.0 * view_dot - eye_direction.x,
            normal.y * 2.0 * view_dot - eye_direction.y,
            normal.z * 2.0 * view_dot - eye_direction.z,
            0.0,
        ));
        let environment_normal = turned(
            lamp.environment0,
            lamp.environment1,
            lamp.environment2,
            normal,
        );
        let environment_reflected = turned(
            lamp.environment0,
            lamp.environment1,
            lamp.environment2,
            reflected,
        );
        let prefiltered = sample_lod(
            radiance,
            radiance_sampler,
            equirect(environment_reflected),
            roughness * lamp.levels,
        );
        let split = sample(brdf, brdf_sampler, fvec2(max(view_dot, 0.0001), roughness));
        let environment_specular = fvec4(
            prefiltered.x * split_sum(f0.x, split),
            prefiltered.y * split_sum(f0.y, split),
            prefiltered.z * split_sum(f0.z, split),
            0.0,
        );
        let environment_diffuse = (irradiance_low(
            lamp.irradiance0,
            lamp.irradiance1,
            lamp.irradiance2,
            lamp.irradiance3,
            environment_normal,
        ) + irradiance_high(
            lamp.irradiance4,
            lamp.irradiance5,
            lamp.irradiance6,
            lamp.irradiance7,
            lamp.irradiance8,
            environment_normal,
        )) * albedo
            * (113.0 / 355.0);
        let global_ambient =
            (environment_diffuse + environment_specular) * (lamp.intensity * openness);
        let mut local_red = 0.0;
        let mut local_green = 0.0;
        let mut local_blue = 0.0;
        let mut local_weight = 0.0;
        let mut local_coverage = 0.0;
        for index in stride(0u32, lamp.probes, 1u32) {
            let held = probes[index];
            let relative = fvec4(
                position.x - held.center.x,
                position.y - held.center.y,
                position.z - held.center.z,
                0.0,
            );
            let local = turned(held.inverse0, held.inverse1, held.inverse2, relative);
            let outside_x = max(abs(local.x) - held.extent.x, 0.0);
            let outside_y = max(abs(local.y) - held.extent.y, 0.0);
            let outside_z = max(abs(local.z) - held.extent.z, 0.0);
            let outside = max(outside_x, max(outside_y, outside_z));
            let fade = clamped(1.0 - outside / max(held.center.w, 0.00001), 0.0, 1.0);
            if fade > 0.0 {
                let local_normal =
                    normalized(turned(held.inverse0, held.inverse1, held.inverse2, normal));
                let local_reflected = normalized(turned(
                    held.inverse0,
                    held.inverse1,
                    held.inverse2,
                    reflected,
                ));
                let corrected = box_projected(local, local_reflected, held.extent);
                let held_radiance = sample_lod(
                    probe_textures[held.texture],
                    probe_samplers[held.sampler],
                    equirect(corrected),
                    roughness * held.levels,
                );
                let held_diffuse = (irradiance_low(
                    held.irradiance0,
                    held.irradiance1,
                    held.irradiance2,
                    held.irradiance3,
                    local_normal,
                ) + irradiance_high(
                    held.irradiance4,
                    held.irradiance5,
                    held.irradiance6,
                    held.irradiance7,
                    held.irradiance8,
                    local_normal,
                )) * albedo
                    * (113.0 / 355.0);
                let held_specular = fvec4(
                    held_radiance.x * split_sum(f0.x, split),
                    held_radiance.y * split_sum(f0.y, split),
                    held_radiance.z * split_sum(f0.z, split),
                    0.0,
                );
                let rank = 1.0 / max(held.extent.x, max(held.extent.y, held.extent.z));
                let scale = held.extent.w * openness * fade * rank;
                local_red += (held_diffuse.x + held_specular.x) * scale;
                local_green += (held_diffuse.y + held_specular.y) * scale;
                local_blue += (held_diffuse.z + held_specular.z) * scale;
                local_weight += fade * rank;
                local_coverage = max(local_coverage, fade);
            }
        }
        let local_scale = local_coverage / max(local_weight, 0.00001);
        let global_scale = max(1.0 - local_coverage, 0.0);
        let ambient = fvec4(
            global_ambient.x * global_scale + local_red * local_scale,
            global_ambient.y * global_scale + local_green * local_scale,
            global_ambient.z * global_scale + local_blue * local_scale,
            0.0,
        );
        let glow = sample(
            textures[entry.emissive_image],
            samplers[entry.emissive_sampler],
            uv,
        );
        let emissive = fvec4(
            glow.x * entry.emissive.x,
            glow.y * entry.emissive.y,
            glow.z * entry.emissive.z,
            0.0,
        );
        let mut emitted_x = 0.0;
        let mut emitted_y = 0.0;
        let mut emitted_z = 0.0;
        if camera.point_lights > 0u32
            && camera.cluster_x > 0u32
            && camera.cluster_y > 0u32
            && camera.far > camera.near
        {
            let tile_x = min(
                floor(screen.x * camera.view_width / 64.0),
                (camera.cluster_x - 1u32) as f32,
            ) as u32;
            let tile_y = min(
                floor(screen.y * camera.view_height / 64.0),
                (camera.cluster_y - 1u32) as f32,
            ) as u32;
            let ratio = log(max(reach, camera.near) / camera.near) / log(camera.far / camera.near);
            let mut slice = floor(ratio * 24.0) as u32;
            if slice >= 24u32 {
                slice = 23u32;
            }
            let clustered = (slice * camera.cluster_y + tile_y) * camera.cluster_x + tile_x;
            let cluster_base = camera.cluster_offset + clustered * 33u32;
            let mut count = lights[cluster_base];
            if count > 32u32 {
                count = 32u32;
            }
            for slot in stride(0u32, count, 1u32) {
                let light = lights[cluster_base + 1u32 + slot];
                let at = light * 8u32;
                let point_x = bitcast_f32(lights[at]);
                let point_y = bitcast_f32(lights[at + 1u32]);
                let point_z = bitcast_f32(lights[at + 2u32]);
                let point_range = bitcast_f32(lights[at + 3u32]);
                let radiance = fvec4(
                    bitcast_f32(lights[at + 4u32]),
                    bitcast_f32(lights[at + 5u32]),
                    bitcast_f32(lights[at + 6u32]),
                    0.0,
                );
                let delta = fvec4(
                    point_x - position.x,
                    point_y - position.y,
                    point_z - position.z,
                    0.0,
                );
                let distance2 = dot3(delta, delta);
                let distance = sqrt(distance2);
                let direction = delta * (1.0 / max(distance, 0.0001));
                let n_dot_l = max(dot3(normal, direction), 0.0);
                let falloff = max(0.0, 1.0 - pow(distance / point_range, 4.0));
                let attenuation = falloff * falloff / max(distance2, 0.0001);
                let term = pbr(normal, eye_direction, direction, base, metallic, roughness);
                emitted_x += term.x * radiance.x * n_dot_l * attenuation;
                emitted_y += term.y * radiance.y * n_dot_l * attenuation;
                emitted_z += term.z * radiance.z * n_dot_l * attenuation;
            }
        }
        let lit = fvec4(
            sun.x + emitted_x + ambient.x + emissive.x,
            sun.y + emitted_y + ambient.y + emissive.y,
            sun.z + emitted_z + ambient.z + emissive.z,
            coverage,
        );
        let marker = select(0.0, 1.0, marked);
        let blended = entry.alpha_mode == 2u32;
        if blended {
            let setup = settings[0u32];
            let index = atomic_add(&counter[0u32], 1u32);
            if index < setup.capacity && coverage > 0.0 {
                let width = setup.width as f32;
                let height = setup.height as f32;
                let pixel_x = min(max(floor(screen.x * width), 0.0), width - 1.0) as u32;
                let pixel_y = min(max(floor(screen.y * height), 0.0), height - 1.0) as u32;
                let pixel = pixel_y * setup.width + pixel_x;
                let previous = atomic_exchange(&heads[pixel], index + 1u32);
                fragments[index].red = lit.x * coverage;
                fragments[index].green = lit.y * coverage;
                fragments[index].blue = lit.z * coverage;
                fragments[index].alpha = coverage;
                fragments[index].depth = reach;
                fragments[index].next = previous;
            }
        }
        target = fvec4(
            select(lit.x, lit.x * coverage, blended),
            select(lit.y, lit.y * coverage, blended),
            select(lit.z, lit.z * coverage, blended),
            select(marker, coverage, blended),
        );
    }

    fn irradiance_low(c0: Fvec4, c1: Fvec4, c2: Fvec4, c3: Fvec4, normal: Fvec4) -> Fvec4 {
        c0 * 0.282095
            + c1 * (0.488603 * normal.y)
            + c2 * (0.488603 * normal.z)
            + c3 * (0.488603 * normal.x)
    }

    fn turned(first: Fvec4, second: Fvec4, third: Fvec4, vector: Fvec4) -> Fvec4 {
        fvec4(
            first.x * vector.x + second.x * vector.y + third.x * vector.z,
            first.y * vector.x + second.y * vector.y + third.y * vector.z,
            first.z * vector.x + second.z * vector.y + third.z * vector.z,
            0.0,
        )
    }

    fn irradiance_high(
        c4: Fvec4,
        c5: Fvec4,
        c6: Fvec4,
        c7: Fvec4,
        c8: Fvec4,
        normal: Fvec4,
    ) -> Fvec4 {
        let x = normal.x;
        let y = normal.y;
        let z = normal.z;
        c4 * (1.092548 * x * y)
            + c5 * (1.092548 * y * z)
            + c6 * (0.315392 * (3.0 * z * z - 1.0))
            + c7 * (1.092548 * x * z)
            + c8 * (0.546274 * (x * x - y * y))
    }

    fn split_sum(f0: f32, split: Fvec4) -> f32 {
        let first = f0 * split.x + split.y;
        let energy = 1.0 - (split.x + split.y);
        let average = f0 + (1.0 - f0) * 0.047619;
        first + first * energy * average / (1.0 - energy * average)
    }

    fn box_projected(position: Fvec4, direction: Fvec4, extent: Fvec4) -> Fvec4 {
        let mut nearest = 1.0e30;
        let mut hit = true;
        if abs(direction.x) > 0.00001 {
            let bound = select(extent.x, -extent.x, direction.x < 0.0);
            let travel = (bound - position.x) / direction.x;
            if travel > 0.0 {
                nearest = min(nearest, travel);
            }
        } else if abs(position.x) > extent.x {
            hit = false;
        }
        if abs(direction.y) > 0.00001 {
            let bound = select(extent.y, -extent.y, direction.y < 0.0);
            let travel = (bound - position.y) / direction.y;
            if travel > 0.0 {
                nearest = min(nearest, travel);
            }
        } else if abs(position.y) > extent.y {
            hit = false;
        }
        if abs(direction.z) > 0.00001 {
            let bound = select(extent.z, -extent.z, direction.z < 0.0);
            let travel = (bound - position.z) / direction.z;
            if travel > 0.0 {
                nearest = min(nearest, travel);
            }
        } else if abs(position.z) > extent.z {
            hit = false;
        }
        if hit && nearest < 1.0e29 {
            return normalized(fvec4(
                position.x + direction.x * nearest,
                position.y + direction.y * nearest,
                position.z + direction.z * nearest,
                0.0,
            ));
        }
        direction
    }

    fn equirect(direction: Fvec4) -> Fvec2 {
        let unit = normalized(direction);
        fvec2(
            atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
            acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
        )
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }

    fn cross3(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            left.y * right.z - left.z * right.y,
            left.z * right.x - left.x * right.z,
            left.x * right.y - left.y * right.x,
            0.0,
        )
    }

    fn pbr(
        normal: Fvec4,
        view: Fvec4,
        light: Fvec4,
        base: Fvec4,
        metallic: f32,
        roughness: f32,
    ) -> Fvec4 {
        let pi = 355.0 / 113.0;
        let n_dot_v = max(dot3(normal, view), 0.0001);
        let n_dot_l = max(dot3(normal, light), 0.0);
        let albedo = fvec4(
            base.x * (1.0 - metallic),
            base.y * (1.0 - metallic),
            base.z * (1.0 - metallic),
            1.0,
        );
        let f0 = fvec4(
            0.04 + (base.x - 0.04) * metallic,
            0.04 + (base.y - 0.04) * metallic,
            0.04 + (base.z - 0.04) * metallic,
            1.0,
        );
        let half = normalized(fvec4(
            view.x + light.x,
            view.y + light.y,
            view.z + light.z,
            0.0,
        ));
        let n_dot_h = max(dot3(normal, half), 0.0);
        let v_dot_h = max(dot3(view, half), 0.0);
        let alpha = max(roughness * roughness, 0.0025);
        let alpha2 = alpha * alpha;
        let denominator = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
        let distribution = alpha2 / (pi * denominator * denominator);
        let k = (roughness + 1.0) * (roughness + 1.0) * 0.125;
        let geometry =
            n_dot_v * n_dot_l / ((n_dot_v * (1.0 - k) + k) * (n_dot_l * (1.0 - k) + k) + 0.0001);
        let fresnel_power = pow(1.0 - v_dot_h, 5.0);
        let fresnel = fvec4(
            f0.x + (1.0 - f0.x) * fresnel_power,
            f0.y + (1.0 - f0.y) * fresnel_power,
            f0.z + (1.0 - f0.z) * fresnel_power,
            0.0,
        );
        let specular = fvec4(
            distribution * geometry * fresnel.x / (4.0 * n_dot_v * n_dot_l + 0.0001),
            distribution * geometry * fresnel.y / (4.0 * n_dot_v * n_dot_l + 0.0001),
            distribution * geometry * fresnel.z / (4.0 * n_dot_v * n_dot_l + 0.0001),
            0.0,
        );
        let diffuse = fvec4(
            (1.0 - fresnel.x) * albedo.x / pi,
            (1.0 - fresnel.y) * albedo.y / pi,
            (1.0 - fresnel.z) * albedo.z / pi,
            0.0,
        );
        fvec4(
            diffuse.x + specular.x,
            diffuse.y + specular.y,
            diffuse.z + specular.z,
            0.0,
        )
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn shadowed(
        shadows: Texture2D,
        shadower: Sampler,
        at: Fvec2,
        depth: f32,
        texel: f32,
        low: Fvec2,
        high: Fvec2,
    ) -> f32 {
        let left = clamped(at.x - texel, low.x, high.x);
        let right = clamped(at.x + texel, low.x, high.x);
        let down = clamped(at.y - texel, low.y, high.y);
        let up = clamped(at.y + texel, low.y, high.y);
        let x = clamped(at.x, low.x, high.x);
        let y = clamped(at.y, low.y, high.y);
        let total = compared(shadows, shadower, fvec2(left, down), depth)
            + compared(shadows, shadower, fvec2(x, down), depth)
            + compared(shadows, shadower, fvec2(right, down), depth)
            + compared(shadows, shadower, fvec2(left, y), depth)
            + compared(shadows, shadower, fvec2(x, y), depth)
            + compared(shadows, shadower, fvec2(right, y), depth)
            + compared(shadows, shadower, fvec2(left, up), depth)
            + compared(shadows, shadower, fvec2(x, up), depth)
            + compared(shadows, shadower, fvec2(right, up), depth);
        total * (1.0 / 9.0)
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }

    fn compared(shadows: Texture2D, shadower: Sampler, at: Fvec2, depth: f32) -> f32 {
        let stored = sample(shadows, shadower, at);
        if stored.x < depth { 0.0 } else { 1.0 }
    }
}

#[belfry_gpu::graphics]
mod casting {
    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        clip: Out<Fvec4>,
        surface: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let shaped = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            0.0,
        );
        if held.joints > 0u32 {
            let skin = skins[vertex];
            let first = palette[held.palette + skin.joints.x];
            let second = palette[held.palette + skin.joints.y];
            let third = palette[held.palette + skin.joints.z];
            let fourth = palette[held.palette + skin.joints.w];
            let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                    * skin.weights.w;
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = moved;
            clip = moved;
        } else {
            let moved = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = moved;
            clip = moved;
        }
        surface = shaped;
    }

    #[fragment]
    fn fragment(
        space: In<Fvec4>,
        shaped: In<Fvec4>,
        materials: Read<MaterialRecord>,
        textures: TextureTable,
        samplers: SamplerTable,
        target: Out<Fvec4>,
    ) {
        let placed = space;
        let shape = shaped;
        let material = shape.z as u32;
        let entry = materials[material];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shape.x, shape.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        target = fvec4(placed.z / placed.w, 0.0, 0.0, 1.0);
    }
}

#[belfry_gpu::graphics]
mod selection {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        picked: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let id = held.pick;
        picked = fvec4(
            (id % 256u32) as f32 / 255.0,
            ((id / 256u32) % 256u32) as f32 / 255.0,
            ((id / 65536u32) % 256u32) as f32 / 255.0,
            1.0,
        );
        if held.joints > 0u32 {
            let skin = skins[vertex];
            let first = palette[held.palette + skin.joints.x];
            let second = palette[held.palette + skin.joints.y];
            let third = palette[held.palette + skin.joints.z];
            let fourth = palette[held.palette + skin.joints.w];
            let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                    * skin.weights.w;
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            position = camera.clip0 * moved.x
                + camera.clip1 * moved.y
                + camera.clip2 * moved.z
                + camera.clip3;
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            position = camera.clip0 * moved.x
                + camera.clip1 * moved.y
                + camera.clip2 * moved.z
                + camera.clip3;
        }
    }

    #[fragment]
    fn fragment(space: In<Fvec4>, target: Out<Fvec4>) {
        target = space;
    }
}

#[belfry_gpu::graphics]
mod depth {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        drawn: Read<DrawRecord>,
        vertices: Read<f32>,
        position: Out<Position>,
        world: Out<Fvec4>,
        surface: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let shaped = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            0.0,
        );
        if held.joints > 0u32 {
            let skin = skins[vertex];
            let first = palette[held.palette + skin.joints.x];
            let second = palette[held.palette + skin.joints.y];
            let third = palette[held.palette + skin.joints.z];
            let fourth = palette[held.palette + skin.joints.w];
            let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                    * skin.weights.w;
            let local = fvec4(place.x, place.y, place.z, 1.0);
            let moved = held.model0 * local.x
                + held.model1 * local.y
                + held.model2 * local.z
                + held.model3 * local.w;
            let projected = held.clip0 * local.x
                + held.clip1 * local.y
                + held.clip2 * local.z
                + held.clip3 * local.w;
            position = projected;
            world = moved;
        } else {
            let moved = held.model0 * rest.x
                + held.model1 * rest.y
                + held.model2 * rest.z
                + held.model3 * rest.w;
            let projected = held.clip0 * rest.x
                + held.clip1 * rest.y
                + held.clip2 * rest.z
                + held.clip3 * rest.w;
            position = projected;
            world = moved;
        }
        surface = shaped;
    }

    #[fragment]
    fn fragment(
        space: In<Fvec4>,
        shaped: In<Fvec4>,
        materials: Read<MaterialRecord>,
        textures: TextureTable,
        samplers: SamplerTable,
        target: Out<Fvec4>,
    ) {
        let placed = space;
        let shape = shaped;
        let material = shape.z as u32;
        let entry = materials[material];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shape.x, shape.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        let camera = view[0u32];
        let reach = (placed.x - camera.eye.x) * camera.forward.x
            + (placed.y - camera.eye.y) * camera.forward.y
            + (placed.z - camera.eye.z) * camera.forward.z;
        target = fvec4(
            (reach - camera.near) / (camera.far - camera.near),
            0.0,
            0.0,
            1.0,
        );
    }
}

#[belfry_gpu::graphics]
mod basis {
    #[binding]
    fn step(step: Read<HzbStepRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = step[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * held.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let at = coverage;
        let held = step[0u32];
        let column = floor(at.x * held.target_width);
        let row = floor(at.y * held.target_height);
        let mut deepest = 0.0;
        for y in stride(0u32, 4u32, 1u32) {
            for x in stride(0u32, 4u32, 1u32) {
                let u = clamped(
                    (column * 4.0 + x as f32 + 0.5) / held.source_width,
                    0.5 / held.source_width,
                    1.0 - 0.5 / held.source_width,
                );
                let v = clamped(
                    (row * 4.0 + y as f32 + 0.5) / held.source_height,
                    0.5 / held.source_height,
                    1.0 - 0.5 / held.source_height,
                );
                deepest = max(deepest, sample(source, filter, fvec2(u, v)).x);
            }
        }
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

#[belfry_gpu::graphics]
mod reduction {
    #[binding]
    fn step(step: Read<HzbStepRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = step[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * held.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let at = coverage;
        let held = step[0u32];
        let column = floor(at.x * held.target_width);
        let row = floor(at.y * held.target_height);
        let mut deepest = 0.0;
        for y in stride(0u32, 2u32, 1u32) {
            for x in stride(0u32, 2u32, 1u32) {
                let u = clamped(
                    (column * 2.0 + x as f32 + 0.5) / held.source_width,
                    0.5 / held.source_width,
                    1.0 - 0.5 / held.source_width,
                );
                let v = clamped(
                    (row * 2.0 + y as f32 + 0.5) / held.source_height,
                    0.5 / held.source_height,
                    1.0 - 0.5 / held.source_height,
                );
                deepest = max(deepest, sample(source, filter, fvec2(u, v)).x);
            }
        }
        target = fvec4(deepest, 0.0, 0.0, 1.0);
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

pub(crate) fn culling() -> ComputeProgram {
    cull()
}

pub(crate) fn clustering() -> ComputeProgram {
    cluster()
}

pub(crate) fn instancing() -> GraphicsProgram {
    instanced()
}

pub(crate) fn shadowing() -> GraphicsProgram {
    casting()
}

pub(crate) fn depth_prepass() -> GraphicsProgram {
    depth()
}

pub(crate) fn picking() -> GraphicsProgram {
    selection()
}

pub(crate) fn hzb_base() -> GraphicsProgram {
    basis()
}

pub(crate) fn hzb_reduce() -> GraphicsProgram {
    reduction()
}

#[belfry_gpu::graphics]
mod transparency {
    #[binding]
    fn settings(settings: Read<TransparentSetupRecord>) {}

    #[binding]
    fn fragments(fragments: Read<TransparentRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        source: Texture2D,
        filter: Sampler,
        heads: Read<u32>,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = settings[0u32];
        let at = coverage;
        let width = setup.width as f32;
        let height = setup.height as f32;
        let pixel_x = min(max(floor(at.x * width), 0.0), width - 1.0) as u32;
        let pixel_y = min(max(floor(at.y * height), 0.0), height - 1.0) as u32;
        let pixel = pixel_y * setup.width + pixel_x;
        let base = sample(source, filter, at);
        let mut red = 0.0;
        let mut green = 0.0;
        let mut blue = 0.0;
        let mut marker = 0.0;
        let head = heads[pixel];
        let mut total = 0u32;
        let mut node = head;
        while node != 0u32 && total < 64u32 {
            total += 1u32;
            node = fragments[node - 1u32].next;
        }
        let mut transmit = 1.0;
        let mut previous = 0.0;
        for _step in stride(0u32, total, 1u32) {
            let mut best = 1.0e30;
            let mut found = 0u32;
            let mut node = head;
            while node != 0u32 {
                let held = fragments[node - 1u32];
                if held.depth > previous && held.depth < best {
                    best = held.depth;
                    found = node;
                }
                node = held.next;
            }
            if found == 0u32 {
                break;
            }
            let held = fragments[found - 1u32];
            red += held.red * transmit;
            green += held.green * transmit;
            blue += held.blue * transmit;
            marker += held.alpha * transmit;
            transmit *= max(0.0, 1.0 - held.alpha);
            previous = best;
            if transmit <= 0.001 {
                break;
            }
        }
        target = fvec4(
            base.x * transmit + red,
            base.y * transmit + green,
            base.z * transmit + blue,
            base.w * transmit + marker,
        );
    }
}

pub(crate) fn transparency_resolve() -> GraphicsProgram {
    transparency()
}

#[belfry_gpu::graphics]
mod bloom_extraction {
    #[binding]
    fn settings(settings: Read<BloomRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(source, filter, at, step, -2.0, 2.0);
        let b = tap(source, filter, at, step, 0.0, 2.0);
        let c = tap(source, filter, at, step, 2.0, 2.0);
        let d = tap(source, filter, at, step, -2.0, 0.0);
        let e = tap(source, filter, at, step, 0.0, 0.0);
        let f = tap(source, filter, at, step, 2.0, 0.0);
        let g = tap(source, filter, at, step, -2.0, -2.0);
        let h = tap(source, filter, at, step, 0.0, -2.0);
        let i = tap(source, filter, at, step, 2.0, -2.0);
        let j = tap(source, filter, at, step, -1.0, 1.0);
        let k = tap(source, filter, at, step, 1.0, 1.0);
        let l = tap(source, filter, at, step, -1.0, -1.0);
        let m = tap(source, filter, at, step, 1.0, -1.0);
        let gathered = karis(d, e, i, j) * 0.5
            + karis(a, b, f, g) * 0.125
            + karis(b, c, g, h) * 0.125
            + karis(f, g, k, l) * 0.125
            + karis(g, h, l, m) * 0.125;
        target = thresholded(gathered, setup.threshold, setup.knee);
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }

    fn karis(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4) -> Fvec4 {
        let wa = 1.0 / (1.0 + luma(a));
        let wb = 1.0 / (1.0 + luma(b));
        let wc = 1.0 / (1.0 + luma(c));
        let wd = 1.0 / (1.0 + luma(d));
        let total = wa + wb + wc + wd;
        fvec4(
            (a.x * wa + b.x * wb + c.x * wc + d.x * wd) / total,
            (a.y * wa + b.y * wb + c.y * wc + d.y * wd) / total,
            (a.z * wa + b.z * wb + c.z * wc + d.z * wd) / total,
            1.0,
        )
    }

    fn luma(value: Fvec4) -> f32 {
        0.2126 * value.x + 0.7152 * value.y + 0.0722 * value.z
    }

    fn thresholded(value: Fvec4, threshold: f32, knee: f32) -> Fvec4 {
        let brightness = max(value.x, max(value.y, value.z));
        let soft = clamped(brightness - threshold + knee, 0.0, 2.0 * knee);
        let softened = soft * soft / (4.0 * knee + 0.00001);
        let contribution = max(softened, brightness - threshold) / max(brightness, 0.00001);
        value * contribution
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

#[belfry_gpu::graphics]
mod bloom_downsampling {
    #[binding]
    fn settings(settings: Read<BloomRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(source: Texture2D, filter: Sampler, coverage: In<Fvec2>, target: Out<Fvec4>) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(source, filter, at, step, -2.0, 2.0);
        let b = tap(source, filter, at, step, 0.0, 2.0);
        let c = tap(source, filter, at, step, 2.0, 2.0);
        let d = tap(source, filter, at, step, -2.0, 0.0);
        let e = tap(source, filter, at, step, 0.0, 0.0);
        let f = tap(source, filter, at, step, 2.0, 0.0);
        let g = tap(source, filter, at, step, -2.0, -2.0);
        let h = tap(source, filter, at, step, 0.0, -2.0);
        let i = tap(source, filter, at, step, 2.0, -2.0);
        let j = tap(source, filter, at, step, -1.0, 1.0);
        let k = tap(source, filter, at, step, 1.0, 1.0);
        let l = tap(source, filter, at, step, -1.0, -1.0);
        let m = tap(source, filter, at, step, 1.0, -1.0);
        target = e * 0.125
            + (a + c + g + i) * 0.03125
            + (b + d + f + h) * 0.0625
            + (j + k + l + m) * 0.125;
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }
}

#[belfry_gpu::graphics]
mod bloom_upsampling {
    #[binding]
    fn settings(settings: Read<BloomUpRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        low: Texture2D,
        high: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = settings[0u32];
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let at = coverage;
        let a = tap(low, filter, at, step, -1.0, -1.0);
        let b = tap(low, filter, at, step, 0.0, -1.0);
        let c = tap(low, filter, at, step, 1.0, -1.0);
        let d = tap(low, filter, at, step, -1.0, 0.0);
        let e = tap(low, filter, at, step, 0.0, 0.0);
        let f = tap(low, filter, at, step, 1.0, 0.0);
        let g = tap(low, filter, at, step, -1.0, 1.0);
        let h = tap(low, filter, at, step, 0.0, 1.0);
        let i = tap(low, filter, at, step, 1.0, 1.0);
        let tent = (a + c + g + i) * 0.0625 + (b + d + f + h) * 0.125 + e * 0.25;
        target = tent + sample(high, filter, at);
    }

    fn tap(source: Texture2D, filter: Sampler, at: Fvec2, step: Fvec2, x: f32, y: f32) -> Fvec4 {
        sample(source, filter, fvec2(at.x + step.x * x, at.y + step.y * y))
    }
}

#[belfry_gpu::graphics]
mod tone_mapping {
    #[binding]
    fn settings(settings: Read<TonemapRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let setup = settings[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * setup.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        lit: Texture2D,
        bloom: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = settings[0u32];
        let at = coverage;
        let scene = sample(lit, filter, at);
        let glow = sample(bloom, filter, at);
        let held = fvec4(
            scene.x + glow.x * setup.intensity,
            scene.y + glow.y * setup.intensity,
            scene.z + glow.z * setup.intensity,
            1.0,
        );
        let neutral = pbr_neutral(held);
        let step = fvec2(1.0 / setup.source_width, 1.0 / setup.source_height);
        let left = sample(lit, filter, fvec2(at.x - 2.0 * step.x, at.y));
        let right = sample(lit, filter, fvec2(at.x + 2.0 * step.x, at.y));
        let down = sample(lit, filter, fvec2(at.x, at.y - 2.0 * step.y));
        let up = sample(lit, filter, fvec2(at.x, at.y + 2.0 * step.y));
        let near = max(max(left.w, right.w), max(down.w, up.w));
        let outlined = select(0.0, 1.0, near > 0.5 && scene.w < 0.5);
        let shaded = fvec4(
            encoded(neutral.x),
            encoded(neutral.y),
            encoded(neutral.z),
            1.0,
        );
        let outline = fvec4(0.30, 0.62, 1.0, 1.0);
        target = fvec4(
            shaded.x + (outline.x - shaded.x) * outlined,
            shaded.y + (outline.y - shaded.y) * outlined,
            shaded.z + (outline.z - shaded.z) * outlined,
            1.0,
        );
    }

    fn pbr_neutral(colour: Fvec4) -> Fvec4 {
        let start = 0.76;
        let desaturation = 0.15;
        let lowest = min(colour.x, min(colour.y, colour.z));
        let offset = select(0.04, lowest - 6.25 * lowest * lowest, lowest < 0.08);
        let shifted = fvec4(colour.x - offset, colour.y - offset, colour.z - offset, 1.0);
        let peak = max(shifted.x, max(shifted.y, shifted.z));
        if peak < start {
            return shifted;
        }
        let headroom = 1.0 - start;
        let compressed = 1.0 - headroom * headroom / (peak + headroom - start);
        let scale = compressed / peak;
        let grey = 1.0 - 1.0 / (desaturation * (peak - compressed) + 1.0);
        fvec4(
            shifted.x * scale + (compressed - shifted.x * scale) * grey,
            shifted.y * scale + (compressed - shifted.y * scale) * grey,
            shifted.z * scale + (compressed - shifted.z * scale) * grey,
            1.0,
        )
    }

    fn encoded(value: f32) -> f32 {
        let held = max(value, 0.0);
        select(
            1.055 * pow(held, 0.41666666) - 0.055,
            held * 12.92,
            held <= 0.0031308,
        )
    }
}

#[belfry_gpu::graphics]
mod sky {
    #[binding]
    fn view(view: Read<SkyRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, screen: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        screen = fvec2(x, y);
    }

    #[fragment]
    fn fragment(radiance: Texture2D, sampler: Sampler, at: In<Fvec2>, target: Out<Fvec4>) {
        let held = view[0u32];
        let ray = at;
        let far = held.inverse0 * ray.x + held.inverse1 * ray.y + held.inverse2 + held.inverse3;
        let at = fvec4(far.x / far.w, far.y / far.w, far.z / far.w, 1.0) - held.eye;
        let unit = turned(
            held.environment0,
            held.environment1,
            held.environment2,
            normalized(at),
        );
        let held_radiance = sample_lod(
            radiance,
            sampler,
            fvec2(
                atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
                acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
            ),
            0.0,
        );
        target = fvec4(
            held_radiance.x * held.intensity,
            held_radiance.y * held.intensity,
            held_radiance.z * held.intensity,
            1.0,
        );
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn turned(first: Fvec4, second: Fvec4, third: Fvec4, vector: Fvec4) -> Fvec4 {
        fvec4(
            first.x * vector.x + second.x * vector.y + third.x * vector.z,
            first.y * vector.x + second.y * vector.y + third.y * vector.z,
            first.z * vector.x + second.z * vector.y + third.z * vector.z,
            0.0,
        )
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

pub(crate) fn sky_program() -> GraphicsProgram {
    sky()
}

#[belfry_gpu::graphics]
mod prefilter {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(
        level: Read<BakeLevelRecord>,
        panorama: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = level[0u32];
        let normal = ray(fvec2(at.x * 0.5 + 0.5, at.y * held.vertical + 0.5));
        let radiance = prefiltered(panorama, sampler, normal, held.roughness);
        target = fvec4(radiance.x, radiance.y, radiance.z, 1.0);
    }

    fn prefiltered(panorama: Texture2D, sampler: Sampler, normal: Fvec4, roughness: f32) -> Fvec4 {
        if roughness <= 0.0 {
            return sample_lod(panorama, sampler, equirect(normal), 0.0);
        }
        let view = normalized(normal);
        let mut sign = -1.0;
        if view.z >= 0.0 {
            sign = 1.0;
        }
        let frame = -1.0 / (sign + view.z);
        let cross = view.x * view.y * frame;
        let tangent = fvec4(
            1.0 + sign * view.x * view.x * frame,
            sign * cross,
            -sign * view.x,
            0.0,
        );
        let bitangent = fvec4(cross, sign + view.y * view.y * frame, -view.y, 0.0);
        let alpha = roughness * roughness;
        let mut sum = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut weight = 0.0;
        for index in stride(0u32, 128u32, 1u32) {
            let rank = radical(index);
            let phi = (710.0 / 113.0) * (index as f32 * (1.0 / 128.0));
            let cosine = sqrt(max(
                (1.0 - rank) / (1.0 + (alpha * alpha - 1.0) * rank),
                0.0,
            ));
            let sine = sqrt(max(1.0 - cosine * cosine, 0.0));
            let half = tangent * (sine * cos(phi)) + bitangent * (sine * sin(phi)) + view * cosine;
            let light = normalized(half * (2.0 * dot3(view, half)) - view);
            let incidence = max(dot3(view, light), 0.0);
            if incidence > 0.0 {
                sum = sum + sample_lod(panorama, sampler, equirect(light), 0.0) * incidence;
                weight += incidence;
            }
        }
        if weight == 0.0 {
            return sample_lod(panorama, sampler, equirect(view), 0.0);
        }
        sum * (1.0 / weight)
    }

    fn radical(bits: u32) -> f32 {
        let mut weight = 0.5;
        let mut value = 0.0;
        for bit in stride(0u32, 7u32, 1u32) {
            if ((bits >> bit) & 1u32) == 1u32 {
                value += weight;
            }
            weight *= 0.5;
        }
        value
    }

    fn ray(at: Fvec2) -> Fvec4 {
        let phi = (at.x - 0.5) * (710.0 / 113.0);
        let theta = at.y * (355.0 / 113.0);
        let sine = sin(theta);
        fvec4(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0)
    }

    fn equirect(direction: Fvec4) -> Fvec2 {
        let unit = normalized(direction);
        fvec2(
            atan2(unit.x, -unit.z) * (113.0 / 710.0) + 0.5,
            acos(clamped(unit.y, -1.0, 1.0)) * (113.0 / 355.0),
        )
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

#[belfry_gpu::graphics]
mod integration {
    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * 0.5 + 0.5);
    }

    #[fragment]
    fn fragment(
        setup: Read<IrradianceSetupRecord>,
        partials: ReadWrite<IrradianceRecord>,
        panorama: Texture2D,
        sampler: Sampler,
        at: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let held = setup[0u32];
        let index = floor(at.x * held.partials as f32) as u32;
        let width = held.width;
        let height = held.height;
        let width_f = width as f32;
        let height_f = height as f32;
        let solid = ((710.0 / 113.0) / width_f) * ((355.0 / 113.0) / height_f);
        let mut first = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut second = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut third = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut fourth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut fifth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut sixth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut seventh = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut eighth = fvec4(0.0, 0.0, 0.0, 0.0);
        let mut ninth = fvec4(0.0, 0.0, 0.0, 0.0);
        for texel in stride(index, held.texels, held.partials) {
            let column = texel % width;
            let row = texel / width;
            let u = (column as f32 + 0.5) / width_f;
            let v = (row as f32 + 0.5) / height_f;
            let phi = (u - 0.5) * (710.0 / 113.0);
            let theta = v * (355.0 / 113.0);
            let sine = sin(theta);
            let axis_x = sine * sin(phi);
            let axis_y = cos(theta);
            let axis_z = -sine * cos(phi);
            let radiance = sample_lod(panorama, sampler, fvec2(u, v), 0.0)
                * (sin(v * (355.0 / 113.0)) * solid);
            first = first + radiance * 0.886227;
            second = second + radiance * (1.0233276 * axis_y);
            third = third + radiance * (1.0233276 * axis_z);
            fourth = fourth + radiance * (1.0233276 * axis_x);
            fifth = fifth + radiance * (0.8580851 * axis_x * axis_y);
            sixth = sixth + radiance * (0.8580851 * axis_y * axis_z);
            seventh = seventh + radiance * (0.2477083 * (3.0 * axis_z * axis_z - 1.0));
            eighth = eighth + radiance * (0.8580851 * axis_x * axis_z);
            ninth = ninth + radiance * (0.429041 * (axis_x * axis_x - axis_y * axis_y));
        }
        partials[index].coefficient0 = first;
        partials[index].coefficient1 = second;
        partials[index].coefficient2 = third;
        partials[index].coefficient3 = fourth;
        partials[index].coefficient4 = fifth;
        partials[index].coefficient5 = sixth;
        partials[index].coefficient6 = seventh;
        partials[index].coefficient7 = eighth;
        partials[index].coefficient8 = ninth;
        target = fvec4(0.0, 0.0, 0.0, 1.0);
    }
}

#[belfry_gpu::kernel(workgroup_size = 64)]
mod irradiance_reduction {
    #[kernel]
    fn irradiance_reduction(lid: u32, partials: Read<f32>, total: ReadWrite<f32>) {
        if lid < 27u32 {
            let at = (lid / 3u32) * 4u32 + lid % 3u32;
            let mut sum = 0.0;
            for index in stride(0u32, 1024u32, 1u32) {
                sum += partials[index * 36u32 + at];
            }
            total[lid] = sum;
        }
    }
}

pub(crate) fn prefiltering() -> GraphicsProgram {
    prefilter()
}

pub(crate) fn integrating() -> GraphicsProgram {
    integration()
}

pub(crate) fn irradiance_reduction_program() -> ComputeProgram {
    irradiance_reduction()
}

#[belfry_gpu::graphics]
mod capture_atlas {
    #[binding]
    fn setup(setup: Read<AtlasRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x, y);
    }

    #[fragment]
    fn fragment(atlas: Texture2D, sampler: Sampler, at: In<Fvec2>, target: Out<Fvec4>) {
        let held = setup[0u32];
        let direction = ray(fvec2(at.x * 0.5 + 0.5, at.y * held.vertical + 0.5));
        let lit = sample_lod(
            atlas,
            sampler,
            face_of(direction, held.face, held.texel),
            0.0,
        );
        target = fvec4(lit.x, lit.y, lit.z, 1.0);
    }

    fn face_of(direction: Fvec4, face: f32, texel: f32) -> Fvec2 {
        let unit = normalized(direction);
        let across = abs(unit.x);
        let up = abs(unit.y);
        let forward = abs(unit.z);
        let mut index = 0.0;
        let mut horizontal = -unit.z;
        let mut vertical = -unit.y;
        let mut major = across;
        if across >= up && across >= forward {
            if unit.x < 0.0 {
                index = 1.0;
                horizontal = unit.z;
            }
        } else if up >= forward {
            major = up;
            index = 2.0;
            horizontal = unit.x;
            vertical = unit.z;
            if unit.y < 0.0 {
                index = 3.0;
                vertical = -unit.z;
            }
        } else {
            major = forward;
            index = 4.0;
            horizontal = unit.x;
            vertical = -unit.y;
            if unit.z < 0.0 {
                index = 5.0;
                horizontal = -unit.x;
            }
        }
        let span = max(major, 0.000001);
        let left = 0.5 + 0.5 * horizontal / span;
        let top = 0.5 - 0.5 * vertical / span;
        fvec2(
            (index + clamped(left, texel, 1.0 - texel)) * face,
            clamped(top, texel, 1.0 - texel),
        )
    }

    fn ray(at: Fvec2) -> Fvec4 {
        let phi = (at.x - 0.5) * (710.0 / 113.0);
        let theta = at.y * (355.0 / 113.0);
        let sine = sin(theta);
        fvec4(sine * sin(phi), cos(theta), -sine * cos(phi), 0.0)
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }
}

pub(crate) fn capture_conversion() -> GraphicsProgram {
    capture_atlas()
}

pub(crate) fn bloom_extract() -> GraphicsProgram {
    bloom_extraction()
}

pub(crate) fn debug_lines() -> GraphicsProgram {
    lines()
}

pub(crate) fn bloom_down() -> GraphicsProgram {
    bloom_downsampling()
}

pub(crate) fn bloom_up() -> GraphicsProgram {
    bloom_upsampling()
}

pub(crate) fn tonemap() -> GraphicsProgram {
    tone_mapping()
}

#[belfry_gpu::graphics]
mod lines {
    #[binding]
    fn view(view: Read<ViewRecord>) {}

    #[binding]
    fn segments(segments: Read<DebugLineRecord>) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, tint: Out<Fvec4>) {
        let camera = view[0u32];
        let segment = segments[vertex / 6u32];
        let corner = vertex % 6u32;
        let head = column(
            camera.clip0,
            camera.clip1,
            camera.clip2,
            camera.clip3,
            segment.start,
        );
        let tail = column(
            camera.clip0,
            camera.clip1,
            camera.clip2,
            camera.clip3,
            segment.end,
        );
        let mut x = 2.0;
        let mut y = 2.0;
        let mut z = 1.0;
        let mut alpha = 0.0;
        if head.w > 0.0001 && tail.w > 0.0001 {
            let head_x = head.x / head.w;
            let head_y = head.y / head.w;
            let tail_x = tail.x / tail.w;
            let tail_y = tail.y / tail.w;
            let span_x = (tail_x - head_x) * camera.view_width;
            let span_y = (tail_y - head_y) * camera.view_height;
            let reach = max(sqrt(span_x * span_x + span_y * span_y), 0.0001);
            let normal_x = -span_y / reach * 2.0 / camera.view_width;
            let normal_y = span_x / reach * 2.0 / camera.view_height;
            let mut end = 0.0;
            let mut side = -1.0;
            if corner == 1u32 {
                side = 1.0;
            }
            if corner == 2u32 {
                end = 1.0;
                side = 1.0;
            }
            if corner == 4u32 {
                end = 1.0;
                side = 1.0;
            }
            if corner == 5u32 {
                end = 1.0;
                side = -1.0;
            }
            x = head_x + (tail_x - head_x) * end + normal_x * side;
            y = head_y + (tail_y - head_y) * end + normal_y * side;
            z = head.z / head.w + (tail.z / tail.w - head.z / head.w) * end;
            alpha = segment.color.w;
        }
        position = fvec4(x, y, z, 1.0);
        tint = fvec4(segment.color.x, segment.color.y, segment.color.z, alpha);
    }

    #[fragment]
    fn fragment(input: In<Fvec4>, target: Out<Fvec4>) {
        let color = input;
        target = fvec4(
            color.x * color.w,
            color.y * color.w,
            color.z * color.w,
            color.w,
        );
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }
}

#[belfry_gpu::graphics]
mod motion {
    #[binding]
    fn view(view: Read<MotionRecord>) {}

    #[binding]
    fn drawn(drawn: Read<DrawRecord>) {}

    #[binding]
    fn palette(palette: Read<JointRecord>) {}

    #[binding]
    fn skins(skins: Read<SkinVertexRecord>) {}

    #[binding]
    fn materials(materials: Read<MaterialRecord>) {}

    #[binding]
    fn previous(previous: Read<JointRecord>) {}

    fn deformed(col0: Fvec4, col1: Fvec4, col2: Fvec4, col3: Fvec4, point: Fvec4) -> Fvec4 {
        col0 * point.x + col1 * point.y + col2 * point.z + col3 * point.w
    }

    #[vertex]
    fn vertex(
        vertex: u32,
        instance: u32,
        vertices: Read<f32>,
        position: Out<Position>,
        surface: Out<Fvec4>,
        travel: Out<Fvec4>,
    ) {
        let held = drawn[instance];
        let camera = view[0u32];
        let base = vertex * 8u32;
        let rest = fvec4(
            vertices[base],
            vertices[base + 1u32],
            vertices[base + 2u32],
            1.0,
        );
        let mut local = rest;
        let mut carried = rest;
        if held.joints > 0u32 {
            let skin = skins[vertex];
            let first = palette[held.palette + skin.joints.x];
            let second = palette[held.palette + skin.joints.y];
            let third = palette[held.palette + skin.joints.z];
            let fourth = palette[held.palette + skin.joints.w];
            let place = deformed(first.col0, first.col1, first.col2, first.col3, rest)
                * skin.weights.x
                + deformed(second.col0, second.col1, second.col2, second.col3, rest)
                    * skin.weights.y
                + deformed(third.col0, third.col1, third.col2, third.col3, rest) * skin.weights.z
                + deformed(fourth.col0, fourth.col1, fourth.col2, fourth.col3, rest)
                    * skin.weights.w;
            local = fvec4(place.x, place.y, place.z, 1.0);
            let was_first = previous[held.previous_palette + skin.joints.x];
            let was_second = previous[held.previous_palette + skin.joints.y];
            let was_third = previous[held.previous_palette + skin.joints.z];
            let was_fourth = previous[held.previous_palette + skin.joints.w];
            let was = deformed(
                was_first.col0,
                was_first.col1,
                was_first.col2,
                was_first.col3,
                rest,
            ) * skin.weights.x
                + deformed(
                    was_second.col0,
                    was_second.col1,
                    was_second.col2,
                    was_second.col3,
                    rest,
                ) * skin.weights.y
                + deformed(
                    was_third.col0,
                    was_third.col1,
                    was_third.col2,
                    was_third.col3,
                    rest,
                ) * skin.weights.z
                + deformed(
                    was_fourth.col0,
                    was_fourth.col1,
                    was_fourth.col2,
                    was_fourth.col3,
                    rest,
                ) * skin.weights.w;
            carried = fvec4(was.x, was.y, was.z, 1.0);
        }
        let current_world = held.model0 * local.x
            + held.model1 * local.y
            + held.model2 * local.z
            + held.model3 * local.w;
        let current_clip = held.clip0 * local.x
            + held.clip1 * local.y
            + held.clip2 * local.z
            + held.clip3 * local.w;
        let previous_world = held.previous0 * carried.x
            + held.previous1 * carried.y
            + held.previous2 * carried.z
            + held.previous3 * carried.w;
        let previous_clip = camera.previous0 * previous_world.x
            + camera.previous1 * previous_world.y
            + camera.previous2 * previous_world.z
            + camera.previous3 * previous_world.w;
        let current_inverse = 1.0 / max(current_clip.w, 0.0001);
        let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
        let reach = (current_world.x - camera.eye.x) * camera.forward.x
            + (current_world.y - camera.eye.y) * camera.forward.y
            + (current_world.z - camera.eye.z) * camera.forward.z;
        position = current_clip;
        surface = fvec4(
            vertices[base + 6u32],
            vertices[base + 7u32],
            held.material as f32,
            reach,
        );
        travel = fvec4(
            current_clip.x * current_inverse * 0.5 + 0.5,
            current_clip.y * current_inverse * camera.vertical + 0.5,
            previous_clip.x * previous_inverse * 0.5 + 0.5,
            previous_clip.y * previous_inverse * camera.vertical + 0.5,
        );
    }

    #[fragment]
    fn fragment(
        textures: TextureTable,
        samplers: SamplerTable,
        shaped: In<Fvec4>,
        moved: In<Fvec4>,
        target: Out<Fvec4>,
    ) {
        let entry = materials[shaped.z as u32];
        if entry.alpha_mode == 1u32 {
            let sampled = sample(
                textures[entry.base_color_image],
                samplers[entry.base_color_sampler],
                fvec2(shaped.x, shaped.y),
            );
            if sampled.w * entry.base_color.w < entry.alpha_cutoff {
                discard();
            }
        }
        target = fvec4(moved.z - moved.x, moved.w - moved.y, shaped.w, 1.0);
    }
}

pub(crate) fn motion_vectors() -> GraphicsProgram {
    motion()
}

#[belfry_gpu::graphics]
mod temporal {
    #[binding]
    fn frame(frame: Read<TemporalRecord>) {}

    #[binding]
    fn motion(motion: Texture2D, previous: Texture2D, nearest: Sampler) {}

    #[vertex]
    fn vertex(vertex: u32, position: Out<Position>, surface: Out<Fvec2>) {
        let held = frame[0u32];
        let mut x = -1.0;
        let mut y = -1.0;
        if vertex == 1u32 {
            x = 3.0;
        }
        if vertex == 2u32 {
            y = 3.0;
        }
        position = fvec4(x, y, 0.0, 1.0);
        surface = fvec2(x * 0.5 + 0.5, y * held.vertical + 0.5);
    }

    #[fragment]
    fn fragment(
        source: Texture2D,
        history: Texture2D,
        filter: Sampler,
        coverage: In<Fvec2>,
        target: Out<Fvec4>,
    ) {
        let setup = frame[0u32];
        let at = coverage;
        let recorded = sample(motion, nearest, at);
        let sky = recorded.w < 0.5;
        let mut reach = setup.far;
        if !sky {
            reach = recorded.z;
        }
        let mut history_at = fvec2(at.x + recorded.x, at.y + recorded.y);
        let ndc_x = at.x * 2.0 - 1.0;
        let ndc_y = (at.y - 0.5) / setup.vertical;
        let head = column(
            setup.inverse0,
            setup.inverse1,
            setup.inverse2,
            setup.inverse3,
            fvec4(ndc_x, ndc_y, 0.0, 1.0),
        );
        let tail = column(
            setup.inverse0,
            setup.inverse1,
            setup.inverse2,
            setup.inverse3,
            fvec4(ndc_x, ndc_y, 1.0, 1.0),
        );
        let head_point = fvec4(head.x / head.w, head.y / head.w, head.z / head.w, 1.0);
        let tail_point = fvec4(tail.x / tail.w, tail.y / tail.w, tail.z / tail.w, 1.0);
        let ray = normalized(tail_point - head_point);
        let mut travel = setup.far;
        if !sky {
            travel = reach / max(dot3(ray, setup.forward), 0.0001);
        }
        let world = fvec4(
            setup.eye.x + ray.x * travel,
            setup.eye.y + ray.y * travel,
            setup.eye.z + ray.z * travel,
            1.0,
        );
        let previous_clip = column(
            setup.previous0,
            setup.previous1,
            setup.previous2,
            setup.previous3,
            world,
        );
        let previous_inverse = 1.0 / max(previous_clip.w, 0.0001);
        let previous_at = fvec2(
            previous_clip.x * previous_inverse * 0.5 + 0.5,
            previous_clip.y * previous_inverse * setup.vertical + 0.5,
        );
        if sky {
            history_at = previous_at;
        }
        let previous_reach = (world.x - setup.previous_eye.x) * setup.previous_forward.x
            + (world.y - setup.previous_eye.y) * setup.previous_forward.y
            + (world.z - setup.previous_eye.z) * setup.previous_forward.z;
        let carried = sample(previous, nearest, history_at);
        let mut valid = history_at.x > 0.0
            && history_at.x < 1.0
            && history_at.y > 0.0
            && history_at.y < 1.0
            && previous_clip.w > 0.0001;
        if carried.w > 0.5 {
            valid = valid
                && abs(carried.z - previous_reach)
                    <= setup.tolerance * max(previous_reach, setup.near);
        } else {
            valid = valid && sky;
        }
        let current = sample(source, filter, at);
        let held = sample(history, filter, history_at);
        let step = fvec2(1.0 / setup.width, 1.0 / setup.height);
        let left = sample(source, filter, fvec2(at.x - step.x, at.y));
        let right = sample(source, filter, fvec2(at.x + step.x, at.y));
        let below = sample(source, filter, fvec2(at.x, at.y - step.y));
        let above = sample(source, filter, fvec2(at.x, at.y + step.y));
        let shade_low = lowest(lowest(lowest(left, right), lowest(below, above)), current);
        let shade_high = highest(
            highest(highest(left, right), highest(below, above)),
            current,
        );
        let bounded = fvec4(
            clamped(held.x, shade_low.x, shade_high.x),
            clamped(held.y, shade_low.y, shade_high.y),
            clamped(held.z, shade_low.z, shade_high.z),
            1.0,
        );
        let weight = select(0.0, setup.blend, valid);
        target = fvec4(
            bounded.x * weight + current.x * (1.0 - weight),
            bounded.y * weight + current.y * (1.0 - weight),
            bounded.z * weight + current.z * (1.0 - weight),
            current.w,
        );
    }

    fn column(a: Fvec4, b: Fvec4, c: Fvec4, d: Fvec4, local: Fvec4) -> Fvec4 {
        a * local.x + b * local.y + c * local.z + d * local.w
    }

    fn lowest(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            min(left.x, right.x),
            min(left.y, right.y),
            min(left.z, right.z),
            0.0,
        )
    }

    fn highest(left: Fvec4, right: Fvec4) -> Fvec4 {
        fvec4(
            max(left.x, right.x),
            max(left.y, right.y),
            max(left.z, right.z),
            0.0,
        )
    }

    fn clamped(value: f32, low: f32, high: f32) -> f32 {
        min(max(value, low), high)
    }

    fn dot3(left: Fvec4, right: Fvec4) -> f32 {
        left.x * right.x + left.y * right.y + left.z * right.z
    }

    fn normalized(vector: Fvec4) -> Fvec4 {
        let inverse =
            1.0 / sqrt(vector.x * vector.x + vector.y * vector.y + vector.z * vector.z + 0.0000001);
        vector * inverse
    }
}

pub(crate) fn temporal_resolve() -> GraphicsProgram {
    temporal()
}
