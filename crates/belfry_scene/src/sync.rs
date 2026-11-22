use crate::animation_library::AnimationLibrary;
use crate::graph_library::GraphLibrary;
use crate::install::NodeMap;
use crate::node::SceneNode;
use crate::registry::Assets;
use crate::scene::Scene;
use belfry_ecs::{Entity, Parent, World, propagate};
use std::collections::{HashMap, HashSet};

pub struct SceneSync {
    applied: Option<Scene>,
    entities: Vec<Entity>,
    ids: HashMap<String, Entity>,
}

impl SceneSync {
    pub fn new() -> Self {
        Self {
            applied: None,
            entities: Vec::new(),
            ids: HashMap::new(),
        }
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn applied(&self) -> Option<&Scene> {
        self.applied.as_ref()
    }

    pub fn entity(&self, index: usize) -> Option<Entity> {
        self.entities.get(index).copied()
    }

    pub fn id(&self, id: &str) -> Option<Entity> {
        self.ids.get(id).copied()
    }

    pub fn sync(
        &mut self,
        scene: &Scene,
        world: &mut World,
        assets: &dyn Assets,
        replace_assets: bool,
    ) {
        if !replace_assets && self.applied.as_ref() == Some(scene) {
            return;
        }
        let previous = self.applied.as_ref();
        let mut ids = HashMap::with_capacity(scene.nodes().len());
        let mut entities = Vec::with_capacity(scene.nodes().len());
        for node in scene.nodes() {
            let held = self.ids.get(&node.id).copied();
            let reused = held.is_some_and(|entity| world.is_alive(entity));
            let entity = match held {
                Some(entity) if reused => entity,
                _ => world.spawn(),
            };
            assert!(
                ids.insert(node.id.clone(), entity).is_none(),
                "the scene holds the node {} twice",
                node.id,
            );
            entities.push((entity, reused));
        }
        let retained = ids.values().copied().collect::<HashSet<_>>();
        for entity in self.ids.values() {
            if !retained.contains(entity) {
                world.despawn(*entity);
            }
        }
        let nodes = NodeMap::new(ids.clone());
        let animations = AnimationLibrary::build(scene, assets, &nodes);
        let graphs = GraphLibrary::build(scene, &animations);
        world.insert_resource(nodes);
        world.insert_resource(animations);
        world.insert_resource(graphs);
        for (index, node) in scene.nodes().iter().enumerate() {
            let (entity, reused) = entities[index];
            let previous = previous
                .and_then(|applied| applied.node_index(&node.id).map(|at| &applied.nodes()[at]));
            world.insert(
                entity,
                SceneNode {
                    id: node.id.clone(),
                },
            );
            match node.parent.as_deref() {
                Some(parent) => {
                    let parent = *ids.get(parent).unwrap_or_else(|| {
                        panic!(
                            "the scene parents {} to {parent}, which it does not hold",
                            node.id,
                        )
                    });
                    world.insert(entity, Parent(parent));
                }
                None => {
                    world.remove::<Parent>(entity);
                }
            }
            for registration in scene.registry().registrations() {
                let replaced = !reused
                    || (replace_assets && registration.tied_to_assets)
                    || previous.is_none_or(|previous| {
                        match (
                            previous.component_by_name(registration.spec.name),
                            node.component_by_name(registration.spec.name),
                        ) {
                            (None, None) => false,
                            (Some(authored), Some(next)) => !authored.equals(next),
                            _ => true,
                        }
                    });
                if !replaced {
                    continue;
                }
                (registration.remove)(world, entity);
                if let Some(component) = node.component_by_name(registration.spec.name) {
                    component.install(scene, &node.id, world, entity, assets);
                }
            }
        }
        self.applied = Some(scene.clone());
        self.entities = entities.into_iter().map(|(entity, _)| entity).collect();
        self.ids = ids;
        belfry_animation::settle(world);
        propagate(world);
    }

    pub fn apply(&mut self, scene: &Scene, world: &mut World, assets: &dyn Assets, index: usize) {
        let Some(node) = scene.nodes().get(index) else {
            return;
        };
        let Some(entity) = self.ids.get(&node.id).copied() else {
            return;
        };
        if !world.is_alive(entity) {
            return;
        }
        let previous = self
            .applied
            .as_ref()
            .and_then(|applied| applied.node_index(&node.id).map(|at| &applied.nodes()[at]));
        match node.parent.as_deref() {
            Some(parent) => {
                let Some(parent) = self.ids.get(parent).copied() else {
                    return;
                };
                world.insert(entity, Parent(parent));
            }
            None => {
                world.remove::<Parent>(entity);
            }
        }
        for registration in scene.registry().registrations() {
            let replaced = previous.is_none_or(|previous| {
                match (
                    previous.component_by_name(registration.spec.name),
                    node.component_by_name(registration.spec.name),
                ) {
                    (None, None) => false,
                    (Some(authored), Some(next)) => !authored.equals(next),
                    _ => true,
                }
            });
            if !replaced {
                continue;
            }
            (registration.remove)(world, entity);
            if let Some(component) = node.component_by_name(registration.spec.name) {
                component.install(scene, &node.id, world, entity, assets);
            }
        }
        if let Some(applied) = &mut self.applied
            && let Some(at) = applied.node_index(&node.id)
        {
            applied.nodes_mut()[at] = node.clone();
        }
        belfry_animation::settle(world);
        propagate(world);
    }
}

impl Default for SceneSync {
    fn default() -> Self {
        Self::new()
    }
}
