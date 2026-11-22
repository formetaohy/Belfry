use super::prepare::panic_message;
use super::{AssetError, TextureKey};
use crate::digest::Digest;
use crate::environment::EnvironmentEntry;
use crate::image::ImageEntry;
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::scene::Scene;
use crate::skin::{SkinData, SkinEntry};
use crate::sound::SoundEntry;
use crate::source::Sources;
use belfry_animation::Clip;
use belfry_audio::Source;
use belfry_image::ColorSpace;
use belfry_render::TextureFormat;
use belfry_render::environment::Panorama;
use std::any::Any;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

pub(super) struct Job {
    pub(super) scene: Scene,
    pub(super) fingerprints: Fingerprints,
    pub(super) textures: HashSet<TextureKey>,
    pub(super) sources: Sources,
}

#[derive(Clone, Default)]
pub(super) struct Fingerprints {
    pub(super) images: HashMap<String, Digest>,
    pub(super) materials: HashMap<String, Digest>,
    pub(super) meshes: HashMap<String, Digest>,
    pub(super) skins: HashMap<String, Digest>,
    pub(super) animations: HashMap<String, Digest>,
    pub(super) sounds: HashMap<String, Digest>,
    pub(super) environments: HashMap<String, Digest>,
}

pub(super) enum Outcome {
    Prepared(Box<Prepared>),
    Failed(String),
}

pub(super) struct Prepared {
    pub(super) applied: bool,
    pub(super) changed: bool,
    pub(super) errors: Vec<AssetError>,
    pub(super) images: Vec<PreparedImage>,
    pub(super) materials: Vec<PreparedMaterial>,
    pub(super) meshes: Vec<PreparedMesh>,
    pub(super) skins: Vec<PreparedSkin>,
    pub(super) animations: Vec<PreparedAnimation>,
    pub(super) sounds: Vec<PreparedSound>,
    pub(super) environments: Vec<PreparedEnvironment>,
    pub(super) textures: HashMap<String, Arc<ImageMips>>,
    pub(super) changed_images: HashSet<String>,
    pub(super) removed: Removed,
    pub(super) sources: Sources,
    pub(super) files: BTreeSet<PathBuf>,
}

pub(super) struct PreparedImage {
    pub(super) entry: ImageEntry,
    pub(super) fingerprint: Digest,
    pub(super) mips: Arc<ImageMips>,
}

pub(super) struct PreparedMaterial {
    pub(super) entry: MaterialEntry,
    pub(super) fingerprint: Digest,
}

pub(super) struct PreparedMesh {
    pub(super) entry: MeshEntry,
    pub(super) fingerprint: Digest,
    pub(super) data: super::prepare::MeshData,
}

pub(super) struct PreparedSkin {
    pub(super) entry: SkinEntry,
    pub(super) fingerprint: Digest,
    pub(super) data: SkinData,
}

pub(super) struct PreparedAnimation {
    pub(super) entry: crate::animation::AnimationEntry,
    pub(super) fingerprint: Digest,
    pub(super) clip: Arc<Clip>,
}

pub(super) struct PreparedSound {
    pub(super) entry: SoundEntry,
    pub(super) fingerprint: Digest,
    pub(super) source: Source,
}

pub(super) struct PreparedEnvironment {
    pub(super) entry: EnvironmentEntry,
    pub(super) fingerprint: Digest,
    pub(super) panorama: Panorama,
    pub(super) sky: Option<Panorama>,
}

#[derive(Default)]
pub(super) struct Removed {
    pub(super) images: Vec<String>,
    pub(super) materials: Vec<String>,
    pub(super) meshes: Vec<String>,
    pub(super) skins: Vec<String>,
    pub(super) animations: Vec<String>,
    pub(super) sounds: Vec<String>,
    pub(super) environments: Vec<String>,
}

impl Removed {
    pub(super) fn any(&self) -> bool {
        !self.images.is_empty()
            || !self.materials.is_empty()
            || !self.meshes.is_empty()
            || !self.skins.is_empty()
            || !self.animations.is_empty()
            || !self.sounds.is_empty()
            || !self.environments.is_empty()
    }

    pub(super) fn gpu(&self) -> bool {
        !self.materials.is_empty() || !self.meshes.is_empty() || !self.environments.is_empty()
    }
}

pub(super) struct ImageMips {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) srgb: Vec<Vec<u8>>,
    pub(super) linear: Option<Vec<Vec<u8>>>,
}

impl ImageMips {
    pub(super) fn levels(&self, color: ColorSpace) -> Option<&[Vec<u8>]> {
        match color {
            ColorSpace::Srgb => Some(&self.srgb),
            ColorSpace::Linear => self.linear.as_deref(),
        }
    }

    pub(super) fn bytes(&self, color: ColorSpace) -> u64 {
        self.levels(color).map_or(0, |levels| {
            levels.iter().map(|level| level.len() as u64).sum()
        })
    }
}

impl Prepared {
    pub(super) fn gpu(&self) -> bool {
        !self.images.is_empty()
            || !self.materials.is_empty()
            || !self.meshes.is_empty()
            || !self.environments.is_empty()
    }
}

pub(super) fn format_of(color: ColorSpace) -> TextureFormat {
    match color {
        ColorSpace::Srgb => TextureFormat::Rgba8UnormSrgb,
        ColorSpace::Linear => TextureFormat::Rgba8Unorm,
    }
}

pub(super) fn image_modes(entry: &MaterialEntry) -> [(Option<&str>, ColorSpace); 5] {
    let asset = &entry.asset;
    [
        (
            asset.base_color_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Srgb,
        ),
        (
            asset
                .metallic_roughness_image
                .as_ref()
                .map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.normal_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.occlusion_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Linear,
        ),
        (
            asset.emissive_image.as_ref().map(|id| id.as_str()),
            ColorSpace::Srgb,
        ),
    ]
}

pub(super) struct Worker {
    jobs: Sender<Job>,
    results: Receiver<Outcome>,
}

impl Worker {
    pub(super) fn new() -> Self {
        let (jobs, held) = mpsc::channel::<Job>();
        let (outcome, results) = mpsc::channel::<Outcome>();
        thread::Builder::new()
            .name(String::from("belfry assets"))
            .spawn(move || {
                while let Ok(job) = held.recv() {
                    let result = catch_unwind(AssertUnwindSafe(|| super::plan::plan(job)))
                        .map(|prepared| Outcome::Prepared(Box::new(prepared)))
                        .unwrap_or_else(|payload| Outcome::Failed(describe(payload.as_ref())));
                    if outcome.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("the asset worker starts");
        Self { jobs, results }
    }

    pub(super) fn start(&self, job: Job) {
        self.jobs
            .send(job)
            .expect("the asset worker accepts the scene it loads");
    }

    pub(super) fn outcome(&self) -> Option<Outcome> {
        match self.results.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Outcome::Failed(String::from(
                "the asset worker stops before it loads the scene",
            ))),
        }
    }
}

fn describe(payload: &(dyn Any + Send)) -> String {
    panic_message(payload)
}
