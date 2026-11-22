use crate::animation::AnimationEntry;
use crate::environment::EnvironmentEntry;
use crate::graph::GraphEntry;
use crate::image::ImageEntry;
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::model::ModelEntry;
use crate::node::Node;
use crate::scene::{Owned, Scene};
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delta {
    steps: Vec<Step>,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn apply(&self, scene: &mut Scene) {
        for step in &self.steps {
            step.apply(scene);
        }
    }

    pub fn revert(&self, scene: &mut Scene) {
        for step in self.steps.iter().rev() {
            step.revert(scene);
        }
    }

    pub(crate) fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    pub(crate) fn pop(&mut self) -> Option<Step> {
        self.steps.pop()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Nodes(Splice<Node>),
    Images(Splice<ImageEntry>),
    Sounds(Splice<SoundEntry>),
    Environments(Splice<EnvironmentEntry>),
    Meshes(Splice<MeshEntry>),
    Materials(Splice<MaterialEntry>),
    Models(Splice<ModelEntry>),
    Skins(Splice<SkinEntry>),
    Animations(Splice<AnimationEntry>),
    Graphs(Splice<GraphEntry>),
    Owners(Splice<Owned>),
    Instances {
        key: String,
        before: Option<BTreeSet<PathBuf>>,
        after: Option<BTreeSet<PathBuf>>,
    },
}

impl Step {
    fn apply(&self, scene: &mut Scene) {
        match self {
            Self::Nodes(splice) => {
                scene
                    .nodes
                    .splice(splice.at, splice.removed.len(), splice.inserted.clone());
            }
            Self::Images(splice) => splice.apply(&mut scene.images),
            Self::Sounds(splice) => splice.apply(&mut scene.sounds),
            Self::Environments(splice) => splice.apply(&mut scene.environments),
            Self::Meshes(splice) => splice.apply(&mut scene.meshes),
            Self::Materials(splice) => splice.apply(&mut scene.materials),
            Self::Models(splice) => splice.apply(&mut scene.models),
            Self::Skins(splice) => splice.apply(&mut scene.skins),
            Self::Animations(splice) => splice.apply(&mut scene.animations),
            Self::Graphs(splice) => splice.apply(&mut scene.graphs),
            Self::Owners(splice) => splice.apply(&mut scene.owners),
            Self::Instances { key, after, .. } => hold(scene, key, after),
        }
    }

    pub(crate) fn revert(&self, scene: &mut Scene) {
        match self {
            Self::Nodes(splice) => {
                scene
                    .nodes
                    .splice(splice.at, splice.inserted.len(), splice.removed.clone());
            }
            Self::Images(splice) => splice.revert(&mut scene.images),
            Self::Sounds(splice) => splice.revert(&mut scene.sounds),
            Self::Environments(splice) => splice.revert(&mut scene.environments),
            Self::Meshes(splice) => splice.revert(&mut scene.meshes),
            Self::Materials(splice) => splice.revert(&mut scene.materials),
            Self::Models(splice) => splice.revert(&mut scene.models),
            Self::Skins(splice) => splice.revert(&mut scene.skins),
            Self::Animations(splice) => splice.revert(&mut scene.animations),
            Self::Graphs(splice) => splice.revert(&mut scene.graphs),
            Self::Owners(splice) => splice.revert(&mut scene.owners),
            Self::Instances { key, before, .. } => hold(scene, key, before),
        }
    }
}

fn hold(scene: &mut Scene, key: &str, held: &Option<BTreeSet<PathBuf>>) {
    match held {
        Some(keys) => {
            scene.instances.insert(String::from(key), keys.clone());
        }
        None => {
            scene.instances.remove(key);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Splice<T> {
    pub(crate) at: usize,
    pub(crate) removed: Vec<T>,
    pub(crate) inserted: Vec<T>,
}

impl<T> Splice<T> {
    pub(crate) fn insert(at: usize, inserted: Vec<T>) -> Self {
        Self {
            at,
            removed: Vec::new(),
            inserted,
        }
    }

    pub(crate) fn remove(at: usize, removed: Vec<T>) -> Self {
        Self {
            at,
            removed,
            inserted: Vec::new(),
        }
    }
}

impl<T: Clone> Splice<T> {
    fn apply(&self, entries: &mut Vec<T>) {
        entries.splice(
            self.at..self.at + self.removed.len(),
            self.inserted.iter().cloned(),
        );
    }

    fn revert(&self, entries: &mut Vec<T>) {
        entries.splice(
            self.at..self.at + self.inserted.len(),
            self.removed.iter().cloned(),
        );
    }
}

pub(crate) trait Asset: Clone + PartialEq {
    fn id(&self) -> &str;

    fn entries(scene: &mut Scene) -> &mut Vec<Self>;
    fn held(scene: &Scene) -> &[Self];
    fn step(splice: Splice<Self>) -> Step;
}

impl Asset for ImageEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.images
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.images
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Images(splice)
    }
}

impl Asset for SoundEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.sounds
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.sounds
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Sounds(splice)
    }
}

impl Asset for EnvironmentEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.environments
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.environments
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Environments(splice)
    }
}

impl Asset for MeshEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.meshes
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.meshes
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Meshes(splice)
    }
}

impl Asset for MaterialEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.materials
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.materials
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Materials(splice)
    }
}

impl Asset for ModelEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.models
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.models
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Models(splice)
    }
}

impl Asset for SkinEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.skins
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.skins
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Skins(splice)
    }
}

impl Asset for AnimationEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.animations
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.animations
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Animations(splice)
    }
}

impl Asset for GraphEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn entries(scene: &mut Scene) -> &mut Vec<Self> {
        &mut scene.graphs
    }

    fn held(scene: &Scene) -> &[Self] {
        &scene.graphs
    }

    fn step(splice: Splice<Self>) -> Step {
        Step::Graphs(splice)
    }
}

pub(crate) fn owned(
    kind: crate::schema::ReferenceKind,
    id: impl Into<String>,
    prefab: &Path,
    scope: Option<&str>,
) -> Owned {
    Owned {
        kind,
        id: id.into(),
        prefab: prefab.to_path_buf(),
        scope: scope.map(String::from),
    }
}
