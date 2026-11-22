use belfry_ecs::{LocalTransform, Parent, World};
use belfry_math::{Transform, Vec3};
use belfry_render::SceneRevision;
use belfry_scene::{Assets, Changed, Scene, SceneSync, SkinData, Target};
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
fn a_scene_sync_follows_the_structure_it_edits() {
    let mut scene = Scene::parse(SCENE, ".", "the test scene");
    let mut world = World::new();
    let mut sync = SceneSync::new();
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
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
    let b_at = scene.node_index("b").expect("the scene holds b");
    let c_at = scene.node_index("c").expect("the scene holds c");
    edit(&mut scene, |edit| {
        edit.reparent(b_at, Some(c_at));
        Ok(())
    })
    .expect("the scene reparents the node b");
    for (at, node) in scene.nodes().iter().enumerate() {
        assert_eq!(
            scene.node_index(&node.id),
            Some(at),
            "the index names the slot of {}",
            node.id,
        );
    }
    assert_eq!(scene.children(c_at).collect::<Vec<_>>(), [b_at]);
    assert_eq!(scene.subtree(c_at), [b_at, c_at]);

    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert_eq!(sync.id("a"), Some(a));
    assert_eq!(sync.id("b"), Some(b));
    assert_eq!(world.get::<Ticks>(a), Some(&Ticks(7)));
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
    );
    let c = sync.id("c").expect("the sync spawns the added node");
    assert!(world.is_alive(c));
    assert_eq!(world.get::<Parent>(b), Some(&Parent(c)));

    edit(&mut scene, |edit| {
        edit.remove(&Target::Node(String::from("b")))
    })
    .expect("the scene removes the node b");
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert!(!world.is_alive(b));

    let a_at = scene.node_index("a").expect("the scene holds a");
    edit(&mut scene, |edit| {
        edit.set_transform(a_at, Transform::from_translation(Vec3::new(3.0, 0.0, 0.0)))
    })
    .expect("the scene moves the node a again");
    let revision = world
        .resource::<SceneRevision>()
        .copied()
        .expect("a synced scene carries its revision");
    sync.apply(&scene, &mut world, &NoAssets, a_at);
    assert_eq!(
        world.resource::<SceneRevision>().copied(),
        Some(SceneRevision(revision.0 + 1)),
        "an applied node advances the revision the probes rebake on",
    );
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            3.0, 0.0, 0.0
        )))),
        "an applied node installs the component its scene changed",
    );
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
    sync.sync(&snapshot, &mut world, &NoAssets, &Changed::none());
    let a = sync.id("a").expect("the sync holds the node a");
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::IDENTITY)),
        "an edited scene rewrites the snapshot it already copied",
    );
    sync.sync(&scene, &mut world, &NoAssets, &Changed::none());
    assert_eq!(
        world.get::<LocalTransform>(a),
        Some(&LocalTransform(Transform::from_translation(Vec3::new(
            2.0, 0.0, 0.0
        )))),
        "the edited scene installs the transform its node holds",
    );
}
