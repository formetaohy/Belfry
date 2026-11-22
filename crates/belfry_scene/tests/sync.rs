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
fn a_scene_index_follows_structure_edits() {
    let mut scene = Scene::parse(SCENE, ".", "the test scene");
    edit(&mut scene, |edit| {
        edit.insert_node(belfry_scene::Node::new("c"))
    })
    .expect("the scene adds the node c");
    let b = scene.node_index("b").expect("the scene holds b");
    let c = scene.node_index("c").expect("the scene holds c");
    edit(&mut scene, |edit| {
        edit.reparent(b, Some(c));
        Ok(())
    })
    .expect("the scene reparents the node b");
    assert_eq!(scene.children(c).collect::<Vec<_>>(), [b]);
    assert_eq!(scene.subtree(c), [b, c]);
    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("a")))
    })
    .expect("the scene removes the node a");
    for (at, node) in scene.nodes().iter().enumerate() {
        assert_eq!(
            scene.node_index(&node.id),
            Some(at),
            "the index names the slot of {}",
            node.id,
        );
    }
    assert_eq!(
        scene
            .children(scene.node_index("c").expect("the scene holds c"))
            .collect::<Vec<_>>(),
        [0],
    );
}

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

    edit(&mut scene, |edit| {
        edit.set_transform(0, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a");
    edit(&mut scene, |edit| {
        edit.insert_node(belfry_scene::Node::new("c"))
    })
    .expect("the scene adds a node");
    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("b")))
    })
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
fn edit<T>(
    scene: &mut belfry_scene::Scene,
    change: impl FnOnce(&mut belfry_scene::Edit<'_>) -> Result<T, String>,
) -> Result<T, String> {
    scene.edit(change).map(|(value, _)| value)
}

#[test]
fn a_shared_scene_snapshot_keeps_the_nodes_it_copied() {
    let mut scene = Scene::parse(SCENE, ".", "the test scene");
    let snapshot = scene.clone();
    edit(&mut scene, |edit| {
        edit.set_transform(0, Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a");
    let mut world = World::new();
    let mut sync = SceneSync::new();
    sync.sync(&snapshot, &mut world, &NoAssets, false);
    let a = sync.id("a").expect("the sync holds the node a");
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::IDENTITY)),
        "an edited scene rewrites the snapshot it already copied",
    );
    sync.sync(&scene, &mut world, &NoAssets, false);
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
        "the edited scene installs the transform its node holds",
    );
}
