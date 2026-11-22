use belfry_ecs::Entity;
use belfry_math::Vec3;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ContactCache {
    pairs: HashMap<(Entity, Entity, usize, usize, u32), Vec<CachedPoint>>,
    rebounds: HashMap<(Entity, Entity), f32>,
}

#[derive(Clone, Copy)]
pub(crate) struct CachedPoint {
    pub local: Vec3,
    pub normal: f32,
    pub tangent: [f32; 2],
}

impl ContactCache {
    pub(crate) fn rebound(&self, first: Entity, second: Entity) -> f32 {
        self.rebounds
            .get(&key(first, second))
            .copied()
            .unwrap_or(0.0)
    }

    pub(crate) fn pairs(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        self.pairs.keys().map(|held| (held.0, held.1))
    }

    pub(crate) fn insert_rebound(&mut self, first: Entity, second: Entity, rebound: f32) {
        if rebound > 0.0 {
            self.rebounds.insert(key(first, second), rebound);
        }
    }

    pub(crate) fn take(
        &mut self,
        first: Entity,
        second: Entity,
        first_part: usize,
        second_part: usize,
        feature: u32,
    ) -> Vec<Option<CachedPoint>> {
        self.pairs
            .remove(&contact_key(
                first,
                second,
                first_part,
                second_part,
                feature,
            ))
            .unwrap_or_default()
            .into_iter()
            .map(Some)
            .collect()
    }

    pub(crate) fn insert(
        &mut self,
        first: Entity,
        second: Entity,
        first_part: usize,
        second_part: usize,
        feature: u32,
        points: impl IntoIterator<Item = CachedPoint>,
    ) {
        let held = points.into_iter().collect::<Vec<_>>();
        if !held.is_empty() {
            self.pairs.insert(
                contact_key(first, second, first_part, second_part, feature),
                held,
            );
        }
    }
}

pub(crate) fn contact_key(
    first: Entity,
    second: Entity,
    first_part: usize,
    second_part: usize,
    feature: u32,
) -> (Entity, Entity, usize, usize, u32) {
    if first <= second {
        (first, second, first_part, second_part, feature)
    } else {
        (second, first, second_part, first_part, feature)
    }
}

pub(crate) fn key(first: Entity, second: Entity) -> (Entity, Entity) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}
