use crate::{Entity, LocalTransform, Parent, World};
use belfry_math::Transform;

pub fn world_transform(world: &World, entity: Entity) -> Transform {
    resolve(world, entity, 0)
}

fn resolve(world: &World, entity: Entity, depth: usize) -> Transform {
    assert!(
        depth <= world.len(),
        "the node of entity {} closes its parent chain into a cycle",
        entity.index(),
    );
    let local = world.get::<LocalTransform>(entity).unwrap_or_else(|| {
        panic!(
            "the node of entity {} holds no local transform",
            entity.index(),
        )
    });
    match world.get::<Parent>(entity) {
        Some(parent) => resolve(world, parent.0, depth + 1).compose(local.0),
        None => local.0,
    }
}
