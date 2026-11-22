use crate::{Entity, LocalTransform, Parent, World};
use belfry_math::Transform;

const VACANT: u32 = u32::MAX;

#[derive(Default)]
pub(crate) struct Propagation {
    applied: u64,
    entities: Vec<Entity>,
    locals: Vec<Transform>,
    parents: Vec<u32>,
    slots: Vec<u32>,
    touched: Vec<u32>,
    offsets: Vec<u32>,
    cursor: Vec<u32>,
    children: Vec<u32>,
    worlds: Vec<Transform>,
    adopted: Vec<bool>,
    stack: Vec<u32>,
}

pub fn propagate(world: &mut World) {
    let mut held = world.remove_resource::<Propagation>().unwrap_or_default();
    held.run(world);
    world.insert_resource(held);
}

impl Propagation {
    fn run(&mut self, world: &mut World) {
        let version = world.hierarchy_change();
        if self.applied == version {
            return;
        }
        self.gather(world);
        self.index();
        self.order(world);
        self.applied = version;
    }

    fn gather(&mut self, world: &World) {
        self.entities.clear();
        self.locals.clear();
        self.parents.clear();
        for (entity, local) in world.iter::<LocalTransform>() {
            self.entities.push(entity);
            self.locals.push(local.0);
            self.parents.push(
                world
                    .get::<Parent>(entity)
                    .map_or(VACANT, |parent| parent.0.index()),
            );
        }
    }

    fn index(&mut self) {
        for index in self.touched.drain(..) {
            self.slots[index as usize] = VACANT;
        }
        let top = self
            .entities
            .iter()
            .map(|entity| entity.index())
            .max()
            .unwrap_or(0) as usize;
        self.slots.resize(top + 1, VACANT);
        for (at, entity) in self.entities.iter().enumerate() {
            let index = entity.index();
            self.slots[index as usize] = at as u32;
            self.touched.push(index);
        }
    }

    fn order(&mut self, world: &mut World) {
        let count = self.entities.len();
        self.offsets.clear();
        self.offsets.resize(count + 1, 0);
        for (at, parent) in self.parents.iter().enumerate() {
            if let Some(parent) = self.parent(*parent, at) {
                self.offsets[parent as usize + 1] += 1;
            }
        }
        for at in 0..count {
            self.offsets[at + 1] += self.offsets[at];
        }
        self.cursor.clear();
        self.cursor.extend_from_slice(&self.offsets[..count]);
        self.children.clear();
        self.children.resize(self.offsets[count] as usize, 0);
        for (at, parent) in self.parents.iter().enumerate() {
            if let Some(parent) = self.parent(*parent, at) {
                self.children[self.cursor[parent as usize] as usize] = at as u32;
                self.cursor[parent as usize] += 1;
            }
        }
        self.worlds.clear();
        self.worlds.resize(count, Transform::IDENTITY);
        self.adopted.clear();
        self.adopted.resize(count, false);
        self.stack.clear();
        for at in 0..count {
            let parent = self.parents[at];
            if self.parent(parent, at).is_none() {
                self.stack.push(at as u32);
            }
        }
        while let Some(at) = self.stack.pop() {
            let at = at as usize;
            let held = self.locals[at];
            let world_at = match self.parents[at] {
                VACANT => held,
                parent => self.worlds[self.slots[parent as usize] as usize].compose(held),
            };
            self.worlds[at] = world_at;
            self.adopted[at] = true;
            let start = self.offsets[at] as usize;
            let end = self.offsets[at + 1] as usize;
            for child in &self.children[start..end] {
                self.stack.push(*child);
            }
        }
        if let Some(at) = self.adopted.iter().position(|held| !held) {
            panic!(
                "the hierarchy closes the parent chain of entity {} into a cycle",
                self.entities[at].index(),
            );
        }
        for (at, entity) in self.entities.iter().enumerate() {
            world.insert(*entity, self.worlds[at]);
        }
    }

    fn parent(&self, parent: u32, at: usize) -> Option<u32> {
        if parent == VACANT {
            return None;
        }
        let slot = self.slots[parent as usize];
        assert!(
            slot != VACANT,
            "the hierarchy parents entity {} to entity {parent}, which holds no local transform",
            self.entities[at].index(),
        );
        assert!(
            slot as usize != at,
            "entity {} parents itself",
            self.entities[at].index(),
        );
        Some(slot)
    }
}
