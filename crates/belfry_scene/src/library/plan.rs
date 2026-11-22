use super::prepare::{
    ImageBytes, Prepare, animation_data, animation_fingerprint, atmosphere_fingerprint,
    collided_meshes, environment_data, environment_fingerprint, fail, image_data_of,
    image_fingerprint, image_source, material_fingerprint, mesh_data, mesh_fingerprint, skin_data,
    skin_fingerprint, sound_data, sound_fingerprint, validate_material,
};
use super::work::{
    ImageMips, Job, Prepared, PreparedAnimation, PreparedEnvironment, PreparedImage,
    PreparedMaterial, PreparedMesh, PreparedSkin, PreparedSound, Removed, format_of, image_modes,
};
use crate::environment::EnvironmentSource;
use crate::library::TextureKey;
use belfry_image::{ColorSpace, mip_chain};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

pub(super) fn plan(job: Job) -> Prepared {
    let Job {
        scene,
        fingerprints,
        textures: cached_textures,
        sources,
        models,
        environment_cache,
    } = job;
    let mut errors = Vec::new();
    let mut files = BTreeSet::new();
    let mut prepare = Prepare::new(&scene, sources, models);
    let collided = collided_meshes(&scene);

    let mut effective_images = HashMap::new();
    let mut pending_images = Vec::new();
    let mut changed_images = HashSet::new();
    for entry in scene.images() {
        let source = image_source(
            entry,
            &scene,
            &mut prepare.sources,
            &mut files,
            &mut prepare.models,
        );
        let fingerprint = image_fingerprint(entry, source.as_ref().ok().copied());
        let previous = fingerprints.images.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            effective_images.insert(entry.id.clone(), fingerprint);
            continue;
        }
        let prepared = source.and_then(|_| {
            image_data_of(
                &scene,
                &entry.id,
                &mut prepare.images,
                &mut prepare.models,
                &mut prepare.sources,
            )
        });
        match prepared {
            Ok(_) => {
                effective_images.insert(entry.id.clone(), fingerprint);
                changed_images.insert(entry.id.clone());
                pending_images.push((entry.clone(), fingerprint));
            }
            Err(message) => {
                fail(&mut errors, "image", &entry.id, message, previous.is_none());
                if let Some(previous) = previous {
                    effective_images.insert(entry.id.clone(), previous);
                }
            }
        }
    }

    let mut pending_materials = Vec::new();
    for entry in scene.materials() {
        let mut dependencies = Vec::new();
        let mut missing = None;
        for image in entry.asset.images().into_iter().flatten() {
            match effective_images.get(image.as_str()) {
                Some(fingerprint) => dependencies.push(*fingerprint),
                None => {
                    missing = Some(String::from(image.as_str()));
                    break;
                }
            }
        }
        let fingerprint = material_fingerprint(entry, &dependencies);
        let previous = fingerprints.materials.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        if let Some(image) = missing {
            fail(
                &mut errors,
                "material",
                &entry.id,
                format!(
                    "the material {} samples the image {image}, which the scene builds none",
                    entry.id,
                ),
                previous.is_none(),
            );
            continue;
        }
        if let Err(message) = validate_material(entry) {
            fail(
                &mut errors,
                "material",
                &entry.id,
                message,
                previous.is_none(),
            );
            continue;
        }
        pending_materials.push((entry.clone(), fingerprint));
    }

    let mut modes: HashMap<String, HashSet<ColorSpace>> = HashMap::new();
    for id in &changed_images {
        modes
            .entry(id.clone())
            .or_default()
            .insert(ColorSpace::Srgb);
    }
    for (entry, _) in &pending_materials {
        for (id, color) in image_modes(entry) {
            let Some(id) = id else {
                continue;
            };
            let key: TextureKey = (String::from(id), format_of(color), color);
            if changed_images.contains(id) || !cached_textures.contains(&key) {
                modes.entry(String::from(id)).or_default().insert(color);
            }
        }
    }
    let mut textures = HashMap::new();
    for (id, colors) in modes {
        let data = match prepare.images.get(&id) {
            Some(data) => Arc::clone(data),
            None => match image_data_of(
                &scene,
                &id,
                &mut prepare.images,
                &mut prepare.models,
                &mut prepare.sources,
            ) {
                Ok(data) => data,
                Err(_) => continue,
            },
        };
        textures.insert(id, Arc::new(mips(&data, &colors)));
    }
    prepare.images.clear();

    let mut images = Vec::new();
    for (entry, fingerprint) in pending_images {
        let Some(mips) = textures.get(&entry.id).cloned() else {
            continue;
        };
        images.push(PreparedImage {
            entry,
            fingerprint,
            mips,
        });
    }

    pending_materials.retain(|(entry, _)| {
        let missing = image_modes(entry).into_iter().find_map(|(id, color)| {
            let id = id?;
            let key = (String::from(id), format_of(color), color);
            (!cached_textures.contains(&key) && !textures.contains_key(id)).then_some(id)
        });
        match missing {
            Some(image) => {
                let previous = fingerprints.materials.contains_key(&entry.id);
                fail(
                    &mut errors,
                    "material",
                    &entry.id,
                    format!(
                        "the material {} samples the image {image}, which the scene builds none",
                        entry.id
                    ),
                    !previous,
                );
                false
            }
            None => true,
        }
    });

    let mut meshes = Vec::new();
    for entry in scene.meshes() {
        let wanted = collided.contains(entry.id.as_str());
        let model = match &entry.asset.geometry {
            crate::mesh::Geometry::Model { model, .. } => prepare.models.digest(
                model.as_str(),
                super::prepare::Scope::Geometry,
                &mut prepare.sources,
                &mut files,
            ),
            crate::mesh::Geometry::Shape(_) | crate::mesh::Geometry::Vertices(_) => {
                Ok(belfry_asset::Digest::default())
            }
        };
        let fingerprint = mesh_fingerprint(entry, model.as_ref().ok().copied(), wanted);
        let previous = fingerprints.meshes.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared =
            model.and_then(|_| mesh_data(entry, wanted, &mut prepare.models, &mut prepare.sources));
        match prepared {
            Ok(data) => meshes.push(PreparedMesh {
                entry: entry.clone(),
                fingerprint,
                data,
            }),
            Err(message) => fail(&mut errors, "mesh", &entry.id, message, previous.is_none()),
        }
    }

    let mut skins = Vec::new();
    for entry in scene.skins() {
        let model = prepare.models.digest(
            entry.asset.model_of().as_str(),
            super::prepare::Scope::Geometry,
            &mut prepare.sources,
            &mut files,
        );
        let fingerprint = skin_fingerprint(entry, model.as_ref().ok().copied());
        let previous = fingerprints.skins.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = model.and_then(|_| {
            let model = prepare
                .models
                .load(entry.asset.model_of().as_str(), &mut prepare.sources)?;
            skin_data(entry, model)
        });
        match prepared {
            Ok(data) => skins.push(PreparedSkin {
                entry: entry.clone(),
                fingerprint,
                data,
            }),
            Err(message) => fail(&mut errors, "skin", &entry.id, message, previous.is_none()),
        }
    }

    let mut animations = Vec::new();
    for entry in scene.animations() {
        let model = prepare.models.digest(
            entry.asset.model_of().as_str(),
            super::prepare::Scope::Geometry,
            &mut prepare.sources,
            &mut files,
        );
        let fingerprint = animation_fingerprint(entry, model.as_ref().ok().copied());
        let previous = fingerprints.animations.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = model.and_then(|_| {
            let model = prepare
                .models
                .load(entry.asset.model_of().as_str(), &mut prepare.sources)?;
            animation_data(entry, model)
        });
        match prepared {
            Ok(clip) => animations.push(PreparedAnimation {
                entry: entry.clone(),
                fingerprint,
                clip,
            }),
            Err(message) => fail(
                &mut errors,
                "animation",
                &entry.id,
                message,
                previous.is_none(),
            ),
        }
    }

    let mut sounds = Vec::new();
    for entry in scene.sounds() {
        let file = scene.root().join(&entry.path);
        files.insert(file.clone());
        let digest = prepare.sources.digest(&file);
        let fingerprint = sound_fingerprint(entry, digest.as_ref().ok().copied());
        let previous = fingerprints.sounds.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let prepared = digest.and_then(|_| sound_data(&scene, entry, &mut prepare.sources));
        match prepared {
            Ok(source) => sounds.push(PreparedSound {
                entry: entry.clone(),
                fingerprint,
                source,
            }),
            Err(message) => fail(&mut errors, "sound", &entry.id, message, previous.is_none()),
        }
    }

    let mut environments = Vec::new();
    for entry in scene.environments() {
        let digest = match &entry.source {
            EnvironmentSource::Panorama(path) => {
                let file = scene.root().join(path);
                files.insert(file.clone());
                prepare.sources.digest(&file)
            }
            EnvironmentSource::Atmosphere(atmosphere) => Ok(atmosphere_fingerprint(atmosphere)),
        };
        let fingerprint = environment_fingerprint(entry, digest.as_ref().ok().copied());
        let previous = fingerprints.environments.get(&entry.id).copied();
        if previous == Some(fingerprint) {
            continue;
        }
        let key = digest.as_ref().ok().map(|digest| match &entry.source {
            EnvironmentSource::Panorama(_) => format!(
                "v{}/{}.env",
                belfry_environment::CACHE_VERSION,
                digest.hex(),
            ),
            EnvironmentSource::Atmosphere(_) => format!(
                "v{}/atmosphere-{}.env",
                belfry_environment::CACHE_VERSION,
                digest.hex(),
            ),
        });
        let cached = key
            .as_ref()
            .and_then(|key| environment_cache.load(key))
            .and_then(|bytes| belfry_environment::decode(&bytes));
        let prepared = match cached {
            Some(image) => Ok((image, false)),
            None => digest
                .as_ref()
                .map_err(|message| message.clone())
                .and_then(|_| environment_data(&scene, entry, &mut prepare.sources))
                .map(|image| (image, true)),
        };
        match prepared {
            Ok((image, computed)) => {
                if computed && let Some(key) = &key {
                    environment_cache.store(key, &belfry_environment::encode(&image));
                }
                environments.push(PreparedEnvironment {
                    entry: entry.clone(),
                    fingerprint,
                    image,
                });
            }
            Err(message) => fail(
                &mut errors,
                "environment",
                &entry.id,
                message,
                previous.is_none(),
            ),
        }
    }

    let removed = Removed {
        images: absent(&fingerprints.images, scene.images(), |entry| {
            entry.id.as_str()
        }),
        materials: absent(&fingerprints.materials, scene.materials(), |entry| {
            entry.id.as_str()
        }),
        meshes: absent(&fingerprints.meshes, scene.meshes(), |entry| {
            entry.id.as_str()
        }),
        skins: absent(&fingerprints.skins, scene.skins(), |entry| {
            entry.id.as_str()
        }),
        animations: absent(&fingerprints.animations, scene.animations(), |entry| {
            entry.id.as_str()
        }),
        sounds: absent(&fingerprints.sounds, scene.sounds(), |entry| {
            entry.id.as_str()
        }),
        environments: absent(&fingerprints.environments, scene.environments(), |entry| {
            entry.id.as_str()
        }),
    };
    let changed = removed.any()
        || !images.is_empty()
        || !meshes.is_empty()
        || !pending_materials.is_empty()
        || !skins.is_empty()
        || !animations.is_empty()
        || !sounds.is_empty()
        || !environments.is_empty();
    let applied = !errors.iter().any(|error| error.fatal);

    prepare.models.retain();
    let mut prepared = Prepared {
        applied,
        changed: applied && changed,
        errors,
        images: Vec::new(),
        materials: Vec::new(),
        meshes: Vec::new(),
        skins: Vec::new(),
        animations: Vec::new(),
        sounds: Vec::new(),
        environments: Vec::new(),
        textures: HashMap::new(),
        changed_images: HashSet::new(),
        removed: Removed::default(),
        sources: prepare.sources,
        models: prepare.models.take_cache(),
        files,
    };
    if !applied {
        return prepared;
    }
    prepared.images = images;
    prepared.materials = pending_materials
        .into_iter()
        .map(|(entry, fingerprint)| PreparedMaterial { entry, fingerprint })
        .collect();
    prepared.meshes = meshes;
    prepared.skins = skins;
    prepared.animations = animations;
    prepared.sounds = sounds;
    prepared.environments = environments;
    prepared.textures = textures;
    prepared.changed_images = changed_images;
    prepared.removed = removed;
    prepared
}

fn mips(data: &ImageBytes, colors: &HashSet<ColorSpace>) -> ImageMips {
    ImageMips {
        width: data.width,
        height: data.height,
        srgb: mip_chain(data.width, data.height, ColorSpace::Srgb, &data.rgba),
        linear: colors
            .contains(&ColorSpace::Linear)
            .then(|| mip_chain(data.width, data.height, ColorSpace::Linear, &data.rgba)),
    }
}

fn absent<T>(
    held: &HashMap<String, belfry_asset::Digest>,
    entries: &[T],
    id: impl Fn(&T) -> &str,
) -> Vec<String> {
    let present = entries
        .iter()
        .map(|entry| String::from(id(entry)))
        .collect::<HashSet<_>>();
    held.keys()
        .filter(|id| !present.contains(*id))
        .cloned()
        .collect()
}
