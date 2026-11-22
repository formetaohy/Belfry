use crate::animation::AnimationEntry;
use crate::environment::{EnvironmentEntry, EnvironmentSource};
use crate::graph::GraphEntry;
use crate::image::{ImageEntry, ImageSource, address_name, filter_name};
use crate::material::MaterialEntry;
use crate::mesh::{Geometry, MeshEntry};
use crate::model::ModelEntry;
use crate::node::Node;
use crate::registry::Registry;
use crate::scene::{FORMAT, Scene};
use crate::schema::ReferenceKind;
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use belfry_render::Vertex;
use belfry_toml::Toml;

pub(crate) fn write(scene: &Scene) -> String {
    let format = if scene.is_prefab() {
        crate::prefab::FORMAT
    } else {
        FORMAT
    };
    let document = Toml::object([
        ("format", Toml::string(format)),
        (
            "models",
            Toml::array(
                scene
                    .models
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Model, &entry.id))
                    .map(model)
                    .collect(),
            ),
        ),
        (
            "images",
            Toml::array(
                scene
                    .images
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Image, &entry.id))
                    .map(image)
                    .collect(),
            ),
        ),
        (
            "sounds",
            Toml::array(
                scene
                    .sounds
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Sound, &entry.id))
                    .map(sound)
                    .collect(),
            ),
        ),
        (
            "environments",
            Toml::array(
                scene
                    .environments
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Environment, &entry.id))
                    .map(environment)
                    .collect(),
            ),
        ),
        (
            "meshes",
            Toml::array(
                scene
                    .meshes
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Mesh, &entry.id))
                    .map(mesh)
                    .collect(),
            ),
        ),
        (
            "materials",
            Toml::array(
                scene
                    .materials
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Material, &entry.id))
                    .map(material)
                    .collect(),
            ),
        ),
        (
            "skins",
            Toml::array(
                scene
                    .skins
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Skin, &entry.id))
                    .map(skin)
                    .collect(),
            ),
        ),
        (
            "animations",
            Toml::array(
                scene
                    .animations
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Animation, &entry.id))
                    .map(animation)
                    .collect(),
            ),
        ),
        (
            "graphs",
            Toml::array(
                scene
                    .graphs
                    .iter()
                    .filter(|entry| !owned(scene, ReferenceKind::Graph, &entry.id))
                    .map(graph)
                    .collect(),
            ),
        ),
        (
            "nodes",
            Toml::array(
                scene
                    .nodes
                    .iter()
                    .filter(|node| node.owner().is_none())
                    .map(|node| self::node(node, scene.registry()))
                    .collect(),
            ),
        ),
    ]);
    document.write()
}

fn owned(scene: &Scene, kind: ReferenceKind, id: &str) -> bool {
    scene.asset_owner(kind, id).is_some()
}

fn model(entry: &ModelEntry) -> Toml {
    Toml::object([
        ("id", Toml::string(entry.id.as_str())),
        (
            "path",
            Toml::string(entry.path.to_str().unwrap_or_else(|| {
                panic!("a model path of {} holds no text", entry.path.display())
            })),
        ),
    ])
}

fn sound(entry: &SoundEntry) -> Toml {
    Toml::object([
        ("id", Toml::string(entry.id.as_str())),
        (
            "path",
            Toml::string(entry.path.to_str().unwrap_or_else(|| {
                panic!("a sound path of {} holds no text", entry.path.display())
            })),
        ),
        ("stream", Toml::boolean(entry.stream)),
    ])
}

fn environment(entry: &EnvironmentEntry) -> Toml {
    let mut fields = vec![("id", Toml::string(entry.id.as_str()))];
    match &entry.source {
        EnvironmentSource::Panorama(path) => fields.push((
            "path",
            Toml::string(
                path.to_str()
                    .unwrap_or_else(|| panic!("an environment path holds no text")),
            ),
        )),
        EnvironmentSource::Atmosphere(atmosphere) => fields.push((
            "atmosphere",
            Toml::object([
                ("sun_elevation", Toml::from(atmosphere.sun_elevation)),
                ("sun_azimuth", Toml::from(atmosphere.sun_azimuth)),
                ("sun_intensity", Toml::from(atmosphere.sun_intensity)),
                ("rayleigh", Toml::from(atmosphere.rayleigh)),
                ("mie", Toml::from(atmosphere.mie)),
                ("mie_anisotropy", Toml::from(atmosphere.mie_anisotropy)),
                ("ozone", Toml::from(atmosphere.ozone)),
                ("ground_albedo", vector(atmosphere.ground_albedo.to_array())),
                ("altitude", Toml::from(atmosphere.altitude)),
            ]),
        )),
    }
    Toml::object(fields)
}

fn image(entry: &ImageEntry) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
    match &entry.source {
        ImageSource::Path(path) => fields.push((
            String::from("path"),
            Toml::string(path.to_str().unwrap_or_else(|| {
                panic!("a scene image path of {} holds no text", path.display())
            })),
        )),
        ImageSource::Model { model, image } => {
            fields.push((String::from("model"), Toml::string(model.as_str())));
            fields.push((String::from("image"), Toml::from(*image)));
        }
    }
    fields.push((
        String::from("min_filter"),
        Toml::string(filter_name(entry.sampler.min_filter)),
    ));
    fields.push((
        String::from("mag_filter"),
        Toml::string(filter_name(entry.sampler.mag_filter)),
    ));
    fields.push((
        String::from("mip_filter"),
        Toml::string(filter_name(entry.sampler.mip_filter)),
    ));
    fields.push((
        String::from("wrap_u"),
        Toml::string(address_name(entry.sampler.address_u)),
    ));
    fields.push((
        String::from("wrap_v"),
        Toml::string(address_name(entry.sampler.address_v)),
    ));
    Toml::object(fields)
}

fn mesh(entry: &MeshEntry) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
    match &entry.asset.geometry {
        Geometry::Shape(shape) => {
            fields.push((String::from("shape"), Toml::string(shape.name())));
        }
        Geometry::Vertices(vertices) => {
            fields.push((
                String::from("vertices"),
                Toml::array(vertices.iter().map(vertex).collect()),
            ));
        }
        Geometry::Model {
            model,
            mesh,
            primitive,
        } => {
            fields.push((String::from("model"), Toml::string(model.as_str())));
            fields.push((String::from("mesh"), Toml::from(*mesh)));
            fields.push((String::from("primitive"), Toml::from(*primitive)));
        }
    }
    Toml::object(fields)
}

fn vertex(vertex: &Vertex) -> Toml {
    Toml::object([
        ("position", vector(vertex.position)),
        ("normal", vector(vertex.normal)),
        ("uv", vector(vertex.uv)),
    ])
}

fn vector<const N: usize>(values: [f32; N]) -> Toml {
    Toml::array(values.into_iter().map(Toml::from).collect())
}

fn material(entry: &MaterialEntry) -> Toml {
    let asset = &entry.asset;
    let mut fields = vec![
        (String::from("id"), Toml::string(entry.id.as_str())),
        (
            String::from("base_color"),
            vector(asset.base_color.to_array()),
        ),
        (String::from("alpha"), Toml::from(asset.alpha)),
        (
            String::from("alpha_mode"),
            Toml::string(asset.alpha_mode.name()),
        ),
        (String::from("alpha_cutoff"), Toml::from(asset.alpha_cutoff)),
        (String::from("metallic"), Toml::from(asset.metallic)),
        (String::from("roughness"), Toml::from(asset.roughness)),
        (String::from("normal_scale"), Toml::from(asset.normal_scale)),
        (
            String::from("occlusion_strength"),
            Toml::from(asset.occlusion_strength),
        ),
        (String::from("emissive"), vector(asset.emissive.to_array())),
    ];
    for (name, image) in [
        ("base_color_image", &asset.base_color_image),
        ("metallic_roughness_image", &asset.metallic_roughness_image),
        ("normal_image", &asset.normal_image),
        ("occlusion_image", &asset.occlusion_image),
        ("emissive_image", &asset.emissive_image),
    ] {
        if let Some(image) = image {
            fields.push((String::from(name), Toml::string(image.as_str())));
        }
    }
    Toml::object(fields)
}

fn skin(entry: &SkinEntry) -> Toml {
    Toml::object([
        ("id", Toml::string(entry.id.as_str())),
        ("model", Toml::string(entry.asset.model_of().as_str())),
        ("skin", Toml::from(entry.asset.index())),
        (
            "joints",
            Toml::array(
                entry
                    .asset
                    .joints()
                    .iter()
                    .map(|joint| Toml::string(joint.as_str()))
                    .collect(),
            ),
        ),
    ])
}

fn graph(entry: &GraphEntry) -> Toml {
    entry.authored()
}

fn animation(entry: &AnimationEntry) -> Toml {
    Toml::object([
        ("id", Toml::string(entry.id.as_str())),
        ("model", Toml::string(entry.asset.model_of().as_str())),
        ("animation", Toml::from(entry.asset.index())),
        (
            "targets",
            Toml::array(
                entry
                    .asset
                    .targets()
                    .iter()
                    .map(|target| Toml::string(target.as_str()))
                    .collect(),
            ),
        ),
    ])
}

fn node(node: &Node, registry: &Registry) -> Toml {
    let mut fields = vec![(String::from("id"), Toml::string(&node.id))];
    if let Some(parent) = &node.parent {
        fields.push((String::from("parent"), Toml::string(parent)));
    }
    if let Some(prefab) = node.prefab() {
        fields.push((
            String::from("prefab"),
            Toml::string(
                prefab.to_str().unwrap_or_else(|| {
                    panic!("a prefab path of {} holds no text", prefab.display())
                }),
            ),
        ));
    }
    if !node.overrides().is_empty() {
        fields.push((
            String::from("overrides"),
            Toml::array(
                node.overrides()
                    .iter()
                    .map(|entry| {
                        Toml::object([
                            ("node", Toml::string(entry.node.as_str())),
                            ("path", Toml::string(entry.path.as_str())),
                            ("value", entry.value.to_toml()),
                        ])
                    })
                    .collect(),
            ),
        ));
    }
    if !node.additions().is_empty() {
        fields.push((
            String::from("additions"),
            Toml::array(
                node.additions()
                    .iter()
                    .map(|addition| self::node(addition, registry))
                    .collect(),
            ),
        ));
    }
    if !node.removals().is_empty() {
        fields.push((
            String::from("removals"),
            Toml::array(
                node.removals()
                    .iter()
                    .map(|removal| Toml::string(removal.as_str()))
                    .collect(),
            ),
        ));
    }
    for component in registry.components() {
        if let Some(held) = node.component_by_name(component.name) {
            fields.push((String::from(component.name), held.authored()));
        }
    }
    Toml::object(fields)
}
