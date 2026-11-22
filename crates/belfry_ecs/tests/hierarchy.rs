use belfry_ecs::{LocalTransform, Parent, World, propagate};
use belfry_math::{Quat, Transform, Vec3};

#[test]
fn a_parent_carries_its_child_into_world_space() {
    let mut world = World::new();
    let root = world.spawn();
    world.insert(
        root,
        LocalTransform(Transform::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::new(1.0, 2.0, 3.0),
        )),
    );
    let child = world.spawn();
    world.insert(
        child,
        LocalTransform(Transform::from_translation(Vec3::new(1.0, 0.0, 0.0))),
    );
    world.insert(child, Parent(root));
    propagate(&mut world);
    let held = *world
        .get::<Transform>(child)
        .expect("the child holds its world transform");
    assert_eq!(held.translation, Vec3::new(3.0, 2.0, 3.0));
    assert_eq!(held.scale, Vec3::splat(2.0));
}
