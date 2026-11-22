use crate::environment::Environment;
use crate::frame::{
    Batch, Placement, Screen, Skinning, ambient_of, arguments_of, bases_of, camera_of,
    direction_record, light_of, record_of, record_of_cascade, record_of_main_view, record_of_view,
    vector,
};
use crate::growing::Growing;
use crate::hidden::Hidden;
use crate::hzb::Hzb;
use crate::instance::MeshInstance;
use crate::light::PointLight;
use crate::line::Lines;
use crate::material::{AlphaMode, Material, MaterialDesc, MaterialLibrary};
use crate::mesh::{Mesh, MeshLibrary};
use crate::pick::{PickBatch, PickFrame, PickResources, Picker};
use crate::post::Post;
use crate::probe::Probes;
use crate::program;
use crate::shadow;
use crate::skin::Skin;
use crate::span;
use crate::transparency::Transparency;
use crate::view::View;
use belfry_ecs::{Entity, World};
use belfry_environment::BRDF_SIZE;
use belfry_gpu_compiler::Fvec4;
use belfry_gpu_device::{
    AddressMode, BindGroup, Binding, BufferUsages, Device, Draw, Filter, GpuBuffer, GpuContext,
    GraphicsPipeline, PipelineHandle, PipelineState, Queue, Sampler, SamplerDesc, SamplerTable,
    Submission, SurfaceFrame, Texture, TextureFormat, TextureTable, TextureUsages, Viewport,
};
use belfry_math::{Mat4, Transform, Vec2, Vec3};
use belfry_render_abi::{
    ARGUMENT_WORDS, CascadeRecord, DebugLineRecord, DrawRecord, InstanceRecord, JOINT, JointRecord,
    LIGHT, LIGHT_CAPACITY, LIGHT_SLICES, LIGHT_STRIDE, LIGHT_TILE, LightRecord, MATERIAL,
    MaterialRecord, NO_HIGHLIGHT, POINT_LIGHT, PointLightRecord, SHADOW_ATLAS,
    SHADOW_ATLAS_COLUMNS, SHADOW_CASCADES, SHADOW_SIDE, SkinVertexRecord, SkyRecord, VERTEX_FLOATS,
    ViewRecord, WORKGROUP,
};
use half::f16;
use std::mem::size_of;

const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const LIGHT_BYTES: u64 = size_of::<LightRecord>() as u64;
const CASCADE_BYTES: u64 = size_of::<CascadeRecord>() as u64;
const INSTANCE_BYTES: u64 = size_of::<InstanceRecord>() as u64;
const DRAW_BYTES: u64 = size_of::<DrawRecord>() as u64;
const MATERIAL_BYTES: u64 = size_of::<MaterialRecord>() as u64;
const POINT_LIGHT_BYTES: u64 = size_of::<PointLightRecord>() as u64;
const CLUSTER_BYTES: u64 = LIGHT_STRIDE as u64 * WORD_BYTES;
const ARGUMENT_BYTES: u64 = (ARGUMENT_WORDS * 4) as u64;
const ARGUMENT_USIZE: usize = ARGUMENT_BYTES as usize;
const WORD_BYTES: u64 = 4;
const VERTEX_BYTES: u64 = (VERTEX_FLOATS * 4) as u64;
const JOINT_BYTES: u64 = size_of::<JointRecord>() as u64;
const DEBUG_LINE_BYTES: u64 = size_of::<DebugLineRecord>() as u64;
const SKY_BYTES: u64 = size_of::<SkyRecord>() as u64;
const SKY_VERTICES: u32 = 3;
const CASCADE_COUNT: u32 = SHADOW_CASCADES;

pub enum Target<'a> {
    Texture(&'a Texture),
    Surface(SurfaceFrame),
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    cull: PipelineHandle,
    light_clusters: PipelineHandle,
    draw: GraphicsPipeline,
    transparent_draw: GraphicsPipeline,
    sky: GraphicsPipeline,
    sky_view: BindGroup,
    sky_record: GpuBuffer,
    sky_shade: BindGroup,
    line_pipeline: GraphicsPipeline,
    post: Post,
    shadow: GraphicsPipeline,
    depth: GraphicsPipeline,
    hzb_base: GraphicsPipeline,
    hzb_reduce: GraphicsPipeline,
    hzb_sampler: Sampler,
    hzb: Option<Hzb>,
    meshes: MeshLibrary,
    materials: MaterialLibrary,
    material_records: Growing,
    materials_dirty: bool,
    tables: Option<BindGroup>,
    transparent_tables: Option<BindGroup>,
    depth_tables: Option<BindGroup>,
    shadow_tables: Option<BindGroup>,
    shade: BindGroup,
    transparent_shade: BindGroup,
    shadow_map: Texture,
    views: GpuBuffer,
    shadow_views: GpuBuffer,
    light: GpuBuffer,
    shadow_sampler: Sampler,
    brdf: Texture,
    brdf_sampler: Sampler,
    radiance: Texture,
    radiance_sampler: Sampler,
    ambient: Option<(usize, usize, usize)>,
    cascades: GpuBuffer,
    instances: Growing,
    depth_drawn: Growing,
    depth_arguments: Growing,
    main_drawn: Growing,
    main_arguments: Growing,
    main_batches: u32,
    shadow_drawn: Growing,
    shadow_arguments: Growing,
    shadow_span: u64,
    lighting: Growing,
    joints: Growing,
    line_records: Growing,
    picker: Picker,
    pick_frame: Option<PickFrame>,
    transparency: Transparency,
    probes: Probes,
}

impl Renderer {
    pub fn new(context: &GpuContext, format: TextureFormat) -> Self {
        assert!(
            matches!(
                format,
                TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm
            ),
            "a renderer resolves tone mapped frames into an eight bit target",
        );
        let device = context.device().clone();
        let queue = Queue::of(&device);
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let shadow_map = Texture::new(
            &device,
            "belfry shadow map",
            SHADOW_ATLAS,
            SHADOW_ATLAS,
            1,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED,
        );
        let shadow_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Nearest,
                mag_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        assert!(
            device.capability().bindless,
            "the renderer needs a device whose shaders index resource tables"
        );
        let limits = device.limits();
        assert!(
            belfry_gpu_compiler::TEXTURE_TABLE_CAPACITY
                <= limits.max_sampled_images_per_shader_stage,
            "the renderer material table of {} images outruns the device limit of {}",
            belfry_gpu_compiler::TEXTURE_TABLE_CAPACITY,
            limits.max_sampled_images_per_shader_stage,
        );
        assert!(
            belfry_gpu_compiler::SAMPLER_TABLE_CAPACITY <= limits.max_samplers_per_shader_stage,
            "the renderer material table of {} samplers outruns the device limit of {}",
            belfry_gpu_compiler::SAMPLER_TABLE_CAPACITY,
            limits.max_samplers_per_shader_stage,
        );
        let draw = GraphicsPipeline::new(
            &device,
            program::instancing(),
            TextureFormat::Rgba16Float,
            PipelineState::DEPTH_WRITE,
        );
        let transparent_draw = GraphicsPipeline::new(
            &device,
            program::instancing(),
            TextureFormat::Rgba16Float,
            PipelineState::TRANSPARENT,
        );
        let line_pipeline = GraphicsPipeline::new(
            &device,
            program::debug_lines(),
            TextureFormat::Rgba16Float,
            PipelineState::DEBUG,
        );
        let sky = GraphicsPipeline::new(
            &device,
            program::sky_program(),
            TextureFormat::Rgba16Float,
            PipelineState::SCREEN,
        );
        let shadow = GraphicsPipeline::new(
            &device,
            program::shadowing(),
            TextureFormat::R32Float,
            PipelineState::DEPTH_WRITE,
        );
        let depth = GraphicsPipeline::new(
            &device,
            program::depth_prepass(),
            TextureFormat::R32Float,
            PipelineState::DEPTH_WRITE,
        );
        let hzb_base = GraphicsPipeline::new(
            &device,
            program::hzb_base(),
            TextureFormat::R32Float,
            PipelineState::SCREEN,
        );
        let hzb_reduce = GraphicsPipeline::new(
            &device,
            program::hzb_reduce(),
            TextureFormat::R32Float,
            PipelineState::SCREEN,
        );
        let hzb_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Nearest,
                mag_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let brdf = Texture::new(
            &device,
            "belfry brdf",
            BRDF_SIZE,
            BRDF_SIZE,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        let table = belfry_environment::brdf(BRDF_SIZE);
        let texels = table
            .iter()
            .flat_map(|texel| texel.iter())
            .map(|value| f16::from_f32(*value).to_bits())
            .collect::<Vec<u16>>();
        brdf.write(&queue, bytemuck::cast_slice(&texels));
        let brdf_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let radiance = Texture::new(
            &device,
            "belfry radiance",
            1,
            1,
            1,
            TextureFormat::Rgba16Float,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        let clear = [0u16, 0u16, 0u16, f16::from_f32(1.0).to_bits()];
        radiance.write(&queue, bytemuck::cast_slice(&clear));
        let radiance_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mip_filter: Filter::Linear,
                address_u: AddressMode::Repeat,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let ambient = Some((
            radiance.identity(),
            radiance.identity(),
            radiance_sampler.identity(),
        ));
        let probes = Probes::new(&device, &radiance, &radiance_sampler);
        let sky_record = GpuBuffer::new(
            &device,
            "belfry sky",
            SKY_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let sky_view = sky.bind_group(
            0,
            &[Binding::buffer(
                program::SKY_VIEW,
                sky_record.binding(0, SKY_BYTES),
            )],
        );
        let shadows = (&shadow_map, &shadow_sampler);
        let lut = (&brdf, &brdf_sampler);
        let probe_tables = (probes.textures(), probes.samplers());
        let shade = Self::shaded_group(
            &draw,
            shadows,
            (&radiance, &radiance_sampler),
            lut,
            probe_tables,
        );
        let transparent_shade = Self::shaded_group(
            &transparent_draw,
            shadows,
            (&radiance, &radiance_sampler),
            lut,
            probe_tables,
        );
        let sky_shade = Self::sky_group(&sky, (&radiance, &radiance_sampler));
        let cull = context.declare(program::culling());
        let light_clusters = context.declare(program::clustering());
        let post = Post::new(&device, format);
        let meshes = MeshLibrary::new(&device);
        let materials = MaterialLibrary::new(&device, &queue);
        let views = GpuBuffer::new(&device, "belfry views", 2 * VIEW_BYTES, storage);
        let shadow_views = GpuBuffer::new(
            &device,
            "belfry shadow views",
            u64::from(CASCADE_COUNT) * VIEW_BYTES,
            storage,
        );
        let light = GpuBuffer::new(&device, "belfry light", LIGHT_BYTES, storage);
        let cascades = GpuBuffer::new(
            &device,
            "belfry cascades",
            u64::from(CASCADE_COUNT) * CASCADE_BYTES,
            storage,
        );
        let instances = Growing::new(&device, "belfry instances", storage);
        let depth_drawn = Growing::new(&device, "belfry depth drawn", storage);
        let depth_arguments = Growing::new(
            &device,
            "belfry depth arguments",
            storage | BufferUsages::INDIRECT,
        );
        let main_drawn = Growing::new(&device, "belfry drawn", storage);
        let main_arguments = Growing::new(
            &device,
            "belfry arguments",
            storage | BufferUsages::INDIRECT | BufferUsages::COPY_SRC,
        );
        let shadow_drawn = Growing::new(&device, "belfry shadow drawn", storage);
        let shadow_arguments = Growing::new(
            &device,
            "belfry shadow arguments",
            storage | BufferUsages::INDIRECT,
        );
        let lighting = Growing::new(&device, "belfry lighting", storage);
        let picker = Picker::new(&device);
        Self {
            cull,
            light_clusters,
            draw,
            transparent_draw,
            sky,
            sky_view,
            sky_record,
            sky_shade,
            line_pipeline,
            post,
            shadow,
            depth,
            hzb_base,
            hzb_reduce,
            hzb_sampler,
            hzb: None,
            meshes,
            materials,
            material_records: Growing::new(&device, "belfry material records", storage),
            materials_dirty: true,
            tables: None,
            transparent_tables: None,
            depth_tables: None,
            shadow_tables: None,
            shade,
            transparent_shade,
            shadow_map,
            views,
            shadow_views,
            light,
            shadow_sampler,
            brdf,
            brdf_sampler,
            radiance,
            radiance_sampler,
            ambient,
            cascades,
            instances,
            depth_drawn,
            depth_arguments,
            main_drawn,
            main_arguments,
            main_batches: 0,
            shadow_drawn,
            shadow_arguments,
            shadow_span: 0,
            lighting,
            joints: Growing::new(&device, "belfry joints", storage),
            line_records: Growing::new(&device, "belfry debug lines", storage),
            picker,
            pick_frame: None,
            transparency: Transparency::new(&device),
            probes,
            queue,
            device,
        }
    }

    fn shaded_group(
        pipeline: &GraphicsPipeline,
        shadow: (&Texture, &Sampler),
        radiance: (&Texture, &Sampler),
        brdf: (&Texture, &Sampler),
        probes: (&TextureTable, &SamplerTable),
    ) -> BindGroup {
        pipeline.bind_group(
            2,
            &[
                Binding::texture(program::SHADOW_SHADOW, shadow.0),
                Binding::sampler(program::SHADOW_SAMPLER, shadow.1),
                Binding::texture(program::SHADE_RADIANCE, radiance.0),
                Binding::sampler(program::SHADE_RADIANCE_SAMPLER, radiance.1),
                Binding::texture(program::SHADE_BRDF, brdf.0),
                Binding::sampler(program::SHADE_BRDF_SAMPLER, brdf.1),
                Binding::texture_table(program::PROBE_TEXTURES, probes.0),
                Binding::sampler_table(program::PROBE_SAMPLERS, probes.1),
            ],
        )
    }

    fn sky_group(sky: &GraphicsPipeline, panorama: (&Texture, &Sampler)) -> BindGroup {
        sky.bind_group(
            1,
            &[
                Binding::texture(program::SKY_RADIANCE, panorama.0),
                Binding::sampler(program::SKY_SAMPLER, panorama.1),
            ],
        )
    }

    fn refresh_ambient(&mut self, environment: Option<&Environment>) {
        let (radiance, panorama, sampler) = match environment {
            Some(held) => (
                held.radiance.clone(),
                held.panorama.clone(),
                held.sampler.clone(),
            ),
            None => (
                self.radiance.clone(),
                self.radiance.clone(),
                self.radiance_sampler.clone(),
            ),
        };
        let identity = (radiance.identity(), panorama.identity(), sampler.identity());
        if self.ambient == Some(identity) {
            return;
        }
        let shadow = (&self.shadow_map, &self.shadow_sampler);
        let lut = (&self.brdf, &self.brdf_sampler);
        let probe_tables = (self.probes.textures(), self.probes.samplers());
        let shade =
            Self::shaded_group(&self.draw, shadow, (&radiance, &sampler), lut, probe_tables);
        let transparent = Self::shaded_group(
            &self.transparent_draw,
            shadow,
            (&radiance, &sampler),
            lut,
            probe_tables,
        );
        let sky = Self::sky_group(&self.sky, (&panorama, &sampler));
        self.shade = shade;
        self.transparent_shade = transparent;
        self.sky_shade = sky;
        self.ambient = Some(identity);
    }

    pub fn reset(&mut self) {
        self.meshes.reset();
        self.materials.reset();
        self.materials_dirty = true;
        self.tables = None;
        self.transparent_tables = None;
        self.depth_tables = None;
        self.shadow_tables = None;
        self.pick_frame = None;
        self.main_batches = 0;
        self.ambient = None;
        self.probes.reset();
    }

    pub fn mesh(&mut self, vertices: &[belfry_render_abi::Vertex]) -> Mesh {
        self.meshes.add(&self.queue, vertices)
    }

    pub fn skinned_mesh(
        &mut self,
        vertices: &[belfry_render_abi::Vertex],
        skin: &[SkinVertexRecord],
    ) -> Mesh {
        self.meshes.add_skinned(&self.queue, vertices, skin)
    }

    pub fn idle(&self) {
        self.queue.idle();
    }

    pub fn remove_mesh(&mut self, mesh: Mesh) {
        self.meshes.remove(&self.queue, mesh);
        self.pick_frame = None;
    }

    pub fn update_mesh(&mut self, mesh: Mesh, vertices: &[belfry_render_abi::Vertex]) {
        self.meshes.update(&self.queue, mesh, vertices, None);
        self.pick_frame = None;
    }

    pub fn update_skinned_mesh(
        &mut self,
        mesh: Mesh,
        vertices: &[belfry_render_abi::Vertex],
        skin: &[SkinVertexRecord],
    ) {
        assert_eq!(
            vertices.len(),
            skin.len(),
            "a mesh of {} vertices holds {} skin weights",
            vertices.len(),
            skin.len(),
        );
        assert!(
            !skin.is_empty(),
            "a skinned mesh of no vertex names no geometry",
        );
        self.meshes.update(&self.queue, mesh, vertices, Some(skin));
    }

    pub fn texture(
        &self,
        label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Texture {
        Texture::new(
            &self.device,
            label,
            width,
            height,
            mip_levels,
            format,
            usages,
        )
    }

    pub fn write_texture(&self, texture: &Texture, levels: &[&[u8]]) {
        texture.write_levels(&self.queue, levels);
    }

    pub fn sampler(&self, desc: SamplerDesc) -> Sampler {
        Sampler::new(&self.device, desc)
    }

    pub fn material(&mut self, desc: MaterialDesc) -> Material {
        let material = self.materials.add(desc);
        self.materials_changed();
        material
    }

    pub fn update_material(&mut self, material: Material, desc: MaterialDesc) {
        self.materials.set(material, desc);
        self.materials_changed();
    }

    pub fn remove_material(&mut self, material: Material) {
        self.materials.remove(material);
        self.materials_changed();
    }

    fn materials_changed(&mut self) {
        self.materials_dirty = true;
        self.tables = None;
        self.transparent_tables = None;
        self.depth_tables = None;
        self.shadow_tables = None;
    }

    fn flush_materials(&mut self) {
        if !self.materials_dirty {
            return;
        }
        let records = self.materials.records();
        if records.is_empty() {
            self.materials_dirty = false;
            return;
        }
        self.material_records
            .fit(&self.device, records.len() as u64 * MATERIAL_BYTES);
        self.material_records
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(records));
        self.materials_dirty = false;
    }

    pub fn render(
        &mut self,
        world: &World,
        target: Target<'_>,
        clear: [f32; 4],
        highlight: Option<Entity>,
    ) {
        let (texture, frame) = match target {
            Target::Texture(texture) => (texture.clone(), None),
            Target::Surface(frame) => (frame.texture().clone(), Some(frame)),
        };
        self.flush_materials();
        let ambient = ambient_of(world);
        let (environment, environment_transform) = match ambient.global {
            Some((environment, transform)) => (Some(environment), transform),
            None => (None, Transform::IDENTITY),
        };
        let (probe_count, probe_textures) =
            self.probes
                .prepare(&self.device, &self.queue, &ambient.local);
        if probe_textures {
            self.ambient = None;
        }
        self.refresh_ambient(environment);
        let aspect = texture.width() as f32 / texture.height() as f32;
        let (camera, transform) = camera_of(world);
        let (light, light_transform) = light_of(world);
        let mut point_light_records = Vec::new();
        world.for_each_pair::<Transform, PointLight>(|entity, transform, light| {
            if world.contains::<Hidden>(entity) {
                return;
            }
            point_light_records.push(PointLightRecord {
                position: Fvec4 {
                    x: transform.translation.x,
                    y: transform.translation.y,
                    z: transform.translation.z,
                    w: light.range,
                },
                radiance: Fvec4 {
                    x: light.color.x * light.intensity,
                    y: light.color.y * light.intensity,
                    z: light.color.z * light.intensity,
                    w: 0.0,
                },
            });
        });
        let point_light_count = point_light_records.len() as u32;
        let eye = transform.translation;
        let forward = transform.rotation.rotate_vec3(-Vec3::Z).normalize();
        let view = camera.view(transform, aspect, self.device.clip_space());
        let mut gathered = Vec::new();
        world.for_each_pair::<Transform, MeshInstance>(|entity, transform, instance| {
            if world.contains::<Hidden>(entity) {
                return;
            }
            gathered.push((
                instance.mesh,
                instance.material,
                entity,
                *transform,
                *instance,
                self.materials.mode(instance.material) == AlphaMode::Blend,
            ));
        });
        let mut opaque = Vec::new();
        let mut transparent = Vec::new();
        for held in gathered {
            if held.5 {
                transparent.push(held);
            } else {
                opaque.push(held);
            }
        }
        opaque.sort_by_key(|(mesh, _, entity, ..)| (mesh.index(), entity.index()));
        transparent.sort_by_key(|(mesh, _, entity, ..)| (mesh.index(), entity.index()));
        let depth_count = opaque.len() as u32;
        let mut gathered = opaque;
        gathered.extend(transparent);
        let mut batches = Vec::new();
        let mut batch_of = Vec::with_capacity(gathered.len());
        let mut previous = None;
        let mut depth_batches = 0u32;
        for (mesh, _, _, _, _, blended) in &gathered {
            if previous != Some((*mesh, *blended)) {
                previous = Some((*mesh, *blended));
                batches.push(Batch {
                    mesh: *mesh,
                    count: 0,
                });
                if !blended {
                    depth_batches += 1;
                }
            }
            let batch = batches.len() - 1;
            batches[batch].count += 1;
            batch_of.push(batch as u32);
        }
        let counts = batches.iter().map(|batch| batch.count).collect::<Vec<_>>();
        let total = gathered.len() as u32;
        let highlight = highlight
            .and_then(|held| {
                gathered
                    .iter()
                    .position(|(_, _, entity, ..)| *entity == held)
            })
            .map_or(NO_HIGHLIGHT, |at| at as u32);
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let (bases, slots) = bases_of(&counts, span::blocks(alignment, DRAW_BYTES));
        let mut palette = Vec::new();
        let mut skin_bases = Vec::with_capacity(gathered.len());
        for (_, _, entity, transform, instance, _) in &gathered {
            let geometry = self.meshes.geometry(instance.mesh);
            match world.get::<Skin>(*entity) {
                Some(skin) => {
                    assert!(
                        geometry.skin_count > 0,
                        "a skinned instance renders a mesh without skin weights",
                    );
                    assert_eq!(
                        skin.joints().len(),
                        skin.inverse_binds().len(),
                        "a skin holds {} joints for {} inverse bind matrices",
                        skin.joints().len(),
                        skin.inverse_binds().len(),
                    );
                    let first = palette.len() as u32;
                    let model = transform.matrix();
                    let inverse = model.inverse();
                    for (joint, bind) in skin.joints().iter().zip(skin.inverse_binds()) {
                        let joint = joint_world(world, *joint);
                        palette.push(inverse * joint * *bind);
                    }
                    skin_bases.push(Skinning {
                        palette: first,
                        joints: skin.joints().len() as u32,
                    });
                }
                None => skin_bases.push(Skinning {
                    palette: 0,
                    joints: 0,
                }),
            }
        }
        let joint_bytes = u64::from(palette.len().max(1) as u32) * JOINT_BYTES;
        self.joints.fit(&self.device, joint_bytes);
        if !palette.is_empty() {
            let records = palette.iter().map(joint_record).collect::<Vec<_>>();
            self.joints
                .buffer()
                .write(&self.queue, bytemuck::cast_slice(&records));
        }
        let mut records = Vec::with_capacity(gathered.len());
        for (index, (_, _, entity, transform, instance, _)) in gathered.iter().enumerate() {
            let geometry = self.meshes.geometry(instance.mesh);
            let skinning = skin_bases[index];
            let bounds = if skinning.joints > 0 {
                let at = skinning.palette as usize;
                let held = &palette[at..at + skinning.joints as usize];
                skinned_bounds(geometry, *transform, held)
            } else {
                static_bounds(geometry, *transform)
            };
            records.push(record_of(
                *entity,
                *transform,
                instance,
                bounds,
                Placement {
                    batch: batch_of[index],
                    base: bases[batch_of[index] as usize],
                    skinning,
                    hzb: index < depth_count as usize,
                },
            ));
        }
        let spheres = records
            .iter()
            .map(|record| {
                (
                    Vec3::new(record.bounds.x, record.bounds.y, record.bounds.z),
                    record.bounds.w,
                )
            })
            .collect::<Vec<_>>();
        let cascades = shadow::cascades(
            camera,
            transform,
            aspect,
            light,
            light_transform,
            &spheres,
            self.device.clip_space(),
        );
        let direction = light_transform.rotation.rotate_vec3(-Vec3::Z).normalize();
        let radiance = light.color * light.intensity;
        let vertical = crate::frame::vertical(&self.device);
        let (hzb_levels, hzb_width, hzb_height) = if total == 0 {
            (0u32, 0.0, 0.0)
        } else {
            self.ensure_hzb(texture.width(), texture.height(), vertical);
            let hzb = self
                .hzb
                .as_ref()
                .expect("a rendered frame owns its depth pyramid");
            (hzb.levels(), hzb.base_width(), hzb.base_height())
        };
        let cluster_x = texture.width().div_ceil(LIGHT_TILE);
        let cluster_y = texture.height().div_ceil(LIGHT_TILE);
        let cluster_count = cluster_x * cluster_y * LIGHT_SLICES;
        let cluster_bytes = u64::from(cluster_count) * CLUSTER_BYTES;
        assert!(
            cluster_bytes <= self.device.limits().max_storage_buffer_binding_size,
            "a {} by {} frame of {cluster_bytes} cluster bytes outruns the device storage binding",
            texture.width(),
            texture.height(),
        );
        assert!(
            cluster_x * cluster_y <= self.device.limits().max_compute_workgroups_per_dimension,
            "a {} by {} frame dispatches {} light tiles beyond the device grid",
            texture.width(),
            texture.height(),
            cluster_x * cluster_y,
        );
        let point_bytes = u64::from(point_light_count) * POINT_LIGHT_BYTES;
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let cluster_offset_bytes = point_bytes.next_multiple_of(alignment);
        let cluster_offset = (cluster_offset_bytes / WORD_BYTES) as u32;
        let lighting_bytes = cluster_offset_bytes + cluster_bytes;
        assert!(
            lighting_bytes <= self.device.limits().max_storage_buffer_binding_size,
            "a {} by {} frame of {lighting_bytes} lighting bytes outruns the device storage binding",
            texture.width(),
            texture.height(),
        );
        self.lighting
            .fit(&self.device, lighting_bytes.max(POINT_LIGHT_BYTES));
        self.lighting
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(&point_light_records));
        let main_view = record_of_main_view(
            &view,
            total,
            eye,
            forward,
            camera,
            Screen {
                width: texture.width(),
                height: texture.height(),
                vertical,
                hzb_levels,
                hzb_width,
                hzb_height,
                cluster_x,
                cluster_y,
                point_lights: point_light_count,
                cluster_offset,
                highlight,
            },
        );
        let depth_view = ViewRecord {
            hzb_levels: 0,
            instances: depth_count,
            ..main_view
        };
        self.views
            .write(&self.queue, bytemuck::cast_slice(&[main_view, depth_view]));
        self.shadow_views.write(
            &self.queue,
            bytemuck::cast_slice(
                &cascades
                    .iter()
                    .map(|cascade| {
                        record_of_view(&View::of(cascade.clip), depth_count, Vec3::ZERO, Vec3::Z)
                    })
                    .collect::<Vec<_>>(),
            ),
        );
        self.cascades.write(
            &self.queue,
            bytemuck::cast_slice(&cascades.iter().map(record_of_cascade).collect::<Vec<_>>()),
        );
        let irradiance = environment.map_or([Vec3::ZERO; 9], |held| held.irradiance);
        let intensity = environment.map_or(0.0, |held| held.intensity);
        let levels = environment.map_or(0.0, |held| (held.levels - 1) as f32);
        let environment_rotation =
            Mat4::from_quat(environment_transform.rotation.normalize().inverse());
        self.light.write(
            &self.queue,
            bytemuck::bytes_of(&LightRecord {
                direction: direction_record(direction),
                radiance: vector(radiance.extend(0.0)),
                irradiance0: vector(irradiance[0].extend(0.0)),
                irradiance1: vector(irradiance[1].extend(0.0)),
                irradiance2: vector(irradiance[2].extend(0.0)),
                irradiance3: vector(irradiance[3].extend(0.0)),
                irradiance4: vector(irradiance[4].extend(0.0)),
                irradiance5: vector(irradiance[5].extend(0.0)),
                irradiance6: vector(irradiance[6].extend(0.0)),
                irradiance7: vector(irradiance[7].extend(0.0)),
                irradiance8: vector(irradiance[8].extend(0.0)),
                environment0: vector(environment_rotation.col(0)),
                environment1: vector(environment_rotation.col(1)),
                environment2: vector(environment_rotation.col(2)),
                intensity,
                levels,
                vertical,
                texel: 1.0 / SHADOW_ATLAS as f32,
                probes: probe_count,
                pad0: 0,
                pad1: 0,
                pad2: 0,
            }),
        );
        let sky = environment.filter(|held| held.background);
        if let Some(held) = sky {
            let inverse = view.clip.inverse();
            self.sky_record.write(
                &self.queue,
                bytemuck::bytes_of(&SkyRecord {
                    inverse0: vector(inverse.col(0)),
                    inverse1: vector(inverse.col(1)),
                    inverse2: vector(inverse.col(2)),
                    inverse3: vector(inverse.col(3)),
                    eye: vector(eye.extend(0.0)),
                    environment0: vector(environment_rotation.col(0)),
                    environment1: vector(environment_rotation.col(1)),
                    environment2: vector(environment_rotation.col(2)),
                    intensity: held.intensity,
                    pad0: 0,
                    pad1: 0,
                    pad2: 0,
                }),
            );
        }
        self.post
            .ensure(&self.device, &self.queue, texture.width(), texture.height());
        self.transparency.ensure(
            &self.device,
            &self.queue,
            texture.width(),
            texture.height(),
            vertical,
        );
        let debug = world.resource::<Lines>().filter(|lines| !lines.is_empty());
        let line_segments = debug.map_or(0, Lines::len);
        let mut line_groups = Vec::new();
        if let Some(lines) = debug {
            let bytes = line_segments as u64 * DEBUG_LINE_BYTES;
            self.line_records.fit(&self.device, bytes);
            let records = lines
                .lines()
                .iter()
                .map(|line| DebugLineRecord {
                    start: vector(line.start.extend(1.0)),
                    end: vector(line.end.extend(1.0)),
                    color: vector(line.color),
                })
                .collect::<Vec<_>>();
            self.line_records
                .buffer()
                .write(&self.queue, bytemuck::cast_slice(&records));
            line_groups.push(self.line_pipeline.bind_group(
                0,
                &[
                    Binding::buffer(program::LINE_VIEW, self.views.binding(0, VIEW_BYTES)),
                    Binding::buffer(
                        program::LINE_SEGMENTS,
                        self.line_records.buffer().binding(0, bytes),
                    ),
                ],
            ));
        }
        let line_vertices = line_segments as u32 * 6;
        let mut submission = Submission::new(&self.device, "belfry frame");
        if total == 0 {
            self.main_batches = 0;
            self.pick_frame = None;
            let sky_groups = [self.sky_view.clone(), self.sky_shade.clone()];
            let mut debug_draws = Vec::new();
            if sky.is_some() {
                debug_draws.push(Draw::new(&self.sky, &sky_groups, SKY_VERTICES));
            }
            if !line_groups.is_empty() {
                debug_draws.push(Draw::new(&self.line_pipeline, &line_groups, line_vertices));
            }
            self.transparency.clear(&mut submission);
            submission.render(
                self.post.hdr(),
                [clear[0], clear[1], clear[2], 0.0],
                &debug_draws,
            );
            self.transparency
                .resolve(&mut submission, self.post.hdr(), self.post.lit());
            self.post.resolve(&mut submission);
            self.post.tonemap(&mut submission, &texture);
            if let Some(frame) = frame {
                submission.present(frame);
            }
            submission.submit(&self.queue);
            return;
        }
        self.pick_frame = Some(PickFrame {
            clip: view.clip,
            width: texture.width(),
            height: texture.height(),
            batches: batches
                .iter()
                .zip(&bases)
                .map(|(batch, base)| PickBatch {
                    mesh: batch.mesh,
                    base: *base,
                    count: batch.count,
                })
                .collect(),
            entities: gathered.iter().map(|(_, _, entity, ..)| *entity).collect(),
        });
        submission.clear(self.lighting.buffer(), cluster_offset_bytes, cluster_bytes);
        if point_light_count > 0 {
            let group = self.light_clusters.bind_group(
                0,
                &[
                    Binding::buffer(program::CLUSTER_VIEW, self.views.binding(0, VIEW_BYTES)),
                    Binding::buffer(
                        program::CLUSTER_POINTS,
                        self.lighting
                            .buffer()
                            .binding(0, u64::from(point_light_count) * POINT_LIGHT_BYTES),
                    ),
                    Binding::buffer(
                        program::CLUSTER_GRID,
                        self.lighting
                            .buffer()
                            .binding(cluster_offset_bytes, cluster_bytes),
                    ),
                ],
            );
            submission.dispatch(
                &self.light_clusters,
                &group,
                [cluster_x * cluster_y, LIGHT_SLICES, 1],
            );
        }
        let arguments = batches
            .iter()
            .flat_map(|batch| arguments_of(self.meshes.geometry(batch.mesh)))
            .collect::<Vec<u32>>();
        assert!(
            records
                .iter()
                .all(|record| record.material < self.materials.records().len() as u32),
            "a rendered instance names a material this renderer does not hold"
        );
        self.upload(&records, &arguments, slots);
        self.main_batches = batches.len() as u32;
        let hzb = self
            .hzb
            .as_ref()
            .expect("a rendered frame owns its depth pyramid");
        let instances = self
            .instances
            .buffer()
            .binding(0, u64::from(total) * INSTANCE_BYTES);
        let depth_cull = hzb.cull(
            &self.cull,
            self.views.binding(VIEW_BYTES, VIEW_BYTES),
            instances,
            self.depth_drawn
                .buffer()
                .binding(0, u64::from(slots) * DRAW_BYTES),
            self.depth_arguments
                .buffer()
                .binding(0, arguments.len() as u64 * WORD_BYTES),
        );
        let main_cull = hzb.cull(
            &self.cull,
            self.views.binding(0, VIEW_BYTES),
            instances,
            self.main_drawn
                .buffer()
                .binding(0, u64::from(slots) * DRAW_BYTES),
            self.main_arguments
                .buffer()
                .binding(0, arguments.len() as u64 * WORD_BYTES),
        );
        let mut shadow_cull = Vec::with_capacity(CASCADE_COUNT as usize);
        for cascade in 0..CASCADE_COUNT {
            shadow_cull.push(
                hzb.cull(
                    &self.cull,
                    self.shadow_views
                        .binding(u64::from(cascade) * VIEW_BYTES, VIEW_BYTES),
                    instances,
                    self.shadow_drawn.buffer().binding(
                        u64::from(cascade) * u64::from(slots) * DRAW_BYTES,
                        u64::from(slots) * DRAW_BYTES,
                    ),
                    self.shadow_arguments.buffer().binding(
                        u64::from(cascade) * self.shadow_span,
                        arguments.len() as u64 * WORD_BYTES,
                    ),
                ),
            );
        }
        submission.dispatch(&self.cull, &depth_cull, [total.div_ceil(WORKGROUP), 1, 1]);
        for group in &shadow_cull {
            submission.dispatch(&self.cull, group, [total.div_ceil(WORKGROUP), 1, 1]);
        }
        let depth_tables = match &self.depth_tables {
            Some(tables) => tables.clone(),
            None => {
                let tables = self.depth.bind_group(
                    1,
                    &[
                        Binding::texture_table(program::SHADE_TEXTURE, self.materials.textures()),
                        Binding::sampler_table(program::SHADE_SAMPLER, self.materials.samplers()),
                    ],
                );
                self.depth_tables = Some(tables.clone());
                tables
            }
        };
        let shadow_tables = match &self.shadow_tables {
            Some(tables) => tables.clone(),
            None => {
                let tables = self.shadow.bind_group(
                    1,
                    &[
                        Binding::texture_table(program::SHADE_TEXTURE, self.materials.textures()),
                        Binding::sampler_table(program::SHADE_SAMPLER, self.materials.samplers()),
                    ],
                );
                self.shadow_tables = Some(tables.clone());
                tables
            }
        };
        let tables = match &self.tables {
            Some(tables) => tables.clone(),
            None => {
                let tables = self.draw.bind_group(
                    1,
                    &[
                        Binding::texture_table(program::SHADE_TEXTURE, self.materials.textures()),
                        Binding::sampler_table(program::SHADE_SAMPLER, self.materials.samplers()),
                    ],
                );
                self.tables = Some(tables.clone());
                tables
            }
        };
        let transparent_tables = match &self.transparent_tables {
            Some(tables) => tables.clone(),
            None => {
                let tables = self.transparent_draw.bind_group(
                    1,
                    &[
                        Binding::texture_table(program::SHADE_TEXTURE, self.materials.textures()),
                        Binding::sampler_table(program::SHADE_SAMPLER, self.materials.samplers()),
                    ],
                );
                self.transparent_tables = Some(tables.clone());
                tables
            }
        };
        let mut depth_groups = Vec::with_capacity(depth_batches as usize);
        let mut main_groups = Vec::with_capacity(batches.len());
        let mut cast_groups = Vec::with_capacity(CASCADE_COUNT as usize * batches.len());
        for (index, batch) in batches.iter().enumerate() {
            let geometry = self.meshes.geometry(batch.mesh);
            let vertices = self.meshes.buffer().binding(
                u64::from(geometry.first) * VERTEX_BYTES,
                u64::from(geometry.count) * VERTEX_BYTES,
            );
            let depth_drawn = self.depth_drawn.buffer().binding(
                u64::from(bases[index]) * DRAW_BYTES,
                u64::from(batch.count) * DRAW_BYTES,
            );
            let skins = self.meshes.skin_binding(geometry);
            if index < depth_batches as usize {
                depth_groups.push(
                    self.depth.bind_group(
                        0,
                        &[
                            Binding::buffer(program::DEPTH_VIEW, self.views.binding(0, VIEW_BYTES)),
                            Binding::buffer(
                                program::DEPTH_PALETTE,
                                self.joints.buffer().binding(0, joint_bytes),
                            ),
                            Binding::buffer(program::DEPTH_SKINS, skins),
                            Binding::buffer(program::DEPTH_DRAWN, depth_drawn),
                            Binding::buffer(program::DEPTH_VERTICES, vertices),
                            Binding::buffer(
                                program::DEPTH_MATERIALS,
                                self.material_records.buffer().binding(
                                    0,
                                    self.materials.records().len() as u64 * MATERIAL_BYTES,
                                ),
                            ),
                        ],
                    ),
                );
            }
            let drawn = self.main_drawn.buffer().binding(
                u64::from(bases[index]) * DRAW_BYTES,
                u64::from(batch.count) * DRAW_BYTES,
            );
            let pipeline = if index < depth_batches as usize {
                &self.draw
            } else {
                &self.transparent_draw
            };
            main_groups.push(
                pipeline.bind_group(
                    0,
                    &[
                        Binding::buffer(program::DRAW_LIGHT, self.light.binding(0, LIGHT_BYTES)),
                        Binding::buffer(
                            program::DRAW_CASCADES,
                            self.cascades
                                .binding(0, u64::from(CASCADE_COUNT) * CASCADE_BYTES),
                        ),
                        Binding::buffer(program::DRAW_DRAWN, drawn),
                        Binding::buffer(
                            program::DRAW_MATERIALS,
                            self.material_records
                                .buffer()
                                .binding(0, self.materials.records().len() as u64 * MATERIAL_BYTES),
                        ),
                        Binding::buffer(program::DRAW_VIEW, self.views.binding(0, VIEW_BYTES)),
                        Binding::buffer(
                            program::DRAW_PALETTE,
                            self.joints.buffer().binding(0, joint_bytes),
                        ),
                        Binding::buffer(program::DRAW_SKINS, skins),
                        Binding::buffer(program::DRAW_FRAGMENTS, self.transparency.fragments()),
                        Binding::buffer(program::DRAW_HEADS, self.transparency.heads()),
                        Binding::buffer(program::DRAW_COUNTER, self.transparency.counter()),
                        Binding::buffer(program::DRAW_SETTINGS, self.transparency.settings()),
                        Binding::buffer(program::DRAW_PROBES, self.probes.record(probe_count)),
                        Binding::buffer(program::DRAW_VERTICES, vertices),
                        Binding::buffer(
                            program::DRAW_LIGHTS,
                            self.lighting.buffer().binding(0, lighting_bytes),
                        ),
                    ],
                ),
            );
        }
        for cascade in 0..CASCADE_COUNT {
            for (index, batch) in batches.iter().enumerate().take(depth_batches as usize) {
                let geometry = self.meshes.geometry(batch.mesh);
                let vertices = self.meshes.buffer().binding(
                    u64::from(geometry.first) * VERTEX_BYTES,
                    u64::from(geometry.count) * VERTEX_BYTES,
                );
                let skins = self.meshes.skin_binding(geometry);
                let drawn = self.shadow_drawn.buffer().binding(
                    u64::from(cascade) * u64::from(slots) * DRAW_BYTES
                        + u64::from(bases[index]) * DRAW_BYTES,
                    u64::from(batch.count) * DRAW_BYTES,
                );
                cast_groups.push([
                    self.shadow.bind_group(
                        0,
                        &[
                            Binding::buffer(
                                program::SHADOW_PALETTE,
                                self.joints.buffer().binding(0, joint_bytes),
                            ),
                            Binding::buffer(program::SHADOW_SKINS, skins),
                            Binding::buffer(program::SHADOW_DRAWN, drawn),
                            Binding::buffer(program::SHADOW_VERTICES, vertices),
                            Binding::buffer(
                                program::SHADOW_MATERIALS,
                                self.material_records.buffer().binding(
                                    0,
                                    self.materials.records().len() as u64 * MATERIAL_BYTES,
                                ),
                            ),
                        ],
                    ),
                    shadow_tables.clone(),
                ]);
            }
        }
        let depth_groups = depth_groups
            .iter()
            .map(|group| [group.clone(), depth_tables.clone()])
            .collect::<Vec<_>>();
        let depth = depth_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                Draw::indirect(
                    &self.depth,
                    groups,
                    self.depth_arguments
                        .buffer()
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        submission.render(hzb.depth(), [1.0, 0.0, 0.0, 1.0], &depth);
        hzb.build(&mut submission, &self.hzb_base, &self.hzb_reduce);
        submission.dispatch(&self.cull, &main_cull, [total.div_ceil(WORKGROUP), 1, 1]);
        let shaded = main_groups
            .into_iter()
            .enumerate()
            .map(|(index, frame)| {
                if index < depth_batches as usize {
                    [frame, tables.clone(), self.shade.clone()]
                } else {
                    [
                        frame,
                        transparent_tables.clone(),
                        self.transparent_shade.clone(),
                    ]
                }
            })
            .collect::<Vec<_>>();
        let mut main = shaded
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                let pipeline = if index < depth_batches as usize {
                    &self.draw
                } else {
                    &self.transparent_draw
                };
                Draw::indirect(
                    pipeline,
                    groups,
                    self.main_arguments
                        .buffer()
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        let sky_groups = [self.sky_view.clone(), self.sky_shade.clone()];
        if sky.is_some() {
            main.insert(0, Draw::new(&self.sky, &sky_groups, SKY_VERTICES));
        }
        if !line_groups.is_empty() {
            main.push(Draw::new(&self.line_pipeline, &line_groups, line_vertices));
        }
        let cast = cast_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                let cascade = index / depth_batches as usize;
                let batch = index % depth_batches as usize;
                let column = cascade as u32 % SHADOW_ATLAS_COLUMNS;
                let row = cascade as u32 / SHADOW_ATLAS_COLUMNS;
                Draw::indirect(
                    &self.shadow,
                    groups,
                    self.shadow_arguments.buffer().binding(
                        u64::from(cascade as u32) * self.shadow_span
                            + batch as u64 * ARGUMENT_BYTES,
                        ARGUMENT_BYTES,
                    ),
                )
                .viewport(Viewport::new(
                    column as f32 * SHADOW_SIDE as f32,
                    row as f32 * SHADOW_SIDE as f32,
                    SHADOW_SIDE as f32,
                    SHADOW_SIDE as f32,
                ))
            })
            .collect::<Vec<_>>();
        submission.render(&self.shadow_map, [1.0, 0.0, 0.0, 1.0], &cast);
        self.transparency.clear(&mut submission);
        submission.render(self.post.hdr(), [clear[0], clear[1], clear[2], 0.0], &main);
        self.transparency
            .resolve(&mut submission, self.post.hdr(), self.post.lit());
        self.post.resolve(&mut submission);
        self.post.tonemap(&mut submission, &texture);
        if let Some(frame) = frame {
            submission.present(frame);
        }
        submission.submit(&self.queue);
    }

    pub fn pick(&mut self, point: Vec2) -> Option<Entity> {
        let frame = self.pick_frame.as_ref()?;
        let clip_space = self.device.clip_space();
        let resources = PickResources {
            drawn: self.main_drawn.buffer(),
            arguments: self.main_arguments.buffer(),
            meshes: &self.meshes,
            palette: self.joints.buffer(),
        };
        self.picker.pick(
            &self.device,
            &self.queue,
            clip_space,
            frame,
            &resources,
            point,
        )
    }

    pub fn visible_instances(&self) -> u32 {
        let batches = self.main_batches;
        if batches == 0 {
            return 0;
        }
        let bytes = u64::from(batches) * ARGUMENT_BYTES;
        let readback = GpuBuffer::new(
            &self.device,
            "belfry visibility readback",
            bytes,
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        );
        let mut submission = Submission::new(&self.device, "belfry visibility");
        submission.copy(self.main_arguments.buffer(), 0, &readback, 0, bytes);
        let index = submission.submit(&self.queue);
        let bytes = readback.read(&self.queue, index, bytes);
        bytes
            .as_chunks::<ARGUMENT_USIZE>()
            .0
            .iter()
            .map(|held| u32::from_ne_bytes(held[4..8].try_into().unwrap()))
            .sum()
    }

    fn ensure_hzb(&mut self, width: u32, height: u32, vertical: f32) {
        if self
            .hzb
            .as_ref()
            .is_some_and(|hzb| hzb.holds(width, height))
        {
            return;
        }
        self.hzb = Some(Hzb::new(
            &self.device,
            &self.queue,
            &self.hzb_base,
            &self.hzb_reduce,
            &self.hzb_sampler,
            width,
            height,
            vertical,
        ));
    }

    fn upload(&mut self, records: &[InstanceRecord], arguments: &[u32], slots: u32) {
        let instances = records.len() as u64 * INSTANCE_BYTES;
        self.instances.fit(&self.device, instances);
        self.instances
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(records));
        self.depth_drawn
            .fit(&self.device, u64::from(slots) * DRAW_BYTES);
        self.depth_arguments
            .fit(&self.device, arguments.len() as u64 * WORD_BYTES);
        self.depth_arguments
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(arguments));
        self.main_drawn
            .fit(&self.device, u64::from(slots) * DRAW_BYTES);
        self.main_arguments
            .fit(&self.device, arguments.len() as u64 * WORD_BYTES);
        self.main_arguments
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(arguments));
        self.shadow_drawn.fit(
            &self.device,
            u64::from(CASCADE_COUNT) * u64::from(slots) * DRAW_BYTES,
        );
        let argument_bytes = arguments.len() as u64 * WORD_BYTES;
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let span = argument_bytes.next_multiple_of(alignment);
        self.shadow_span = span;
        self.shadow_arguments
            .fit(&self.device, u64::from(CASCADE_COUNT) * span);
        let mut shadow = vec![0u8; (u64::from(CASCADE_COUNT) * span) as usize];
        let source = bytemuck::cast_slice(arguments);
        for cascade in 0..CASCADE_COUNT {
            let start = (u64::from(cascade) * span) as usize;
            shadow[start..start + source.len()].copy_from_slice(source);
        }
        self.shadow_arguments.buffer().write(&self.queue, &shadow);
    }
}

fn joint_world(world: &World, entity: Entity) -> Mat4 {
    world
        .get::<Transform>(entity)
        .unwrap_or_else(|| {
            panic!(
                "a skin joint names entity {}, which holds no world transform",
                entity.index(),
            )
        })
        .matrix()
}

fn joint_record(matrix: &Mat4) -> JointRecord {
    JointRecord {
        col0: vector(matrix.col(0)),
        col1: vector(matrix.col(1)),
        col2: vector(matrix.col(2)),
        col3: vector(matrix.col(3)),
    }
}

fn static_bounds(geometry: &crate::mesh::Geometry, transform: Transform) -> (Vec3, f32) {
    let reach = transform
        .scale
        .abs()
        .x
        .max(transform.scale.abs().y)
        .max(transform.scale.abs().z);
    (
        transform.transform_point(geometry.center),
        geometry.radius * reach,
    )
}

fn skinned_bounds(
    geometry: &crate::mesh::Geometry,
    transform: Transform,
    palette: &[Mat4],
) -> (Vec3, f32) {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    let mut spheres = Vec::with_capacity(palette.len());
    for matrix in palette {
        let center = matrix.transform_point(geometry.center);
        let radius = geometry.radius * matrix_reach(*matrix);
        low = low.min(center - Vec3::splat(radius));
        high = high.max(center + Vec3::splat(radius));
        spheres.push((center, radius));
    }
    let center = (low + high) * 0.5;
    let radius = spheres
        .iter()
        .map(|(held, reach)| held.distance(center) + reach)
        .fold(0.0, f32::max);
    let reach = transform
        .scale
        .abs()
        .x
        .max(transform.scale.abs().y)
        .max(transform.scale.abs().z);
    (transform.transform_point(center), radius * reach)
}

fn matrix_reach(matrix: Mat4) -> f32 {
    let matrix = matrix.to_mat3();
    (matrix.col(0).length_squared()
        + matrix.col(1).length_squared()
        + matrix.col(2).length_squared())
    .sqrt()
}

const _: () = assert!(
    JOINT_BYTES == JOINT.size as u64,
    "the joint record keeps the layout its shader reads",
);
const _: () = assert!(
    LIGHT_BYTES == size_of::<LightRecord>() as u64,
    "the light record keeps the layout its shader reads",
);
const _: () = assert!(
    LIGHT.size as u64 == LIGHT_BYTES,
    "the light layout spans its record",
);
const _: () = assert!(
    CASCADE_BYTES == size_of::<CascadeRecord>() as u64,
    "the cascade record keeps the layout its shader reads",
);
const _: () = assert!(
    belfry_render_abi::CASCADE.size as u64 == CASCADE_BYTES,
    "the cascade layout spans its record",
);
const _: () = assert!(
    MATERIAL_BYTES == size_of::<MaterialRecord>() as u64,
    "the material record keeps the layout its shader reads",
);
const _: () = assert!(
    MATERIAL.size as u64 == MATERIAL_BYTES,
    "the material layout spans its record",
);
const _: () = assert!(
    POINT_LIGHT_BYTES == size_of::<PointLightRecord>() as u64,
    "the point light record keeps the layout its shader reads",
);
const _: () = assert!(
    POINT_LIGHT.size as u64 == POINT_LIGHT_BYTES,
    "the point light layout spans its record",
);
const _: () = assert!(
    POINT_LIGHT_BYTES == 8 * WORD_BYTES,
    "a point light record spans the eight words its shader indexes",
);
const _: () = assert!(
    CLUSTER_BYTES == (LIGHT_CAPACITY + 1) as u64 * WORD_BYTES,
    "a light cluster keeps its count and every light beside it",
);
const _: () = assert!(
    VIEW_BYTES == 256,
    "a frame keeps its main and culling views on a storage binding boundary",
);
