use crate::{LocalTransform, Parent, World};
use belfry_math::Transform;

const VACANT: u32 = u32::MAX;

pub fn propagate(world: &mut World) {
    let locals = world
        .iter::<LocalTransform>()
        .map(|(entity, local)| {
            (
                entity,
                local.0,
                world.get::<Parent>(entity).map(|parent| parent.0),
            )
        })
        .collect::<Vec<_>>();
    let Some(top) = locals.iter().map(|(entity, ..)| entity.index()).max() else {
        return;
    };
    let mut slots = vec![VACANT; top as usize + 1];
    for (at, (entity, ..)) in locals.iter().enumerate() {
        slots[entity.index() as usize] = at as u32;
    }
    let mut children = vec![Vec::new(); locals.len()];
    let mut roots = Vec::new();
    for (at, (entity, .., parent)) in locals.iter().enumerate() {
        let Some(parent) = parent else {
            roots.push(at);
            continue;
        };
        let slot = *slots.get(parent.index() as usize).unwrap_or(&VACANT);
        assert!(
            slot != VACANT,
            "the hierarchy parents entity {} to entity {}, which holds no local transform",
            entity.index(),
            parent.index(),
        );
        assert!(
            slot as usize != at,
            "entity {} parents itself",
            entity.index(),
        );
        children[slot as usize].push(at);
    }
    let mut worlds = vec![Transform::IDENTITY; locals.len()];
    let mut visited = vec![false; locals.len()];
    let mut stack = roots
        .iter()
        .map(|at| (*at, Transform::IDENTITY))
        .collect::<Vec<_>>();
    while let Some((at, parent)) = stack.pop() {
        visited[at] = true;
        let held = parent.compose(locals[at].1);
        worlds[at] = held;
        for child in &children[at] {
            stack.push((*child, held));
        }
    }
    if let Some(at) = visited.iter().position(|held| !held) {
        panic!(
            "the hierarchy closes the parent chain of entity {} into a cycle",
            locals[at].0.index(),
        );
    }
    for (at, (entity, ..)) in locals.iter().enumerate() {
        world.insert(*entity, worlds[at]);
    }
}
