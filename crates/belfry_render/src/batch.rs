use crate::abi::SHADOW_CASCADES;
use crate::program;
use belfry_gpu_device::{
    BindGroup, Binding, BufferBinding, BufferRange, GraphicsPipeline, SamplerTable, TextureTable,
};

const CASCADES: usize = SHADOW_CASCADES as usize;

pub(crate) struct Pipelines<'a> {
    pub(crate) depth: &'a GraphicsPipeline,
    pub(crate) opaque: &'a GraphicsPipeline,
    pub(crate) capture: &'a GraphicsPipeline,
    pub(crate) blended: &'a GraphicsPipeline,
    pub(crate) motion: &'a GraphicsPipeline,
    pub(crate) shadow: &'a GraphicsPipeline,
}

pub(crate) struct Shared<'a> {
    pub(crate) view: BufferBinding<'a>,
    pub(crate) motion_view: BufferBinding<'a>,
    pub(crate) palette: BufferBinding<'a>,
    pub(crate) materials: BufferBinding<'a>,
    pub(crate) light: BufferBinding<'a>,
    pub(crate) cascades: BufferBinding<'a>,
    pub(crate) lighting: BufferBinding<'a>,
    pub(crate) probes: BufferBinding<'a>,
    pub(crate) fragments: BufferBinding<'a>,
    pub(crate) counts: BufferBinding<'a>,
    pub(crate) dropped: BufferBinding<'a>,
    pub(crate) settings: BufferBinding<'a>,
}

pub(crate) struct Slot<'a> {
    pub(crate) vertices: BufferBinding<'a>,
    pub(crate) skins: BufferBinding<'a>,
    pub(crate) depth_drawn: BufferBinding<'a>,
    pub(crate) main_drawn: BufferBinding<'a>,
    pub(crate) shadow_drawn: [BufferBinding<'a>; CASCADES],
}

pub(crate) struct Inputs<'a> {
    pub(crate) shared: Shared<'a>,
    pub(crate) slots: &'a [Slot<'a>],
    pub(crate) opaque: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SharedKey {
    view: BufferRange,
    motion_view: BufferRange,
    palette: BufferRange,
    materials: BufferRange,
    light: BufferRange,
    cascades: BufferRange,
    lighting: BufferRange,
    probes: BufferRange,
    fragments: BufferRange,
    counts: BufferRange,
    dropped: BufferRange,
    settings: BufferRange,
}

impl SharedKey {
    fn of(shared: &Shared<'_>) -> Self {
        Self {
            view: shared.view.range(),
            motion_view: shared.motion_view.range(),
            palette: shared.palette.range(),
            materials: shared.materials.range(),
            light: shared.light.range(),
            cascades: shared.cascades.range(),
            lighting: shared.lighting.range(),
            probes: shared.probes.range(),
            fragments: shared.fragments.range(),
            counts: shared.counts.range(),
            dropped: shared.dropped.range(),
            settings: shared.settings.range(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SlotKey {
    vertices: BufferRange,
    skins: BufferRange,
    depth_drawn: BufferRange,
    main_drawn: BufferRange,
    shadow_drawn: [BufferRange; CASCADES],
    blended: bool,
}

impl SlotKey {
    fn of(slot: &Slot<'_>, blended: bool) -> Self {
        Self {
            vertices: slot.vertices.range(),
            skins: slot.skins.range(),
            depth_drawn: slot.depth_drawn.range(),
            main_drawn: slot.main_drawn.range(),
            shadow_drawn: slot.shadow_drawn.map(|drawn| drawn.range()),
            blended,
        }
    }
}

pub(crate) struct SlotGroups {
    key: SlotKey,
    depth: Option<BindGroup>,
    shade: BindGroup,
    motion: BindGroup,
    shadow: [BindGroup; CASCADES],
}

impl SlotGroups {
    fn build(
        pipelines: &Pipelines<'_>,
        shared: &Shared<'_>,
        slot: &Slot<'_>,
        key: SlotKey,
    ) -> Self {
        let depth = (!key.blended).then(|| {
            pipelines.depth.bind_group(
                0,
                &[
                    Binding::buffer(program::DEPTH_VIEW, shared.view),
                    Binding::buffer(program::DEPTH_PALETTE, shared.palette),
                    Binding::buffer(program::DEPTH_SKINS, slot.skins),
                    Binding::buffer(program::DEPTH_DRAWN, slot.depth_drawn),
                    Binding::buffer(program::DEPTH_VERTICES, slot.vertices),
                    Binding::buffer(program::DEPTH_MATERIALS, shared.materials),
                ],
            )
        });
        let pipeline = if key.blended {
            pipelines.blended
        } else {
            pipelines.opaque
        };
        let shade = pipeline.bind_group(
            0,
            &[
                Binding::buffer(program::DRAW_LIGHT, shared.light),
                Binding::buffer(program::DRAW_CASCADES, shared.cascades),
                Binding::buffer(program::DRAW_DRAWN, slot.main_drawn),
                Binding::buffer(program::DRAW_MATERIALS, shared.materials),
                Binding::buffer(program::DRAW_VIEW, shared.view),
                Binding::buffer(program::DRAW_PALETTE, shared.palette),
                Binding::buffer(program::DRAW_SKINS, slot.skins),
                Binding::buffer(program::DRAW_FRAGMENTS, shared.fragments),
                Binding::buffer(program::DRAW_COUNTS, shared.counts),
                Binding::buffer(program::DRAW_DROPPED, shared.dropped),
                Binding::buffer(program::DRAW_SETTINGS, shared.settings),
                Binding::buffer(program::DRAW_PROBES, shared.probes),
                Binding::buffer(program::DRAW_VERTICES, slot.vertices),
                Binding::buffer(program::DRAW_LIGHTS, shared.lighting),
            ],
        );
        let motion = pipelines.motion.bind_group(
            0,
            &[
                Binding::buffer(program::MOTION_VIEW, shared.motion_view),
                Binding::buffer(program::MOTION_DRAWN, slot.main_drawn),
                Binding::buffer(program::MOTION_PALETTE, shared.palette),
                Binding::buffer(program::MOTION_SKINS, slot.skins),
                Binding::buffer(program::MOTION_MATERIALS, shared.materials),
                Binding::buffer(program::MOTION_VERTICES, slot.vertices),
            ],
        );
        let shadow = std::array::from_fn(|cascade| {
            pipelines.shadow.bind_group(
                0,
                &[
                    Binding::buffer(program::SHADOW_PALETTE, shared.palette),
                    Binding::buffer(program::SHADOW_SKINS, slot.skins),
                    Binding::buffer(program::SHADOW_DRAWN, slot.shadow_drawn[cascade]),
                    Binding::buffer(program::SHADOW_VERTICES, slot.vertices),
                    Binding::buffer(program::SHADOW_MATERIALS, shared.materials),
                ],
            )
        });
        Self {
            key,
            depth,
            shade,
            motion,
            shadow,
        }
    }
}

pub(crate) struct Groups {
    shared: Option<SharedKey>,
    slots: Vec<SlotGroups>,
}

impl Groups {
    pub(crate) fn new() -> Self {
        Self {
            shared: None,
            slots: Vec::new(),
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.shared = None;
        self.slots.clear();
    }

    pub(crate) fn prepare(&mut self, pipelines: &Pipelines<'_>, inputs: &Inputs<'_>) {
        let shared = SharedKey::of(&inputs.shared);
        if self.shared != Some(shared) {
            self.shared = Some(shared);
            self.slots.clear();
        }
        let mut held = std::mem::take(&mut self.slots)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        let mut slots = Vec::with_capacity(inputs.slots.len());
        for (index, slot) in inputs.slots.iter().enumerate() {
            let key = SlotKey::of(slot, index >= inputs.opaque as usize);
            let recycled = held
                .get_mut(index)
                .and_then(Option::take)
                .filter(|groups| groups.key == key);
            slots.push(match recycled {
                Some(groups) => groups,
                None => SlotGroups::build(pipelines, &inputs.shared, slot, key),
            });
        }
        self.slots = slots;
    }

    pub(crate) fn at(&self, index: usize) -> &SlotGroups {
        &self.slots[index]
    }
}

impl SlotGroups {
    pub(crate) fn depth(&self) -> Option<&BindGroup> {
        self.depth.as_ref()
    }

    pub(crate) fn shade(&self) -> &BindGroup {
        &self.shade
    }

    pub(crate) fn motion(&self) -> &BindGroup {
        &self.motion
    }

    pub(crate) fn shadow(&self, cascade: usize) -> &BindGroup {
        &self.shadow[cascade]
    }
}

pub(crate) struct Tables {
    depth: Option<BindGroup>,
    opaque: Option<BindGroup>,
    capture: Option<BindGroup>,
    blended: Option<BindGroup>,
    motion: Option<BindGroup>,
    shadow: Option<BindGroup>,
}

impl Tables {
    pub(crate) fn new() -> Self {
        Self {
            depth: None,
            opaque: None,
            capture: None,
            blended: None,
            motion: None,
            shadow: None,
        }
    }

    pub(crate) fn invalidate(&mut self) {
        *self = Self::new();
    }

    pub(crate) fn capture(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.capture,
            pipelines.capture,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        )
    }

    pub(crate) fn depth(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.depth,
            pipelines.depth,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        )
    }

    pub(crate) fn opaque(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.opaque,
            pipelines.opaque,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        )
    }

    pub(crate) fn blended(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.blended,
            pipelines.blended,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        )
    }

    pub(crate) fn motion(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.motion,
            pipelines.motion,
            program::MOTION_TEXTURE,
            program::MOTION_SAMPLER,
            textures,
            samplers,
        )
    }

    pub(crate) fn shadow(
        &mut self,
        pipelines: &Pipelines<'_>,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        Self::held(
            &mut self.shadow,
            pipelines.shadow,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        )
    }

    fn held(
        slot: &mut Option<BindGroup>,
        pipeline: &GraphicsPipeline,
        texture: u32,
        sampler: u32,
        textures: &TextureTable,
        samplers: &SamplerTable,
    ) -> BindGroup {
        slot.get_or_insert_with(|| {
            pipeline.bind_group(
                1,
                &[
                    Binding::texture_table(texture, textures),
                    Binding::sampler_table(sampler, samplers),
                ],
            )
        })
        .clone()
    }
}
