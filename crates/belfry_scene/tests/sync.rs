use belfry_ecs::{LocalTransform, World};
use belfry_math::{Transform, Vec3};
use belfry_scene::{Assets, Scene, SceneSync, SkinData, Target};
use std::sync::Arc;

struct NoAssets;

impl Assets for NoAssets {
    fn mesh(&self, _id: &str) -> belfry_render::Mesh {
        panic!("a synced scene renders no mesh")
    }

    fn material(&self, _id: &str) -> belfry_render::Material {
        panic!("a synced scene renders no material")
    }

    fn skin(&self, _id: &str) -> SkinData {
        panic!("a synced scene deforms no mesh")
    }

    fn animation(&self, _id: &str) -> Arc<belfry_animation::Clip> {
        panic!("a synced scene plays no animation")
    }
}

#[derive(Debug, PartialEq)]
struct Ticks(u32);

const SCENE: &str = r#"format = "belfry_scene"
images = []
meshes = []
materials = []

[[nodes]]
id = "a"

[nodes.transform]
translation = [0.0, 0.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]

[[nodes]]
id = "b"
parent = "a"

[nodes.transform]
translation = [0.0, 1.0, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
scale = [1.0, 1.0, 1.0]
"#;

#[test]
fn a_scene_sync_reuses_entities_and_reinstalls_changed_components() {
    let mut scene = Scene::parse(SCENE, ".", "the test scene");
    let mut world = World::new();
    let mut sync = SceneSync::new();
    sync.sync(&scene, &mut world, &NoAssets, true);
    let a = sync.id("a").expect("the sync holds the node a");
    let b = sync.id("b").expect("the sync holds the node b");
    world.insert(a, Ticks(7));
    assert_eq!(sync.entities(), [a, b]);

    scene.set_transform(0, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)));
    scene
        .insert_node(belfry_scene::Node::new("c"))
        .expect("the scene adds a node");
    scene
        .remove(&Target::Node(String::from("b")))
        .expect("the scene removes a node");
    assert_eq!(
        scene.transform(0),
        Transform::from_translation(Vec3::new(2.0, 0.0, 0.0))
    );
    sync.sync(&scene, &mut world, &NoAssets, false);

    assert_eq!(sync.id("a"), Some(a));
    assert_eq!(world.get::<Ticks>(a), Some(&Ticks(7)));
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
    );
    let c = sync.id("c").expect("the sync spawns the added node");
    assert!(world.is_alive(c));
    assert!(!world.is_alive(b));
}
