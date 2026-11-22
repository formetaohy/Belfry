use crate::abi::{HZB_LEVEL, HZB_SCALE, HZB_STEP, HzbLevelRecord, HzbStepRecord};
use crate::program;
use belfry_gpu_device::{
    BindGroup, Binding, BufferBinding, BufferUsages, Device, Draw, GpuBuffer, GraphicsPipeline,
    PipelineHandle, Queue, Sampler, Submission, Texture, TextureFormat, TextureUsages,
};
use std::mem::size_of;

const HZB_LEVEL_BYTES: u64 = size_of::<HzbLevelRecord>() as u64;
const HZB_STEP_BYTES: u64 = size_of::<HzbStepRecord>() as u64;
const HZB_COPY_ALIGN: u64 = 512;
const WORD_BYTES: u64 = 4;
const FULLSCREEN_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

pub(crate) struct Hzb {
    width: u32,
    height: u32,
    depth: Texture,
    levels: Vec<Texture>,
    records: GpuBuffer,
    depths: GpuBuffer,
    base: [BindGroup; 2],
    reductions: Vec<[BindGroup; 2]>,
    offsets: Vec<u64>,
}

impl Hzb {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &Device,
        queue: &Queue,
        base_pipeline: &GraphicsPipeline,
        reduce_pipeline: &GraphicsPipeline,
        sampler: &Sampler,
        width: u32,
        height: u32,
        vertical: f32,
    ) -> Self {
        let mut dimensions = Vec::new();
        let mut current = (width.div_ceil(HZB_SCALE), height.div_ceil(HZB_SCALE));
        loop {
            dimensions.push(current);
            if current.0 == 1 && current.1 == 1 {
                break;
            }
            current = (current.0.div_ceil(2), current.1.div_ceil(2));
        }
        let usage =
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED | TextureUsages::COPY_SRC;
        let depth = Texture::new(
            device,
            "belfry depth",
            width,
            height,
            1,
            TextureFormat::R32Float,
            usage,
        );
        let levels = dimensions
            .iter()
            .map(|(level_width, level_height)| {
                Texture::new(
                    device,
                    "belfry hzb level",
                    *level_width,
                    *level_height,
                    1,
                    TextureFormat::R32Float,
                    usage,
                )
            })
            .collect::<Vec<_>>();
        let mut records = Vec::with_capacity(levels.len());
        let mut offsets = Vec::with_capacity(levels.len());
        let mut total = 0u64;
        for level in &levels {
            let offset = total.next_multiple_of(HZB_COPY_ALIGN);
            records.push(HzbLevelRecord {
                offset: (offset / WORD_BYTES) as u32,
                width: level.width(),
                height: level.height(),
                stride: (level.row_stride() / WORD_BYTES) as u32,
            });
            offsets.push(offset);
            total = offset + level.readback_bytes();
        }
        assert!(
            total <= device.limits().max_storage_buffer_binding_size,
            "a {width} by {height} depth pyramid of {total} bytes outruns the device storage binding"
        );
        let depths = GpuBuffer::new(
            device,
            "belfry hzb depths",
            total,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let level_records = GpuBuffer::new(
            device,
            "belfry hzb levels",
            levels.len() as u64 * HZB_LEVEL_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        level_records.write(queue, bytemuck::cast_slice(&records));
        let steps = dimensions
            .iter()
            .enumerate()
            .map(|(index, (level_width, level_height))| HzbStepRecord {
                source_width: if index == 0 {
                    width as f32
                } else {
                    dimensions[index - 1].0 as f32
                },
                source_height: if index == 0 {
                    height as f32
                } else {
                    dimensions[index - 1].1 as f32
                },
                target_width: *level_width as f32,
                target_height: *level_height as f32,
                vertical,
                pad0: 0,
                pad1: 0,
                pad2: 0,
            })
            .collect::<Vec<_>>();
        let alignment = device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let stride = HZB_STEP_BYTES.next_multiple_of(alignment);
        let mut bytes = vec![0u8; steps.len() * stride as usize];
        for (index, step) in steps.iter().enumerate() {
            let start = index * stride as usize;
            bytes[start..start + HZB_STEP_BYTES as usize].copy_from_slice(bytemuck::bytes_of(step));
        }
        let step_records = GpuBuffer::new(
            device,
            "belfry hzb steps",
            bytes.len() as u64,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        step_records.write(queue, &bytes);
        let base = [
            base_pipeline.bind_group(
                0,
                &[Binding::buffer(
                    program::HZB_STEP_BINDING,
                    step_records.binding(0, HZB_STEP_BYTES),
                )],
            ),
            base_pipeline.bind_group(
                1,
                &[
                    Binding::texture(program::HZB_SOURCE_BINDING, &depth),
                    Binding::sampler(program::HZB_SAMPLER_BINDING, sampler),
                ],
            ),
        ];
        let reductions = (1..levels.len())
            .map(|index| {
                [
                    reduce_pipeline.bind_group(
                        0,
                        &[Binding::buffer(
                            program::HZB_STEP_BINDING,
                            step_records.binding(index as u64 * stride, HZB_STEP_BYTES),
                        )],
                    ),
                    reduce_pipeline.bind_group(
                        1,
                        &[
                            Binding::texture(program::HZB_SOURCE_BINDING, &levels[index - 1]),
                            Binding::sampler(program::HZB_SAMPLER_BINDING, sampler),
                        ],
                    ),
                ]
            })
            .collect::<Vec<_>>();
        Self {
            width,
            height,
            depth,
            levels,
            records: level_records,
            depths,
            base,
            reductions,
            offsets,
        }
    }

    pub(crate) fn holds(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }

    pub(crate) fn depth(&self) -> &Texture {
        &self.depth
    }

    pub(crate) fn levels(&self) -> u32 {
        self.levels.len() as u32
    }

    pub(crate) fn base_width(&self) -> f32 {
        self.levels[0].width() as f32
    }

    pub(crate) fn base_height(&self) -> f32 {
        self.levels[0].height() as f32
    }

    pub(crate) fn build(
        &self,
        submission: &mut Submission,
        base_pipeline: &GraphicsPipeline,
        reduce_pipeline: &GraphicsPipeline,
    ) {
        submission.render(
            &self.levels[0],
            CLEAR,
            &[Draw::new(base_pipeline, &self.base, FULLSCREEN_VERTICES)],
        );
        submission.copy_texture(&self.levels[0], &self.depths, self.offsets[0]);
        for (index, group) in self.reductions.iter().enumerate() {
            let level = index + 1;
            submission.render(
                &self.levels[level],
                CLEAR,
                &[Draw::new(reduce_pipeline, group, FULLSCREEN_VERTICES)],
            );
            submission.copy_texture(&self.levels[level], &self.depths, self.offsets[level]);
        }
    }

    pub(crate) fn cull(
        &self,
        pipeline: &PipelineHandle,
        view: BufferBinding<'_>,
        instances: BufferBinding<'_>,
        drawn: BufferBinding<'_>,
        arguments: BufferBinding<'_>,
    ) -> BindGroup {
        pipeline.bind_group(
            0,
            &[
                Binding::buffer(program::CULL_VIEW, view),
                Binding::buffer(program::CULL_INSTANCES, instances),
                Binding::buffer(program::CULL_DRAWN, drawn),
                Binding::buffer(program::CULL_ARGUMENTS, arguments),
                Binding::buffer(
                    program::CULL_LEVELS,
                    self.records
                        .binding(0, self.levels.len() as u64 * HZB_LEVEL_BYTES),
                ),
                Binding::buffer(
                    program::CULL_DEPTHS,
                    self.depths.binding(0, self.depths.size()),
                ),
            ],
        )
    }
}

const _: () = assert!(
    HZB_LEVEL_BYTES == size_of::<HzbLevelRecord>() as u64,
    "the depth level record keeps the layout its shader reads",
);
const _: () = assert!(
    HZB_LEVEL.size as u64 == HZB_LEVEL_BYTES,
    "the depth level layout spans its record",
);
const _: () = assert!(
    HZB_STEP_BYTES == size_of::<HzbStepRecord>() as u64,
    "the depth step record keeps the layout its shader reads",
);
const _: () = assert!(
    HZB_STEP.size as u64 == HZB_STEP_BYTES,
    "the depth step layout spans its record",
);
