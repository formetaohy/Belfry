use super::work::{
    ImageMips, Prepared, PreparedAnimation, PreparedEnvironment, PreparedImage, PreparedMaterial,
    PreparedMesh, PreparedSkin, PreparedSound, format_of, image_modes,
};
use super::{AssetError, ImageRecord, ImageTexture, Library, MaterialRecord, MeshRecord, Refresh};
use belfry_image::ColorSpace;
use belfry_render::{
    Environment, Renderer, SamplerDesc, SkinVertexRecord, TextureFormat, TextureUsages, Vertex,
};
use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::sync::Arc;

const ITEM_LIMIT: usize = 64;
const EMPTY_TEXEL: &[u8] = &[0, 0, 0, 0, 0, 0, 0x00, 0x3C];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Remove,
    Images,
    Materials,
    Meshes,
    Skins,
    Animations,
    Sounds,
    Environments,
    Finish,
    Done,
}

pub(super) struct Commit {
    prepared: Prepared,
    refresh: Refresh,
    stage: Stage,
    index: usize,
    spent: u64,
    items: usize,
}

impl Commit {
    pub(super) fn new(mut prepared: Prepared) -> Self {
        let refresh = Refresh {
            applied: prepared.applied,
            changed: prepared.changed,
            errors: std::mem::take(&mut prepared.errors),
        };
        Self {
            prepared,
            refresh,
            stage: Stage::Remove,
            index: 0,
            spent: 0,
            items: 0,
        }
    }

    pub(super) fn run(
        &mut self,
        library: &mut Library,
        renderer: &mut Renderer,
        budget: u64,
    ) -> Option<Refresh> {
        self.spent = 0;
        self.items = 0;
        if !self.prepared.applied {
            library.adopt(&mut self.prepared);
            return Some(self.finish());
        }
        loop {
            if self.items >= ITEM_LIMIT {
                return None;
            }
            match self.stage {
                Stage::Remove => self.remove(library, renderer),
                Stage::Images => {
                    let Some(item) = self.prepared.images.get(self.index) else {
                        self.next(Stage::Materials);
                        continue;
                    };
                    let cost = item.mips.bytes(ColorSpace::Srgb);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_image(renderer, item);
                    self.charge(cost);
                }
                Stage::Materials => {
                    let Some(item) = self.prepared.materials.get(self.index) else {
                        self.next(Stage::Meshes);
                        continue;
                    };
                    let cost = library.material_cost(&item.entry, &self.prepared.textures);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    if let Err(message) =
                        library.commit_material(renderer, item, &self.prepared.textures)
                    {
                        self.refresh.errors.push(AssetError {
                            kind: "material",
                            id: item.entry.id.clone(),
                            message,
                            fatal: false,
                        });
                    }
                    self.charge(cost);
                }
                Stage::Meshes => {
                    let Some(item) = self.prepared.meshes.get(self.index) else {
                        self.next(Stage::Skins);
                        continue;
                    };
                    let cost = mesh_bytes(item);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_mesh(renderer, item);
                    self.charge(cost);
                }
                Stage::Skins => {
                    let Some(item) = self.prepared.skins.get(self.index) else {
                        self.next(Stage::Animations);
                        continue;
                    };
                    if !self.allows(0, budget) {
                        return None;
                    }
                    library.commit_skin(item);
                    self.charge(0);
                }
                Stage::Animations => {
                    let Some(item) = self.prepared.animations.get(self.index) else {
                        self.next(Stage::Sounds);
                        continue;
                    };
                    if !self.allows(0, budget) {
                        return None;
                    }
                    library.commit_animation(item);
                    self.charge(0);
                }
                Stage::Sounds => {
                    let Some(item) = self.prepared.sounds.get(self.index) else {
                        self.next(Stage::Environments);
                        continue;
                    };
                    let cost = if item.entry.stream {
                        0
                    } else {
                        item.source.frame_count().unwrap_or(0) * 4
                    };
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_sound(item);
                    self.charge(cost);
                }
                Stage::Environments => {
                    let Some(item) = self.prepared.environments.get(self.index) else {
                        self.next(Stage::Finish);
                        continue;
                    };
                    let cost = environment_bytes(item);
                    if !self.allows(cost, budget) {
                        return None;
                    }
                    library.commit_environment(renderer, item);
                    self.charge(cost);
                }
                Stage::Finish => {
                    library.adopt(&mut self.prepared);
                    self.stage = Stage::Done;
                }
                Stage::Done => return Some(self.finish()),
            }
        }
    }

    fn remove(&mut self, library: &mut Library, renderer: &mut Renderer) {
        if self.prepared.gpu() || (library.synchronous && self.prepared.removed.gpu()) {
            renderer.idle();
        }
        for id in &self.prepared.removed.materials {
            if let Some(record) = library.materials.remove(id) {
                if library.synchronous {
                    renderer.remove_material(record.material);
                } else {
                    library.retired_materials.push(record.material);
                }
            }
        }
        for id in &self.prepared.removed.meshes {
            if let Some(record) = library.meshes.remove(id) {
                if library.synchronous {
                    renderer.remove_mesh(record.mesh);
                } else {
                    library.retired_meshes.push(record.mesh);
                }
            }
        }
        for id in &self.prepared.removed.images {
            library.images.remove(id);
            library.textures.retain(|(held, ..), _| held != id);
        }
        for id in &self.prepared.changed_images {
            library.textures.retain(|(held, ..), _| held != id);
        }
        for id in &self.prepared.removed.skins {
            library.skins.remove(id);
        }
        for id in &self.prepared.removed.animations {
            library.animations.remove(id);
        }
        for id in &self.prepared.removed.sounds {
            library.sounds.remove(id);
        }
        for id in &self.prepared.removed.environments {
            library.environments.remove(id);
        }
        self.stage = Stage::Images;
        self.index = 0;
    }

    fn finish(&mut self) -> Refresh {
        Refresh {
            applied: self.refresh.applied,
            changed: self.refresh.changed,
            errors: std::mem::take(&mut self.refresh.errors),
        }
    }

    fn allows(&self, cost: u64, budget: u64) -> bool {
        if self.items >= ITEM_LIMIT {
            return false;
        }
        self.items == 0 || self.spent.saturating_add(cost) <= budget
    }

    fn charge(&mut self, cost: u64) {
        self.spent = self.spent.saturating_add(cost);
        self.items += 1;
        self.index += 1;
    }

    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.index = 0;
    }
}

impl Library {
    fn adopt(&mut self, prepared: &mut Prepared) {
        self.sources = std::mem::take(&mut prepared.sources);
        self.sources.retain(&prepared.files);
    }

    fn commit_image(&mut self, renderer: &Renderer, item: &PreparedImage) {
        let levels = item
            .mips
            .levels(ColorSpace::Srgb)
            .expect("a prepared image holds its colour levels")
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        let texture = renderer.texture(
            &item.entry.id,
            item.mips.width,
            item.mips.height,
            levels.len() as u32,
            TextureFormat::Rgba8UnormSrgb,
            TextureUsages::SAMPLED | TextureUsages::COPY_DST,
        );
        renderer.write_texture(&texture, &levels);
        self.textures.insert(
            (
                item.entry.id.clone(),
                TextureFormat::Rgba8UnormSrgb,
                ColorSpace::Srgb,
            ),
            texture.clone(),
        );
        self.images.insert(
            item.entry.id.clone(),
            ImageRecord {
                fingerprint: item.fingerprint,
                image: ImageTexture {
                    texture,
                    sampler: item.entry.sampler,
                },
            },
        );
    }

    fn material_cost(
        &self,
        entry: &crate::material::MaterialEntry,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> u64 {
        let mut seen = HashSet::new();
        let mut cost = 0;
        for (id, color) in image_modes(entry) {
            let Some(id) = id else {
                continue;
            };
            let key = (String::from(id), format_of(color), color);
            if self.textures.contains_key(&key) || !seen.insert(key) {
                continue;
            }
            if let Some(mips) = textures.get(id) {
                cost += mips.bytes(color);
            }
        }
        cost
    }

    fn commit_material(
        &mut self,
        renderer: &mut Renderer,
        item: &PreparedMaterial,
        textures: &HashMap<String, Arc<ImageMips>>,
    ) -> Result<(), String> {
        let desc = self.material_desc(renderer, &item.entry, textures)?;
        let material = match self.materials.get(&item.entry.id) {
            Some(previous) => {
                renderer.update_material(previous.material, desc);
                previous.material
            }
            None => renderer.material(desc),
        };
        self.materials.insert(
            item.entry.id.clone(),
            MaterialRecord {
                fingerprint: item.fingerprint,
                material,
            },
        );
        Ok(())
    }

    fn commit_mesh(&mut self, renderer: &mut Renderer, item: &PreparedMesh) {
        let previous = self.meshes.get(&item.entry.id).map(|held| held.mesh);
        let mesh = match (previous, item.data.skin.as_deref()) {
            (Some(mesh), Some(skin)) => {
                renderer.update_skinned_mesh(mesh, &item.data.vertices, skin);
                mesh
            }
            (Some(mesh), None) => {
                renderer.update_mesh(mesh, &item.data.vertices);
                mesh
            }
            (None, Some(skin)) => renderer.skinned_mesh(&item.data.vertices, skin),
            (None, None) => renderer.mesh(&item.data.vertices),
        };
        self.meshes.insert(
            item.entry.id.clone(),
            MeshRecord {
                fingerprint: item.fingerprint,
                mesh,
                shape: item.data.shape.clone(),
            },
        );
    }

    fn commit_skin(&mut self, item: &PreparedSkin) {
        self.skins.insert(
            item.entry.id.clone(),
            super::SkinRecord {
                fingerprint: item.fingerprint,
                data: item.data.clone(),
            },
        );
    }

    fn commit_animation(&mut self, item: &PreparedAnimation) {
        self.animations.insert(
            item.entry.id.clone(),
            super::AnimationRecord {
                fingerprint: item.fingerprint,
                clip: Arc::clone(&item.clip),
            },
        );
    }

    fn commit_sound(&mut self, item: &PreparedSound) {
        self.sounds.insert(
            item.entry.id.clone(),
            super::SoundRecord {
                fingerprint: item.fingerprint,
                source: item.source.clone(),
            },
        );
    }

    fn commit_environment(&mut self, renderer: &mut Renderer, item: &PreparedEnvironment) {
        let sampler = self.sampler(
            renderer,
            SamplerDesc {
                min_filter: belfry_render::Filter::Linear,
                mag_filter: belfry_render::Filter::Linear,
                mip_filter: belfry_render::Filter::Linear,
                address_u: belfry_render::AddressMode::Repeat,
                address_v: belfry_render::AddressMode::ClampToEdge,
            },
        );
        let environment = if item.entry.capture() {
            let radiance = renderer.texture(
                &item.entry.id,
                1,
                1,
                1,
                TextureFormat::Rgba16Float,
                TextureUsages::SAMPLED | TextureUsages::COPY_DST,
            );
            renderer.write_texture(&radiance, &[EMPTY_TEXEL]);
            let panorama = renderer.texture(
                &format!("{}.panorama", item.entry.id),
                1,
                1,
                1,
                TextureFormat::Rgba16Float,
                TextureUsages::SAMPLED | TextureUsages::COPY_DST,
            );
            renderer.write_texture(&panorama, &[EMPTY_TEXEL]);
            Environment::new(radiance, panorama, sampler, 1, [belfry_math::Vec3::ZERO; 9])
        } else {
            let panorama = item.panorama.texture(renderer, &item.entry.id);
            let background = match &item.sky {
                Some(sky) => sky.texture(renderer, &format!("{}.sky", item.entry.id)),
                None => panorama.clone(),
            };
            let baked = renderer.bake(&panorama, &sampler);
            let environment = Environment::new(
                baked.radiance,
                background,
                sampler,
                baked.levels,
                baked.irradiance,
            );
            match item.entry.atmosphere() {
                Some(atmosphere) => environment.lit(atmosphere.sun_intensity, true),
                None => environment,
            }
        };
        self.environments.insert(
            item.entry.id.clone(),
            super::EnvironmentRecord {
                fingerprint: item.fingerprint,
                capture: item.entry.capture(),
                environment,
            },
        );
    }
}

fn environment_bytes(item: &PreparedEnvironment) -> u64 {
    let panorama = u64::from(item.panorama.width()) * u64::from(item.panorama.height()) * 8;
    let sky = item.sky.as_ref().map_or(0, |sky| {
        u64::from(sky.width()) * u64::from(sky.height()) * 8
    });
    panorama + sky
}

fn mesh_bytes(item: &PreparedMesh) -> u64 {
    let vertices = item.data.vertices.len() as u64 * size_of::<Vertex>() as u64;
    let skin = item.data.skin.as_ref().map_or(0, |skin| {
        skin.len() as u64 * size_of::<SkinVertexRecord>() as u64
    });
    vertices + skin
}
