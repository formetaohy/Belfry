use crate::abi::{AtlasRecord, DrawRecord, InstanceRecord, SkyRecord, ViewRecord};
use crate::environment::{BakeRequest, Baked, Environment};
use crate::frame::{direction_record, record_of_view, vector};
use crate::growing::Growing;
use crate::program;
use crate::view::View;
use belfry_ecs::{Entity, World};
use belfry_gpu::Fvec4;
use belfry_gpu_device::ClipSpace;
use belfry_gpu_device::{
    AddressMode, BufferBinding, BufferUsages, Device, Filter, GpuBuffer, GpuContext,
    GraphicsPipeline, PipelineState, Queue, Sampler, SamplerDesc, Texture, TextureFormat,
    TextureUsages,
};
use belfry_math::{Mat4, Transform, Vec3, Vec4};
use std::mem::size_of;

pub const PROBE_RESOLUTION: u32 = 128;
pub(crate) const FACES: u32 = 6;
const PROBE_WIDTH: u32 = PROBE_RESOLUTION * FACES;
pub(crate) const PANORAMA_WIDTH: u32 = PROBE_RESOLUTION * 4;
pub(crate) const PANORAMA_HEIGHT: u32 = PROBE_RESOLUTION * 2;
const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const SKY_BYTES: u64 = size_of::<SkyRecord>() as u64;
const DRAW_BYTES: u64 = size_of::<DrawRecord>() as u64;
const SETUP_BYTES: u64 = size_of::<AtlasRecord>() as u64;
const NEAR: f32 = 0.05;
const FAR: f32 = 1_000.0;
const WIDE: f32 = std::f32::consts::FRAC_PI_2;

const FACES_OF: [(Vec3, Vec3); FACES as usize] = [
    (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
    (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
    (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, -1.0, 0.0)),
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureProbe {
    captured: Option<(Transform, u64)>,
}

impl CaptureProbe {
    pub fn stale(&self, transform: Transform, revision: u64) -> bool {
        self.captured != Some((transform, revision))
    }

    pub fn captured(&mut self, transform: Transform, revision: u64) {
        self.captured = Some((transform, revision));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneRevision(pub u64);

pub struct Capture {
    pub entity: Entity,
    pub probe: CaptureProbe,
    pub environment: Environment,
}

pub fn install_captures(world: &mut World, captures: impl IntoIterator<Item = Capture>) {
    for capture in captures {
        assert!(
            world.is_alive(capture.entity),
            "a capture names the live probe it renders",
        );
        world.insert(capture.entity, capture.probe);
        world.insert(capture.entity, capture.environment);
    }
}

pub(crate) struct Probe {
    pub(crate) entity: Entity,
    pub(crate) transform: Transform,
    pub(crate) probe: CaptureProbe,
    pub(crate) environment: Environment,
}

pub(crate) struct CaptureFrame<'a> {
    pub(crate) device: Device,
    pub(crate) queue: Queue,
    pub(crate) records: &'a [InstanceRecord],
    pub(crate) slots: u32,
    pub(crate) eye: Vec3,
    pub(crate) clip_space: ClipSpace,
    pub(crate) rotation: Mat4,
    pub(crate) intensity: f32,
}

pub(crate) struct Pending {
    pub(crate) entity: Entity,
    pub(crate) probe: CaptureProbe,
    pub(crate) intensity: f32,
    pub(crate) background: bool,
    pub(crate) blend: f32,
    pub(crate) panorama: Texture,
    pub(crate) request: BakeRequest,
}

pub(crate) struct Captures {
    pub(crate) atlas: Texture,
    pub(crate) views: GpuBuffer,
    pub(crate) skies: GpuBuffer,
    pub(crate) setup: GpuBuffer,
    pub(crate) drawn: Growing,
    pub(crate) convert: GraphicsPipeline,
    pub(crate) sampler: Sampler,
    slots: u32,
    pending: Option<Pending>,
    results: Vec<Capture>,
}

impl Captures {
    pub(crate) fn new(context: &GpuContext) -> Self {
        let device = context.device();
        let queue = Queue::of(device);
        let attachment = TextureUsages::SAMPLED | TextureUsages::RENDER_ATTACHMENT;
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let setup = GpuBuffer::new(device, "belfry capture atlas", SETUP_BYTES, storage);
        setup.write(
            &queue,
            bytemuck::bytes_of(&AtlasRecord {
                face: 1.0 / FACES as f32,
                vertical: crate::frame::vertical(device),
                texel: 0.5 / PROBE_RESOLUTION as f32,
                pad0: 0.0,
            }),
        );
        Self {
            atlas: Texture::new(
                device,
                "belfry capture faces",
                PROBE_WIDTH,
                PROBE_RESOLUTION,
                1,
                TextureFormat::Rgba16Float,
                attachment,
            ),
            views: GpuBuffer::new(
                device,
                "belfry capture views",
                u64::from(FACES) * VIEW_BYTES,
                storage,
            ),
            skies: GpuBuffer::new(
                device,
                "belfry capture skies",
                u64::from(FACES) * SKY_BYTES,
                storage,
            ),
            setup,
            drawn: Growing::new(device, "belfry capture drawn", storage),
            convert: GraphicsPipeline::new(
                device,
                program::capture_conversion(),
                TextureFormat::Rgba16Float,
                PipelineState::SCREEN,
            ),
            slots: 0,
            sampler: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: Filter::Linear,
                    mag_filter: Filter::Linear,
                    mip_filter: Filter::Nearest,
                    address_u: AddressMode::ClampToEdge,
                    address_v: AddressMode::ClampToEdge,
                },
            ),
            results: Vec::new(),
            pending: None,
        }
    }

    pub(crate) fn take(&mut self) -> Vec<Capture> {
        std::mem::take(&mut self.results)
    }

    pub(crate) fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    pub(crate) fn begin(&mut self, pending: Pending) {
        assert!(
            self.pending.is_none(),
            "a capture submits one bake before its probe settles",
        );
        self.pending = Some(pending);
    }

    pub(crate) fn settle(&mut self, baked: Baked, sampler: &Sampler) {
        let pending = self
            .pending
            .take()
            .expect("a settled capture answers the bake it submitted");
        let mut environment = Environment::new(
            baked.radiance,
            pending.panorama,
            sampler.clone(),
            baked.levels,
            baked.irradiance,
        );
        environment.intensity = pending.intensity;
        environment.background = pending.background;
        environment.local = true;
        environment.blend = pending.blend;
        self.results.push(Capture {
            entity: pending.entity,
            probe: pending.probe,
            environment,
        });
    }

    pub(crate) fn panorama(&self, device: &Device) -> Texture {
        Texture::new(
            device,
            "belfry capture panorama",
            PANORAMA_WIDTH,
            PANORAMA_HEIGHT,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::RENDER_ATTACHMENT,
        )
    }

    pub(crate) fn setup(&self) -> BufferBinding<'_> {
        self.setup.binding(0, SETUP_BYTES)
    }

    pub(crate) fn view(&self, face: u32) -> BufferBinding<'_> {
        self.views.binding(u64::from(face) * VIEW_BYTES, VIEW_BYTES)
    }

    pub(crate) fn sky(&self, face: u32) -> BufferBinding<'_> {
        self.skies.binding(u64::from(face) * SKY_BYTES, SKY_BYTES)
    }

    pub(crate) fn drawn(&self, face: u32, offset: u32, count: u32) -> BufferBinding<'_> {
        let base = u64::from(face) * u64::from(self.slots) + u64::from(offset);
        self.drawn
            .buffer()
            .binding(base * DRAW_BYTES, u64::from(count) * DRAW_BYTES)
    }

    pub(crate) fn prepare(&mut self, frame: &CaptureFrame<'_>) {
        self.slots = frame.slots.max(1);
        let views = FACES_OF.map(|(forward, up)| face_view(frame, forward, up));
        self.views.write(
            &frame.queue,
            bytemuck::cast_slice(
                &views
                    .iter()
                    .map(|view| {
                        record_of_view(view, frame.records.len() as u32, frame.eye, Vec3::Z)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
        self.skies.write(
            &frame.queue,
            bytemuck::cast_slice(
                &views
                    .iter()
                    .map(|view| sky_record(view, frame))
                    .collect::<Vec<_>>(),
            ),
        );
        let mut drawn = Vec::with_capacity(frame.records.len() * FACES as usize);
        for view in &views {
            for record in frame.records {
                drawn.push(drawn_of(record, view));
            }
        }
        self.drawn.fit(
            &frame.device,
            u64::from(self.slots) * DRAW_BYTES * u64::from(FACES),
        );
        if !drawn.is_empty() {
            self.drawn
                .buffer()
                .write(&frame.queue, bytemuck::cast_slice(&drawn));
        }
    }
}

fn face_view(frame: &CaptureFrame<'_>, forward: Vec3, up: Vec3) -> View {
    let projection = Mat4::perspective(WIDE, 1.0, NEAR, FAR);
    let projection = match frame.clip_space {
        ClipSpace::YUp => projection,
        ClipSpace::YDown => projection.flip_clip_y(),
    };
    View::of(projection * Mat4::look_at(frame.eye, frame.eye + forward.normalize(), up.normalize()))
}

fn sky_record(view: &View, frame: &CaptureFrame<'_>) -> SkyRecord {
    let inverse = view.clip.inverse();
    SkyRecord {
        inverse0: vector(inverse.col(0)),
        inverse1: vector(inverse.col(1)),
        inverse2: vector(inverse.col(2)),
        inverse3: vector(inverse.col(3)),
        eye: direction_record(frame.eye),
        environment0: vector(frame.rotation.col(0)),
        environment1: vector(frame.rotation.col(1)),
        environment2: vector(frame.rotation.col(2)),
        intensity: frame.intensity,
        pad0: 0,
        pad1: 0,
        pad2: 0,
    }
}

fn drawn_of(record: &InstanceRecord, view: &View) -> DrawRecord {
    let model = Mat4::from_cols(
        vec4(record.model0),
        vec4(record.model1),
        vec4(record.model2),
        vec4(record.model3),
    );
    let clip = view.clip * model;
    DrawRecord {
        clip0: vector(clip.col(0)),
        clip1: vector(clip.col(1)),
        clip2: vector(clip.col(2)),
        clip3: vector(clip.col(3)),
        model0: record.model0,
        model1: record.model1,
        model2: record.model2,
        model3: record.model3,
        previous0: record.previous0,
        previous1: record.previous1,
        previous2: record.previous2,
        previous3: record.previous3,
        normal0: record.normal0,
        normal1: record.normal1,
        normal2: record.normal2,
        tint: record.tint,
        material: record.material,
        pick: 0,
        palette: record.palette,
        joints: record.joints,
        previous_palette: record.previous_palette,
        pad0: 0,
        pad1: 0,
        pad2: 0,
    }
}

fn vec4(value: Fvec4) -> Vec4 {
    Vec4::new(value.x, value.y, value.z, value.w)
}
