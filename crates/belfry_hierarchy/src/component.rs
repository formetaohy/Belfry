use belfry_ecs::Entity;
use belfry_math::Transform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTransform(pub Transform);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);
