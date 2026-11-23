use crate::abi::{TRANSPARENT_LAYERS, TransparentRecord, TransparentSetupRecord};
use crate::program;
use belfry_gpu_device::{
    AddressMode, Binding, BufferBinding, BufferUsages, Device, Draw, Filter, GpuBuffer,
    GraphicsPipeline, Pass, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture,
    TextureFormat,
};
use std::mem::size_of;

const RECORD_BYTES: u64 = size_of::<TransparentRecord>() as u64;
const SETTINGS_BYTES: u64 = size_of::<TransparentSetupRecord>() as u64;
const WORD_BYTES: u64 = 4;
const RESOLVE_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const LIT_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

struct Screen {
    counts: GpuBuffer,
    fragments: GpuBuffer,
    dropped: GpuBuffer,
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
        let fragment_bytes = pixels * u64::from(TRANSPARENT_LAYERS) * RECORD_BYTES;
        assert!(
            fragment_bytes <= device.limits().max_storage_buffer_binding_size,
            "a {width} by {height} frame layers {fragment_bytes} transparent bytes beyond the device storage binding",
        );
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let counts = GpuBuffer::new(
            device,
            "belfry transparent counts",
            pixels * WORD_BYTES,
            storage,
        );
        let fragments = GpuBuffer::new(
            device,
            "belfry transparent fragments",
            fragment_bytes,
            BufferUsages::STORAGE,
        );
        let dropped = GpuBuffer::new(
            device,
            "belfry transparent dropped",
            WORD_BYTES,
            storage | BufferUsages::COPY_SRC,
        );
        let settings = GpuBuffer::new(
            device,
            "belfry transparent settings",
            SETTINGS_BYTES,
            storage,
        );
        settings.write(
            queue,
            bytemuck::bytes_of(&TransparentSetupRecord {
                layers: TRANSPARENT_LAYERS,
                width,
                height,
                vertical,
            }),
        );
        self.screen = Some(Screen {
            counts,
            fragments,
            dropped,
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
            &screen.counts,
            0,
            u64::from(screen.width) * u64::from(screen.height) * WORD_BYTES,
        );
        submission.clear(&screen.dropped, 0, WORD_BYTES);
    }

    pub(crate) fn fragments(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.fragments.binding(0, screen.fragments.size())
    }

    pub(crate) fn counts(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.counts.binding(
            0,
            u64::from(screen.width) * u64::from(screen.height) * WORD_BYTES,
        )
    }

    pub(crate) fn dropped(&self) -> BufferBinding<'_> {
        let screen = self
            .screen
            .as_ref()
            .expect("a transparent frame holds its fragments");
        screen.dropped.binding(0, WORD_BYTES)
    }

    pub(crate) fn dropped_buffer(&self) -> Option<&GpuBuffer> {
        self.screen.as_ref().map(|screen| &screen.dropped)
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
                    program::RESOLVE_COUNTS,
                    screen.counts.binding(
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
            Pass::new(lit, CLEAR),
            &[Draw::new(&self.resolve, &[frame, image], RESOLVE_VERTICES)],
        );
    }
}

const _: () = assert!(
    RECORD_BYTES == 8,
    "a transparent layer packs its shaded colour beside its depth and coverage",
);
const _: () = assert!(
    SETTINGS_BYTES == 16,
    "the transparent settings pack the layers, viewport and vertical direction",
);
