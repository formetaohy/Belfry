use crate::field::{axis_value, no_axis, number, split, unit, with_axis};
use crate::image::{ImageSource, address_name, address_of, filter_name, filter_of};
use crate::mesh::MeshAsset;
use crate::scene::Scene;
use crate::schema::ReferenceKind;
use crate::shape::Shape;
use crate::target::Target;
use crate::value::{Effect, Value};
use belfry_render::AlphaMode;
use std::path::PathBuf;

impl Scene {
    pub fn property(&self, target: &Target, path: &str) -> Option<Value> {
        let (field, axis) = split(path);
        let value = match target {
            Target::Node(id) => {
                return self
                    .node_index(id)
                    .and_then(|at| self.node_property(at, path));
            }
            Target::Material(id) => self
                .material_index(id)
                .and_then(|at| material_property(&self.materials[at].asset, field)),
            Target::Mesh(id) => self
                .mesh_index(id)
                .and_then(|at| mesh_property(&self.meshes[at].asset, field)),
            Target::Image(id) => self
                .image_index(id)
                .and_then(|at| image_property(&self.images[at], field)),
            Target::Model(id) => self
                .model_index(id)
                .and_then(|at| model_property(&self.models[at].path, field)),
            Target::Sound(id) => self
                .sound_index(id)
                .and_then(|at| sound_property(&self.sounds[at], field)),
            Target::Environment(id) => self
                .environment_index(id)
                .and_then(|at| environment_property(&self.environments[at], field)),
            Target::Graph(id) => self
                .graph_index(id)
                .and_then(|at| self.graphs[at].asset.read(path)),
        }?;
        axis_value(value, axis)
    }

    pub fn set_property(
        &mut self,
        target: &Target,
        path: &str,
        value: &Value,
    ) -> Result<Effect, String> {
        if !matches!(target, Target::Node(_))
            && let Some(prefab) = self.prefab_owner(target)
        {
            return Err(format!(
                "{target} belongs to the prefab {}",
                prefab.display(),
            ));
        }
        match target {
            Target::Node(id) => {
                let index = self
                    .node_index(id)
                    .ok_or_else(|| format!("the scene holds no node {id}"))?;
                if self.nodes[index].owner.is_some() {
                    self.write_override(index, path, value)
                } else {
                    self.set_node(index, path, value)
                }
            }
            Target::Material(id) => {
                let index = self
                    .material_index(id)
                    .ok_or_else(|| format!("the scene holds no material {id}"))?;
                let (field, axis) = split(path);
                self.set_material(index, field, axis, value)
            }
            Target::Mesh(id) => {
                let index = self
                    .mesh_index(id)
                    .ok_or_else(|| format!("the scene holds no mesh {id}"))?;
                let (field, axis) = split(path);
                self.set_mesh(index, field, axis, value)
            }
            Target::Image(id) => {
                let index = self
                    .image_index(id)
                    .ok_or_else(|| format!("the scene holds no image {id}"))?;
                let (field, axis) = split(path);
                self.set_image(index, field, axis, value)
            }
            Target::Model(id) => {
                let index = self
                    .model_index(id)
                    .ok_or_else(|| format!("the scene holds no model {id}"))?;
                let (field, axis) = split(path);
                self.set_model(index, field, axis, value)
            }
            Target::Sound(id) => {
                let index = self
                    .sound_index(id)
                    .ok_or_else(|| format!("the scene holds no sound {id}"))?;
                let (field, axis) = split(path);
                self.set_sound(index, field, axis, value)
            }
            Target::Environment(id) => {
                let index = self
                    .environment_index(id)
                    .ok_or_else(|| format!("the scene holds no environment {id}"))?;
                let (field, axis) = split(path);
                self.set_environment(index, field, axis, value)
            }
            Target::Graph(id) => {
                let index = self
                    .graph_index(id)
                    .ok_or_else(|| format!("the scene holds no graph {id}"))?;
                let Scene {
                    graphs, animations, ..
                } = self;
                graphs[index]
                    .asset
                    .write(path, value, |clip| {
                        animations.iter().any(|animation| animation.id == clip)
                    })
                    .map(|()| Effect::Structure)
            }
        }
    }

    fn node_property(&self, index: usize, path: &str) -> Option<Value> {
        let node = self.nodes.get(index)?;
        if path == "parent" {
            return Some(Value::Text(node.parent.clone().unwrap_or_default()));
        }
        let (component, rest) = path.split_once('.').unwrap_or((path, ""));
        if rest.is_empty() {
            return self
                .registry
                .registration(component)
                .map(|_| Value::Bool(node.has_component(component)));
        }
        let (field, axis) = split(rest);
        let held = node.component_by_name(component)?;
        axis_value(held.read(field)?, axis)
    }

    pub(crate) fn set_node(
        &mut self,
        index: usize,
        path: &str,
        value: &Value,
    ) -> Result<Effect, String> {
        let id = self
            .nodes
            .get(index)
            .ok_or_else(|| String::from("the scene holds no node"))?
            .id
            .clone();
        if self.nodes[index].prefab.is_some() && path != "parent" && !path.starts_with("transform.")
        {
            return Err(format!(
                "the prefab instance {id} holds its transform and parent alone",
            ));
        }
        if path == "parent" {
            let name = value.text()?;
            let parent = if name.is_empty() {
                None
            } else {
                let at = self
                    .node_index(name)
                    .ok_or_else(|| format!("the scene holds no node {name}"))?;
                if self.is_ancestor(index, at) {
                    return Err(format!("{id} parents itself through {name}"));
                }
                Some(at)
            };
            self.reparent(index, parent);
            return Ok(Effect::Structure);
        }
        let (component, rest) = path.split_once('.').unwrap_or((path, ""));
        let required = self
            .registry
            .registration(component)
            .map(|registration| registration.spec.required)
            .ok_or_else(|| format!("the node {id} holds no component {component}"))?;
        if rest.is_empty() {
            let wanted = value.boolean()?;
            if wanted {
                if self.nodes[index].has_component(component) {
                    return Ok(Effect::Structure);
                }
                let created = self.registry.create(self, component)?;
                created.validate(self, &id)?;
                let before = (component == "audio_bus").then(|| self.nodes[index].clone());
                self.nodes.at_mut(index).insert_component_box(created);
                if let Some(before) = before
                    && let Err(error) = self.check_audio_buses()
                {
                    *self.nodes.at_mut(index) = before;
                    return Err(error);
                }
            } else {
                if !self.nodes[index].has_component(component) {
                    return Ok(Effect::Structure);
                }
                if required {
                    return Err(format!("the node {id} keeps its {component}"));
                }
                if component == "audio_bus" {
                    if let Some(effect) = ["audio_filter", "audio_compressor", "audio_reverb"]
                        .into_iter()
                        .find(|effect| self.nodes[index].has_component(effect))
                    {
                        return Err(format!("the node {id} keeps its {effect}"));
                    }
                    if self.referenced_by(ReferenceKind::AudioBus, &id) {
                        return Err(format!("the scene references the audio bus {id}"));
                    }
                }
                let before = (component == "audio_bus").then(|| self.nodes[index].clone());
                self.nodes.at_mut(index).remove_component(component);
                if let Some(before) = before
                    && let Err(error) = self.check_audio_buses()
                {
                    *self.nodes.at_mut(index) = before;
                    return Err(error);
                }
            }
            return Ok(Effect::Structure);
        }
        let (field, axis) = split(rest);
        let mut next = self.nodes[index]
            .component_by_name(component)
            .ok_or_else(|| format!("the node {id} holds no {component}"))?
            .clone_box();
        let effect = next.write(field, axis, value)?;
        next.validate(self, &id)?;
        let before = (component == "audio_bus").then(|| self.nodes[index].clone());
        self.nodes.at_mut(index).insert_component_box(next);
        if let Some(before) = before
            && let Err(error) = self.check_audio_buses()
        {
            *self.nodes.at_mut(index) = before;
            return Err(error);
        }
        Ok(effect)
    }

    fn set_material(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        if matches!(
            field,
            "base_color_image"
                | "metallic_roughness_image"
                | "normal_image"
                | "occlusion_image"
                | "emissive_image"
        ) {
            no_axis(axis, "the material image")?;
            let image = String::from(value.text()?);
            let held = if image.is_empty() {
                None
            } else {
                if self.image_index(&image).is_none() {
                    return Err(format!("the scene holds no image {image}"));
                }
                Some(crate::reference::ImageId::new(image))
            };
            let asset = &mut self.materials[index].asset;
            match field {
                "base_color_image" => asset.base_color_image = held,
                "metallic_roughness_image" => asset.metallic_roughness_image = held,
                "normal_image" => asset.normal_image = held,
                "occlusion_image" => asset.occlusion_image = held,
                _ => asset.emissive_image = held,
            }
            return Ok(Effect::Structure);
        }
        let material = &mut self.materials[index];
        let id = &material.id;
        let asset = &mut material.asset;
        match field {
            "base_color" | "emissive" => {
                let subject = if field == "base_color" {
                    "base color"
                } else {
                    "emissive"
                };
                let held = if field == "base_color" {
                    &mut asset.base_color
                } else {
                    &mut asset.emissive
                };
                let color = with_axis(
                    *held,
                    axis,
                    value,
                    &format!("the {subject} of material {id}"),
                )?;
                unit(color, &format!("the {subject} of material {id}"))?;
                *held = color;
            }
            "metallic" => {
                no_axis(axis, "the material metallic")?;
                let metallic = number(value, &format!("the metallic of material {id}"))?;
                if !(0.0..=1.0).contains(&metallic) {
                    return Err(format!("the material {id} holds metallic {metallic}"));
                }
                asset.metallic = metallic;
            }
            "alpha" => {
                no_axis(axis, "the material alpha")?;
                let alpha = number(value, &format!("the alpha of material {id}"))?;
                if !(0.0..=1.0).contains(&alpha) {
                    return Err(format!("the material {id} holds alpha {alpha}"));
                }
                asset.alpha = alpha;
            }
            "alpha_mode" => {
                no_axis(axis, "the material alpha mode")?;
                let name = value.text()?;
                asset.alpha_mode = AlphaMode::of(name)
                    .ok_or_else(|| format!("the material {id} holds the alpha mode {name}"))?;
            }
            "alpha_cutoff" => {
                no_axis(axis, "the material alpha cutoff")?;
                let cutoff = number(value, &format!("the alpha cutoff of material {id}"))?;
                if !(0.0..=1.0).contains(&cutoff) {
                    return Err(format!("the material {id} holds alpha cutoff {cutoff}"));
                }
                asset.alpha_cutoff = cutoff;
            }
            "roughness" => {
                no_axis(axis, "the material roughness")?;
                let roughness = number(value, &format!("the roughness of material {id}"))?;
                if roughness <= 0.0 || roughness > 1.0 {
                    return Err(format!("the material {id} holds roughness {roughness}"));
                }
                asset.roughness = roughness;
            }
            "normal_scale" => {
                no_axis(axis, "the material normal scale")?;
                let scale = number(value, &format!("the normal scale of material {id}"))?;
                if !scale.is_finite() || scale < 0.0 {
                    return Err(format!("the material {id} holds normal scale {scale}"));
                }
                asset.normal_scale = scale;
            }
            "occlusion_strength" => {
                no_axis(axis, "the material occlusion strength")?;
                let strength = number(value, &format!("the occlusion strength of material {id}"))?;
                if !(0.0..=1.0).contains(&strength) {
                    return Err(format!(
                        "the material {id} holds occlusion strength {strength}"
                    ));
                }
                asset.occlusion_strength = strength;
            }
            other => return Err(format!("the material {id} holds no property {other}")),
        }
        Ok(Effect::Structure)
    }

    fn set_mesh(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        no_axis(axis, "the mesh shape")?;
        let mesh = &mut self.meshes[index];
        if field != "shape" {
            return Err(format!("the mesh {} holds no property {field}", mesh.id));
        }
        let name = value.text()?;
        let shape = Shape::from_name(name)
            .ok_or_else(|| format!("the mesh {} holds the shape {name}", mesh.id))?;
        mesh.asset = MeshAsset::shape(shape);
        Ok(Effect::Structure)
    }

    fn set_image(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        let image = &mut self.images[index];
        let id = &image.id;
        match field {
            "path" => {
                no_axis(axis, "the image path")?;
                let ImageSource::Path(held) = &mut image.source else {
                    return Err(format!("the image {id} reads its bytes from its model"));
                };
                let path = PathBuf::from(value.text()?);
                if path.as_os_str().is_empty() || !path.is_relative() {
                    return Err(format!("the image {id} holds the path {}", path.display()));
                }
                if !self.root.join(&path).is_file() {
                    return Err(format!(
                        "the image {id} names {}, which the scene root {} does not hold",
                        path.display(),
                        self.root.display(),
                    ));
                }
                *held = path;
            }
            "min_filter" | "mag_filter" | "mip_filter" | "wrap_u" | "wrap_v" => {
                no_axis(axis, "the image sampler")?;
                let name = value.text()?;
                match field {
                    "min_filter" => {
                        image.sampler.min_filter = filter_of(name)
                            .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                    }
                    "mag_filter" => {
                        image.sampler.mag_filter = filter_of(name)
                            .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                    }
                    "mip_filter" => {
                        image.sampler.mip_filter = filter_of(name)
                            .ok_or_else(|| format!("the image {id} holds the filter {name}"))?;
                    }
                    "wrap_u" => {
                        image.sampler.address_u = address_of(name)
                            .ok_or_else(|| format!("the image {id} holds the wrap {name}"))?;
                    }
                    _ => {
                        image.sampler.address_v = address_of(name)
                            .ok_or_else(|| format!("the image {id} holds the wrap {name}"))?;
                    }
                }
            }
            other => return Err(format!("the image {id} holds no property {other}")),
        }
        Ok(Effect::Structure)
    }

    fn set_environment(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        let entry = &mut self.environments[index];
        let id = entry.id.clone();
        match &mut entry.source {
            crate::environment::EnvironmentSource::Panorama(path) => {
                no_axis(axis, "the environment path")?;
                if field != "path" {
                    return Err(format!("the environment {id} holds no property {field}"));
                }
                let next = PathBuf::from(value.text()?);
                if next.as_os_str().is_empty() || !next.is_relative() {
                    return Err(format!(
                        "the environment {id} holds the path {}",
                        next.display(),
                    ));
                }
                if !self.root.join(&next).is_file() {
                    return Err(format!(
                        "the environment {id} names {}, which the scene root {} does not hold",
                        next.display(),
                        self.root.display(),
                    ));
                }
                *path = next;
            }
            crate::environment::EnvironmentSource::Atmosphere(atmosphere) => {
                let what = |subject: &str| format!("the {subject} of environment {id}");
                let parameter = |axis: Option<crate::field::Axis>, value: &Value| {
                    no_axis(axis, &format!("the {field} of environment {id}"))?;
                    number(value, &what(field))
                };
                match field {
                    "sun_elevation" => {
                        let held = parameter(axis, value)?;
                        if !(-90.0..=90.0).contains(&held) {
                            return Err(format!(
                                "the environment {id} holds the sun elevation {held}",
                            ));
                        }
                        atmosphere.sun_elevation = held;
                    }
                    "sun_azimuth" => {
                        let held = parameter(axis, value)?;
                        if !(0.0..=360.0).contains(&held) {
                            return Err(format!(
                                "the environment {id} holds the sun azimuth {held}",
                            ));
                        }
                        atmosphere.sun_azimuth = held;
                    }
                    "sun_intensity" => {
                        let held = parameter(axis, value)?;
                        if held <= 0.0 {
                            return Err(format!(
                                "the environment {id} holds the sun intensity {held}",
                            ));
                        }
                        atmosphere.sun_intensity = held;
                    }
                    "rayleigh" => {
                        let held = parameter(axis, value)?;
                        if held < 0.0 {
                            return Err(format!(
                                "the environment {id} holds the Rayleigh scale {held}",
                            ));
                        }
                        atmosphere.rayleigh = held;
                    }
                    "mie" => {
                        let held = parameter(axis, value)?;
                        if held < 0.0 {
                            return Err(
                                format!("the environment {id} holds the Mie scale {held}",),
                            );
                        }
                        atmosphere.mie = held;
                    }
                    "mie_anisotropy" => {
                        let held = parameter(axis, value)?;
                        if !(-1.0..1.0).contains(&held) {
                            return Err(format!(
                                "the environment {id} holds the Mie anisotropy {held}",
                            ));
                        }
                        atmosphere.mie_anisotropy = held;
                    }
                    "ozone" => {
                        let held = parameter(axis, value)?;
                        if held < 0.0 {
                            return Err(format!(
                                "the environment {id} holds the ozone scale {held}",
                            ));
                        }
                        atmosphere.ozone = held;
                    }
                    "ground_albedo" => {
                        let held = with_axis(
                            atmosphere.ground_albedo,
                            axis,
                            value,
                            &what("ground albedo"),
                        )?;
                        unit(held, &what("ground albedo"))?;
                        atmosphere.ground_albedo = held;
                    }
                    "altitude" => {
                        let held = parameter(axis, value)?;
                        if held < 0.0 {
                            return Err(format!("the environment {id} holds the altitude {held}",));
                        }
                        atmosphere.altitude = held;
                    }
                    _ => {
                        return Err(format!("the environment {id} holds no property {field}",));
                    }
                }
            }
        }
        Ok(Effect::Structure)
    }

    fn set_sound(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        if field == "stream" {
            no_axis(axis, "the sound stream")?;
            self.sounds[index].stream = value.boolean()?;
            return Ok(Effect::Structure);
        }
        no_axis(axis, "the sound path")?;
        let sound = &mut self.sounds[index];
        if field != "path" {
            return Err(format!("the sound {} holds no property {field}", sound.id));
        }
        let path = PathBuf::from(value.text()?);
        if path.as_os_str().is_empty() || !path.is_relative() {
            return Err(format!(
                "the sound {} holds the path {}",
                sound.id,
                path.display()
            ));
        }
        if !self.root.join(&path).is_file() {
            return Err(format!(
                "the sound {} names {}, which the scene root {} does not hold",
                sound.id,
                path.display(),
                self.root.display(),
            ));
        }
        sound.path = path;
        Ok(Effect::Structure)
    }

    fn set_model(
        &mut self,
        index: usize,
        field: &str,
        axis: Option<crate::field::Axis>,
        value: &Value,
    ) -> Result<Effect, String> {
        no_axis(axis, "the model path")?;
        let model = &mut self.models[index];
        if field != "path" {
            return Err(format!("the model {} holds no property {field}", model.id));
        }
        let path = PathBuf::from(value.text()?);
        if path.as_os_str().is_empty() || !path.is_relative() {
            return Err(format!(
                "the model {} holds the path {}",
                model.id,
                path.display()
            ));
        }
        if !self.root.join(&path).is_file() {
            return Err(format!(
                "the model {} names {}, which the scene root {} does not hold",
                model.id,
                path.display(),
                self.root.display(),
            ));
        }
        model.path = path;
        Ok(Effect::Structure)
    }
}

fn material_property(asset: &crate::material::MaterialAsset, field: &str) -> Option<Value> {
    let image = |held: &Option<crate::reference::ImageId>| {
        Some(Value::Text(
            held.as_ref()
                .map_or(String::new(), |id| id.as_str().to_owned()),
        ))
    };
    match field {
        "base_color" => Some(Value::Vector(asset.base_color)),
        "alpha" => Some(Value::Number(asset.alpha)),
        "alpha_mode" => Some(Value::Text(String::from(asset.alpha_mode.name()))),
        "alpha_cutoff" => Some(Value::Number(asset.alpha_cutoff)),
        "base_color_image" => image(&asset.base_color_image),
        "metallic" => Some(Value::Number(asset.metallic)),
        "roughness" => Some(Value::Number(asset.roughness)),
        "metallic_roughness_image" => image(&asset.metallic_roughness_image),
        "normal_image" => image(&asset.normal_image),
        "normal_scale" => Some(Value::Number(asset.normal_scale)),
        "occlusion_image" => image(&asset.occlusion_image),
        "occlusion_strength" => Some(Value::Number(asset.occlusion_strength)),
        "emissive" => Some(Value::Vector(asset.emissive)),
        "emissive_image" => image(&asset.emissive_image),
        _ => None,
    }
}

fn mesh_property(asset: &MeshAsset, field: &str) -> Option<Value> {
    match field {
        "shape" => asset
            .shape_of()
            .map(|shape| Value::Text(String::from(shape.name()))),
        "vertices" => match &asset.geometry {
            crate::mesh::Geometry::Vertices(vertices) => Some(Value::Number(vertices.len() as f32)),
            crate::mesh::Geometry::Shape(_) | crate::mesh::Geometry::Model { .. } => None,
        },
        _ => None,
    }
}

fn image_property(entry: &crate::image::ImageEntry, field: &str) -> Option<Value> {
    match field {
        "path" => entry
            .source
            .path()
            .map(|path| Value::Text(path.display().to_string())),
        "min_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.min_filter,
        )))),
        "mag_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.mag_filter,
        )))),
        "mip_filter" => Some(Value::Text(String::from(filter_name(
            entry.sampler.mip_filter,
        )))),
        "wrap_u" => Some(Value::Text(String::from(address_name(
            entry.sampler.address_u,
        )))),
        "wrap_v" => Some(Value::Text(String::from(address_name(
            entry.sampler.address_v,
        )))),
        _ => None,
    }
}

fn model_property(path: &std::path::Path, field: &str) -> Option<Value> {
    match field {
        "path" => Some(Value::Text(path.display().to_string())),
        _ => None,
    }
}

fn environment_property(
    entry: &crate::environment::EnvironmentEntry,
    field: &str,
) -> Option<Value> {
    match &entry.source {
        crate::environment::EnvironmentSource::Panorama(path) => {
            (field == "path").then(|| Value::Text(path.display().to_string()))
        }
        crate::environment::EnvironmentSource::Atmosphere(atmosphere) => match field {
            "sun_elevation" => Some(Value::Number(atmosphere.sun_elevation)),
            "sun_azimuth" => Some(Value::Number(atmosphere.sun_azimuth)),
            "sun_intensity" => Some(Value::Number(atmosphere.sun_intensity)),
            "rayleigh" => Some(Value::Number(atmosphere.rayleigh)),
            "mie" => Some(Value::Number(atmosphere.mie)),
            "mie_anisotropy" => Some(Value::Number(atmosphere.mie_anisotropy)),
            "ozone" => Some(Value::Number(atmosphere.ozone)),
            "ground_albedo" => Some(Value::Vector(atmosphere.ground_albedo)),
            "altitude" => Some(Value::Number(atmosphere.altitude)),
            _ => None,
        },
    }
}

fn sound_property(entry: &crate::sound::SoundEntry, field: &str) -> Option<Value> {
    match field {
        "path" => Some(Value::Text(entry.path.display().to_string())),
        "stream" => Some(Value::Bool(entry.stream)),
        _ => None,
    }
}
