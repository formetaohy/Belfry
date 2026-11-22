use crate::install::NodeMap;
use crate::registry::Assets;
use crate::scene::Scene;
use belfry_animation::ClipSource;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct AnimationLibrary {
    sources: HashMap<String, Arc<ClipSource>>,
}

impl AnimationLibrary {
    pub(crate) fn build(scene: &Scene, assets: &dyn Assets, nodes: &NodeMap) -> Self {
        let sources = scene
            .animations()
            .iter()
            .map(|entry| {
                let clip = assets.animation(entry.id.as_str());
                let targets = entry
                    .asset
                    .targets()
                    .iter()
                    .map(|target| nodes.entity(target.as_str()))
                    .collect::<Vec<_>>();
                (
                    entry.id.clone(),
                    Arc::new(ClipSource::new(entry.id.as_str(), clip, Arc::from(targets))),
                )
            })
            .collect();
        Self { sources }
    }

    pub fn source(&self, id: &str) -> Arc<ClipSource> {
        Arc::clone(self.sources.get(id).unwrap_or_else(|| {
            panic!("the scene plays the animation {id}, which it does not hold")
        }))
    }

    pub fn get(&self, id: &str) -> Option<Arc<ClipSource>> {
        self.sources.get(id).cloned()
    }
}
