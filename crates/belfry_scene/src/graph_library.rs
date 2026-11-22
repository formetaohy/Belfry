use crate::animation_library::AnimationLibrary;
use crate::scene::Scene;
use belfry_animation::{AnimationGraph, ConditionDesc, ParameterDesc, StateDesc, TransitionDesc};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct GraphLibrary {
    graphs: HashMap<String, Arc<AnimationGraph>>,
}

impl GraphLibrary {
    pub(crate) fn build(scene: &Scene, animations: &AnimationLibrary) -> Self {
        let graphs = scene
            .graphs()
            .iter()
            .map(|entry| {
                let asset = &entry.asset;
                let parameters = asset
                    .parameters()
                    .iter()
                    .map(|held| ParameterDesc {
                        name: String::from(held.name()),
                        default: held.default(),
                    })
                    .collect();
                let states = asset
                    .states()
                    .iter()
                    .map(|held| StateDesc {
                        id: String::from(held.id()),
                        source: animations.source(held.clip().as_str()),
                        speed: held.speed(),
                        looped: held.looped(),
                    })
                    .collect();
                let transitions = asset
                    .transitions()
                    .iter()
                    .map(|held| TransitionDesc {
                        from: String::from(held.from()),
                        to: String::from(held.to()),
                        fade: held.fade(),
                        exit: held.exit(),
                        conditions: held
                            .conditions()
                            .iter()
                            .map(|held| ConditionDesc {
                                parameter: String::from(held.parameter()),
                                comparison: held.comparison(),
                                value: held.value(),
                            })
                            .collect(),
                    })
                    .collect();
                let position = asset
                    .states()
                    .iter()
                    .position(|held| held.id() == asset.entry())
                    .expect("a validated graph enters a state it holds");
                (
                    entry.id.clone(),
                    Arc::new(AnimationGraph::new(
                        entry.id.as_str(),
                        parameters,
                        states,
                        transitions,
                        position,
                    )),
                )
            })
            .collect();
        Self { graphs }
    }

    pub fn get(&self, id: &str) -> Option<Arc<AnimationGraph>> {
        self.graphs.get(id).cloned()
    }

    pub fn source(&self, id: &str) -> Arc<AnimationGraph> {
        self.get(id).unwrap_or_else(|| {
            panic!("the scene plays the animation graph {id}, which it does not hold")
        })
    }
}
