use crate::scene::Scene;
use crate::schema::{
    ENVIRONMENT_ATMOSPHERE_FIELDS, ENVIRONMENT_PANORAMA_FIELDS, Field, FieldKind, IMAGE_FIELDS,
    MATERIAL_FIELDS, MESH_FIELDS, MODEL_FIELDS, ReferenceKind,
};
use crate::target::Target;
use belfry_toml::Toml;

const VERTEX_FIELDS: &[Field] = &[Field {
    name: "vertices",
    label: "Vertices",
    kind: FieldKind::Number {
        range: [0.0, 100000000.0],
        speed: 1.0,
    },
}];

impl Scene {
    pub fn observe(&self) -> Toml {
        let models = self
            .models
            .iter()
            .map(|entry| {
                let target = Target::Model(entry.id.clone());
                let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
                fields.extend(self.owned_field(ReferenceKind::Model, &entry.id));
                fields.extend(self.fields(&target, "", MODEL_FIELDS));
                Toml::object(fields)
            })
            .collect();
        let images = self
            .images
            .iter()
            .map(|entry| {
                let target = Target::Image(entry.id.clone());
                let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
                fields.extend(self.owned_field(ReferenceKind::Image, &entry.id));
                fields.extend(self.fields(&target, "", IMAGE_FIELDS));
                Toml::object(fields)
            })
            .collect();
        let sounds = self
            .sounds
            .iter()
            .map(|entry| {
                let mut fields = vec![
                    (String::from("id"), Toml::string(entry.id.as_str())),
                    (
                        String::from("path"),
                        Toml::string(
                            entry
                                .path
                                .to_str()
                                .unwrap_or_else(|| panic!("a sound path holds no text")),
                        ),
                    ),
                    (String::from("stream"), Toml::boolean(entry.stream)),
                ];
                fields.extend(self.owned_field(ReferenceKind::Sound, &entry.id));
                Toml::object(fields)
            })
            .collect();
        let environments = self
            .environments
            .iter()
            .map(|entry| {
                let target = Target::Environment(entry.id.clone());
                let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
                fields.extend(self.owned_field(ReferenceKind::Environment, &entry.id));
                fields.extend(self.fields(&target, "", ENVIRONMENT_PANORAMA_FIELDS));
                fields.extend(self.fields(&target, "", ENVIRONMENT_ATMOSPHERE_FIELDS));
                Toml::object(fields)
            })
            .collect();
        let meshes = self
            .meshes
            .iter()
            .map(|entry| {
                let target = Target::Mesh(entry.id.clone());
                let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
                fields.extend(self.owned_field(ReferenceKind::Mesh, &entry.id));
                fields.extend(self.fields(&target, "", MESH_FIELDS));
                fields.extend(self.fields(&target, "", VERTEX_FIELDS));
                Toml::object(fields)
            })
            .collect();
        let materials = self
            .materials
            .iter()
            .map(|entry| {
                let target = Target::Material(entry.id.clone());
                let mut fields = vec![(String::from("id"), Toml::string(entry.id.as_str()))];
                fields.extend(self.owned_field(ReferenceKind::Material, &entry.id));
                fields.extend(self.fields(&target, "", MATERIAL_FIELDS));
                Toml::object(fields)
            })
            .collect();
        let nodes = self
            .nodes
            .iter()
            .map(|entry| {
                let mut fields = vec![
                    (String::from("id"), Toml::string(entry.id.as_str())),
                    (
                        String::from("parent"),
                        Toml::string(entry.parent.as_deref().unwrap_or("")),
                    ),
                ];
                if let Some(prefab) = entry.prefab() {
                    fields.push((
                        String::from("prefab"),
                        Toml::string(
                            prefab
                                .to_str()
                                .unwrap_or_else(|| panic!("a prefab path holds no text")),
                        ),
                    ));
                }
                if let Some(owner) = entry.owner() {
                    fields.push((String::from("owner"), Toml::string(owner)));
                }
                if !entry.overrides().is_empty() {
                    fields.push((
                        String::from("overrides"),
                        Toml::array(
                            entry
                                .overrides()
                                .iter()
                                .map(|override_entry| {
                                    Toml::object([
                                        ("node", Toml::string(override_entry.node.as_str())),
                                        ("path", Toml::string(override_entry.path.as_str())),
                                        ("value", override_entry.value.to_toml()),
                                    ])
                                })
                                .collect(),
                        ),
                    ));
                }
                if !entry.additions().is_empty() {
                    fields.push((
                        String::from("additions"),
                        Toml::array(
                            entry
                                .additions()
                                .iter()
                                .map(|addition| Toml::string(addition.id.as_str()))
                                .collect(),
                        ),
                    ));
                }
                if !entry.removals().is_empty() {
                    fields.push((
                        String::from("removals"),
                        Toml::array(
                            entry
                                .removals()
                                .iter()
                                .map(|removal| Toml::string(removal.as_str()))
                                .collect(),
                        ),
                    ));
                }
                for component in self.registry.components() {
                    if let Some(held) = entry.component_by_name(component.name) {
                        fields.push((String::from(component.name), held.observed()));
                    }
                }
                Toml::object(fields)
            })
            .collect();
        let graphs = self.graphs.iter().map(|entry| entry.authored()).collect();
        Toml::object([
            ("models", Toml::array(models)),
            ("images", Toml::array(images)),
            ("sounds", Toml::array(sounds)),
            ("environments", Toml::array(environments)),
            ("graphs", Toml::array(graphs)),
            ("meshes", Toml::array(meshes)),
            ("materials", Toml::array(materials)),
            ("nodes", Toml::array(nodes)),
        ])
    }

    fn owned_field(&self, kind: ReferenceKind, id: &str) -> Option<(String, Toml)> {
        let prefab = self.asset_owner(kind, id)?;
        Some((
            String::from("prefab"),
            Toml::string(
                prefab
                    .to_str()
                    .unwrap_or_else(|| panic!("a prefab path holds no text")),
            ),
        ))
    }

    fn fields(&self, target: &Target, prefix: &str, fields: &[Field]) -> Vec<(String, Toml)> {
        fields
            .iter()
            .filter_map(|field| {
                let value = self.property(target, &format!("{prefix}{}", field.name))?;
                Some((String::from(field.name), value.to_toml()))
            })
            .collect()
    }
}
