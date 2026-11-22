use belfry_gpu::{SAMPLER_TABLE_CAPACITY, TEXTURE_TABLE_CAPACITY};
use belfry_gpu_device::{
    AddressMode, Device, Filter, Queue, Sampler, SamplerDesc, SamplerTable, Texture, TextureFormat,
    TextureTable, TextureUsages,
};
use belfry_text::Text;
use std::collections::HashMap;

const WHITE: [u8; 4] = [255, 255, 255, 255];

pub(crate) const LINEAR_SAMPLER: u32 = 0;

pub(crate) const LINEAR: SamplerDesc = SamplerDesc {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mip_filter: Filter::Linear,
    address_u: AddressMode::ClampToEdge,
    address_v: AddressMode::ClampToEdge,
};

pub(crate) const NEAREST: SamplerDesc = SamplerDesc {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mip_filter: Filter::Nearest,
    address_u: AddressMode::ClampToEdge,
    address_v: AddressMode::ClampToEdge,
};

struct Named {
    index: u32,
    sampler: u32,
    identity: usize,
}

pub(crate) struct Textures {
    device: Device,
    queue: Queue,
    table: TextureTable,
    samplers: SamplerTable,
    atlas: Option<Texture>,
    atlas_revision: Option<u64>,
    atlas_index: u32,
    scene_index: u32,
    scene_identity: usize,
    named: HashMap<String, Named>,
    generation: u64,
}

impl Textures {
    pub(crate) fn new(device: &Device, queue: &Queue) -> Self {
        let white_texture = Texture::new(
            device,
            "belfry ui white",
            1,
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        white_texture.write(queue, &WHITE);
        let mut table = TextureTable::new(device, TEXTURE_TABLE_CAPACITY, &white_texture);
        let scene_index = table.add(&white_texture);
        let atlas_index = table.add(&white_texture);
        let mut samplers = SamplerTable::new(
            device,
            SAMPLER_TABLE_CAPACITY,
            &Sampler::new(device, LINEAR),
        );
        samplers.add(&Sampler::new(device, LINEAR));
        samplers.add(&Sampler::new(device, NEAREST));
        Self {
            device: device.clone(),
            queue: queue.clone(),
            table,
            samplers,
            atlas: None,
            atlas_revision: None,
            atlas_index,
            scene_index,
            scene_identity: white_texture.identity(),
            named: HashMap::new(),
            generation: 0,
        }
    }

    pub(crate) fn refresh(&mut self, text: &Text) {
        let revision = text.atlas().revision();
        if self.atlas_revision == Some(revision) {
            return;
        }
        let size = text.atlas().size();
        let covered = atlas_pixels(text);
        if self.atlas.as_ref().is_some_and(|held| held.width() == size) {
            self.atlas
                .as_ref()
                .expect("a held atlas names its texture")
                .write(&self.queue, &covered);
        } else {
            let texture = Texture::new(
                &self.device,
                "belfry ui atlas",
                size,
                size,
                1,
                TextureFormat::Rgba8Unorm,
                TextureUsages::SAMPLED | TextureUsages::COPY_DST,
            );
            texture.write(&self.queue, &covered);
            self.table.set(self.atlas_index, &texture);
            self.atlas = Some(texture);
        }
        self.atlas_revision = Some(revision);
    }

    pub(crate) fn register(&mut self, name: &str, texture: &Texture, sampler: SamplerDesc) {
        let sampler = self
            .samplers
            .add_shared(&Sampler::new(&self.device, sampler));
        let identity = texture.identity();
        if let Some(named) = self.named.get_mut(name) {
            if named.identity != identity {
                self.table.set(named.index, texture);
                named.identity = identity;
            }
            named.sampler = sampler;
        } else {
            self.generation = self.generation.wrapping_add(1);
            let index = self.table.add_shared(texture);
            self.named.insert(
                String::from(name),
                Named {
                    index,
                    sampler,
                    identity,
                },
            );
        }
    }

    pub(crate) fn index(&self, name: &str) -> u32 {
        self.named
            .get(name)
            .unwrap_or_else(|| panic!("a user interface paints the unregistered texture {name}"))
            .index
    }

    pub(crate) fn sampler(&self, name: &str) -> u32 {
        self.named
            .get(name)
            .unwrap_or_else(|| panic!("a user interface paints the unregistered texture {name}"))
            .sampler
    }

    pub(crate) fn set_scene(&mut self, texture: &Texture) {
        if self.scene_identity == texture.identity() {
            return;
        }
        self.table.set(self.scene_index, texture);
        self.scene_identity = texture.identity();
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn scene(&self) -> (u32, u32) {
        (self.scene_index, LINEAR_SAMPLER)
    }

    pub(crate) fn atlas(&self) -> (u32, u32) {
        (self.atlas_index, LINEAR_SAMPLER)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn table(&self) -> &TextureTable {
        &self.table
    }

    pub(crate) fn samplers(&self) -> &SamplerTable {
        &self.samplers
    }
}

fn atlas_pixels(text: &Text) -> Vec<u8> {
    let atlas = text.atlas();
    let coverage = atlas.pixels();
    let mut pixels = vec![255u8; (atlas.size() * atlas.size() * 4) as usize];
    for (target, alpha) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(coverage.iter())
    {
        target[3] = *alpha;
    }
    pixels
}
