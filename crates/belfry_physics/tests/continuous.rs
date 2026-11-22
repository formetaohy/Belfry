use belfry_ecs::{Entity, LocalTransform, World};
use belfry_math::{Quat, Transform, Vec3};
use belfry_physics::{BodyKind, Collider, Gravity, RigidBody, Shape, TriangleMesh, step};
use std::sync::Arc;

const DELTA: f32 = 1.0 / 60.0;

fn world(gravity: Vec3) -> World {
    let mut world = World::new();
    world.insert_resource(Gravity(gravity));
    world
}

fn spawn(
    world: &mut World,
    translation: Vec3,
    shape: Shape,
    kind: BodyKind,
    velocity: Vec3,
    restitution: f32,
) -> Entity {
    let entity = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, translation);
    world.insert(entity, LocalTransform(transform));
    world.insert(entity, transform);
    let mut body = RigidBody::new(kind);
    body.set_linear_velocity(velocity);
    body.set_can_sleep(false);
    world.insert(entity, body);
    let mut collider = Collider::new(shape);
    collider.set_restitution(restitution);
    world.insert(entity, collider);
    entity
}

fn thin_floor(world: &mut World, restitution: f32) -> Entity {
    spawn(
        world,
        Vec3::new(0.0, -0.01, 0.0),
        Shape::cuboid(Vec3::new(10.0, 0.01, 10.0)),
        BodyKind::Static,
        Vec3::ZERO,
        restitution,
    )
}

fn settle(world: &mut World, steps: usize) {
    for _ in 0..steps {
        step(world, DELTA);
    }
}

fn translation(world: &World, entity: Entity) -> Vec3 {
    world
        .get::<Transform>(entity)
        .expect("a simulated body holds its world transform")
        .translation
}

#[test]
fn a_fast_sphere_lands_on_a_thin_floor() {
    let mut world = world(Vec3::new(0.0, -9.81, 0.0));
    thin_floor(&mut world, 0.0);
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 10.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
        Vec3::new(0.0, -200.0, 0.0),
        0.0,
    );
    settle(&mut world, 120);
    let held = translation(&world, sphere);
    assert!(
        held.y > 0.4,
        "a fast sphere of radius 0.5 rests at {} above a floor at 0.01",
        held.y,
    );
}

#[test]
fn a_fast_cuboid_lands_on_a_thin_floor() {
    let mut world = world(Vec3::new(0.0, -9.81, 0.0));
    thin_floor(&mut world, 0.0);
    let cube = spawn(
        &mut world,
        Vec3::new(0.0, 10.0, 0.0),
        Shape::cuboid(Vec3::splat(0.25)),
        BodyKind::Dynamic,
        Vec3::new(0.0, -150.0, 0.0),
        0.0,
    );
    settle(&mut world, 120);
    let held = translation(&world, cube);
    assert!(
        held.y > 0.2,
        "a fast cube of half extent 0.25 rests at {} above a floor at 0.01",
        held.y,
    );
}

#[test]
fn a_fast_sphere_bounces_off_a_thin_floor() {
    let mut world = world(Vec3::ZERO);
    thin_floor(&mut world, 1.0);
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 1.0, 0.0),
        Shape::sphere(0.25),
        BodyKind::Dynamic,
        Vec3::new(0.0, -60.0, 0.0),
        1.0,
    );
    settle(&mut world, 60);
    let held = translation(&world, sphere);
    let velocity = world
        .get::<RigidBody>(sphere)
        .expect("the sphere holds its body")
        .linear_velocity();
    assert!(
        held.y > 0.5,
        "a bouncing sphere climbs from the floor to {}",
        held.y,
    );
    assert!(
        velocity.y > 10.0,
        "a bouncing sphere leaves the floor at {}",
        velocity.y,
    );
}

#[test]
fn a_fast_sphere_lands_on_a_triangle_mesh() {
    let mut world = world(Vec3::ZERO);
    let floor = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, Vec3::ZERO);
    world.insert(floor, LocalTransform(transform));
    world.insert(floor, transform);
    world.insert(floor, RigidBody::new(BodyKind::Static));
    world.insert(
        floor,
        Collider::new(Shape::mesh(Arc::new(TriangleMesh::from_positions(&[
            [-10.0, 0.0, -10.0],
            [-10.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [10.0, 0.0, -10.0],
        ])))),
    );
    let sphere = spawn(
        &mut world,
        Vec3::new(0.0, 10.0, 0.0),
        Shape::sphere(0.5),
        BodyKind::Dynamic,
        Vec3::new(0.0, -200.0, 0.0),
        0.0,
    );
    settle(&mut world, 120);
    let held = translation(&world, sphere);
    assert!(
        held.y > 0.4,
        "a fast sphere rests at {} above a triangle mesh",
        held.y,
    );
}

#[test]
fn a_fast_cuboid_lands_on_a_triangle_mesh() {
    let mut world = world(Vec3::ZERO);
    let floor = world.spawn();
    let transform =
        Transform::from_scale_rotation_translation(Vec3::ONE, Quat::IDENTITY, Vec3::ZERO);
    world.insert(floor, LocalTransform(transform));
    world.insert(floor, transform);
    world.insert(floor, RigidBody::new(BodyKind::Static));
    world.insert(
        floor,
        Collider::new(Shape::mesh(Arc::new(TriangleMesh::from_positions(&[
            [-10.0, 0.0, -10.0],
            [-10.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [10.0, 0.0, -10.0],
        ])))),
    );
    let cube = spawn(
        &mut world,
        Vec3::new(0.0, 10.0, 0.0),
        Shape::cuboid(Vec3::splat(0.25)),
        BodyKind::Dynamic,
        Vec3::new(0.0, -150.0, 0.0),
        0.0,
    );
    settle(&mut world, 120);
    let held = translation(&world, cube);
    assert!(
        held.y > 0.2,
        "a fast cube rests at {} above a triangle mesh",
        held.y,
    );
}
