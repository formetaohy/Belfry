use crate::node::Node;
use crate::registry::Assets;
use crate::scene::Scene;
use belfry_ecs::{Entity, World};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeMap(HashMap<String, Entity>);

impl NodeMap {
    pub(crate) fn new(nodes: HashMap<String, Entity>) -> Self {
        Self(nodes)
    }

    pub(crate) fn entity(&self, id: &str) -> Entity {
        *self
            .0
            .get(id)
            .unwrap_or_else(|| panic!("the scene holds no node {id}"))
    }
}

pub(crate) fn install_components(
    scene: &Scene,
    node: &Node,
    world: &mut World,
    entity: Entity,
    assets: &dyn Assets,
) {
    for component in scene.registry().components() {
        if let Some(held) = node.component_by_name(component.name) {
            held.install(scene, &node.id, world, entity, assets);
        }
    }
}
