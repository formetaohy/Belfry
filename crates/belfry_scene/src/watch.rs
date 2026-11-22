use crate::scene::Scene;
use belfry_gltf::{Model, ModelSources};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub scene: bool,
    pub model: bool,
    pub asset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Scene,
    Model,
    Asset,
}

struct WatchedModel {
    stamp: Option<Stamp>,
    sources: ModelSources,
}

pub struct Watch {
    path: PathBuf,
    files: Vec<(PathBuf, Option<Stamp>, Kind)>,
    models: HashMap<PathBuf, WatchedModel>,
    pending: Option<Change>,
}

impl Watch {
    pub fn new(scene: &Path, root: &Scene) -> Self {
        let mut watch = Self {
            path: scene.to_path_buf(),
            files: Vec::new(),
            models: HashMap::new(),
            pending: None,
        };
        watch.renew(root);
        watch
    }

    pub fn renew(&mut self, root: &Scene) {
        let mut desired = BTreeMap::new();
        desired.insert(self.path.clone(), Kind::Scene);
        for owners in root.instances.values() {
            for prefab in owners {
                desired.insert(root.root().join(prefab), Kind::Scene);
            }
        }
        for image in root.images() {
            if let Some(path) = image.source.path() {
                desired.insert(root.root().join(path), Kind::Asset);
            }
        }
        for sound in root.sounds() {
            desired.insert(root.root().join(&sound.path), Kind::Asset);
        }
        for environment in root.environments() {
            if let Some(path) = environment.panorama() {
                desired.insert(root.root().join(path), Kind::Asset);
            }
        }
        let mut models = HashMap::new();
        for model in root.models() {
            let path = root.root().join(&model.path);
            let stamp = stamp_of(&path);
            let sources = match self.models.get(&path) {
                Some(held) if held.stamp == stamp => held.sources.clone(),
                _ => Model::sources(&path),
            };
            models.insert(
                path.clone(),
                WatchedModel {
                    stamp,
                    sources: sources.clone(),
                },
            );
            desired.insert(path, Kind::Model);
            for source in sources.buffers.into_iter().chain(sources.images) {
                desired.insert(source, Kind::Asset);
            }
        }
        self.models = models;

        let mut previous = self
            .files
            .drain(..)
            .map(|(path, stamp, kind)| (path, (stamp, kind)))
            .collect::<BTreeMap<_, _>>();
        self.files = desired
            .into_iter()
            .map(|(path, kind)| {
                let stamp = previous
                    .remove(&path)
                    .and_then(|(stamp, held)| (held == kind).then_some(stamp))
                    .unwrap_or_else(|| stamp_of(&path));
                (path, stamp, kind)
            })
            .collect();
    }

    pub fn changed(&mut self) -> Option<Change> {
        let mut changed = Change::default();
        for (path, stamp, kind) in &mut self.files {
            let current = stamp_of(path);
            if *stamp != current {
                *stamp = current;
                match kind {
                    Kind::Scene => changed.scene = true,
                    Kind::Model => changed.model = true,
                    Kind::Asset => changed.asset = true,
                }
            }
        }
        if changed.scene || changed.model || changed.asset {
            let pending = self.pending.get_or_insert(Change::default());
            pending.scene |= changed.scene;
            pending.model |= changed.model;
            pending.asset |= changed.asset;
            return None;
        }
        self.pending.take()
    }
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(Stamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}
