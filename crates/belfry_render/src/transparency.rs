use crate::program;
use belfry_gpu_device::{
    AddressMode, Binding, BufferBinding, BufferUsages, Device, Draw, Filter, GpuBuffer,
    GraphicsPipeline, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture,
    TextureFormat,
};
use belfry_render_abi::{TransparentRecord, TransparentSetupRecord};
use std::mem::size_of;

const RECORD_BYTES: u64 = size_of::<TransparentRecord>() as u64;
const SETTINGS_BYTES: u64 = size_of::<TransparentSetupRecord>() as u64;
const FRAGMENT_BUDGET: u64 = 128 << 20;
const EXPECTED_LAYERS: u64 = 4;
const WORD_BYTES: u64 = 4;
const RESOLVE_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const LIT_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

struct Screen {
    heads: GpuBuffer,
    fragments: GpuBuffer,
    counter: GpuBuffer,
    settings: GpuBuffer,
    width: u32,
    height: u32,
}

pub(crate) struct Transparency {
    resolve: GraphicsPipeline,
    sampler: Sampler,
    screen: Option<Screen>,
}

impl Transparency {
    pub(crate) fn new(device: &Device) -> Self {
        Self {
            resolve: GraphicsPipeline::new(
                device,
                program::transparency_resolve(),
                LIT_FORMAT,
                PipelineState::SCREEN,
            ),
            sampler: Sampler::new(
                device,
                SamplerDesc {
                    min_filter: Filter::Nearest,
                    mag_filter: Filter::Nearest,
                    mip_filter: Filter::Nearest,
                    address_u: AddressMode::ClampToEdge,
                    address_v: AddressMode::ClampToEdge,
                },
            ),
            screen: None,
        }
    }

    pub(crate) fn ensure(
        &mut self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        vertical: f32,
    ) {
        if self
            .screen
            .as_ref()
            .is_some_and(|screen| screen.width == width && screen.height == height)
        {
            return;
        }
        let pixels = u64::from(width) * u64::from(height);
        let budget = FRAGMENT_BUDGET
            .min(device.limits().max_storage_buffer_binding_size)
            .min(device.limits().max_buffer_size)
            / RECORD_BYTES;
        let capacity = (pixels * EXPECTED_LAYERS)
            .min(budget)
            .clamp(1, u64::from(u32::MAX));
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let heads = GpuBuffer::new(
            device,
            "belfry transparent heads",
            pixels * WORD_BYTES,
            storage,
        );
        let fragments = GpuBuffer::new(
            device,
            "belfry transparent fragments",
            capacity * RECORD_BYTES,
            BufferUsages::STORAGE,
        );
        let counter = GpuBuffer::new(device, "belfry transparent counter", WORD_BYTES, storage);
        let settings = GpuBuffer::new(
            device,
            "belfry transparent settings",
            SETTINGS_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        settings.write(
            queue,
            bytemuck::bytes_of(&TransparentSetupRecord {
                capacity: capacity as u32,
                width,
                height,
                vertical,
            }),
        );
        self.screen = Some(Screen {
            heads,
            fragments,
            counter,
            settings,
            width,
            height,
        });
    }

    pub(crate) fn clear(&self, submission: &mut Submission) {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        submission.clear(
            &screen.heads,
            0,
            u64::from(screen.width) * u64::from(screen.height) * WORD_BYTES,
        );
        submission.clear(&screen.counter, 0, WORD_BYTES);
    }

    pub(crate) fn fragments(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.fragments.binding(0, screen.fragments.size())
    }

    pub(crate) fn heads(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.heads.binding(
            0,
            u64::from(screen.width) * u64::from(screen.height) * WORD_BYTES,
        )
    }

    pub(crate) fn counter(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.counter.binding(0, WORD_BYTES)
    }

    pub(crate) fn settings(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.settings.binding(0, SETTINGS_BYTES)
    }

    pub(crate) fn resolve(&self, submission: &mut Submission, source: &Texture, lit: &Texture) {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        let frame = self.resolve.bind_group(
            0,
            &[
                Binding::buffer(
                    program::RESOLVE_SETTINGS,
                    screen.settings.binding(0, SETTINGS_BYTES),
                ),
                Binding::buffer(
                    program::RESOLVE_FRAGMENTS,
                    screen.fragments.binding(0, screen.fragments.size()),
                ),
                Binding::buffer(
                    program::RESOLVE_HEADS,
                    screen.heads.binding(
                        0,
                        u64::from(screen.width) * u64::from(screen.height) * WORD_BYTES,
                    ),
                ),
            ],
        );
        let image = self.resolve.bind_group(
            1,
            &[
                Binding::texture(program::RESOLVE_SOURCE, source),
                Binding::sampler(program::RESOLVE_FILTER, &self.sampler),
            ],
        );
        submission.render(
            lit,
            CLEAR,
            &[Draw::new(&self.resolve, &[frame, image], RESOLVE_VERTICES)],
        );
    }
}

const _: () = assert!(
    RECORD_BYTES == 24,
    "a transparent fragment packs its shaded colour, depth and successor",
);
const _: () = assert!(
    SETTINGS_BYTES == 16,
    "the transparent settings pack the capacity, viewport and vertical direction",
);
