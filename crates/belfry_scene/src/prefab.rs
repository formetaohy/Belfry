use crate::animation::AnimationEntry;
use crate::graph::GraphEntry;
use crate::image::{ImageEntry, ImageSource};
use crate::material::MaterialEntry;
use crate::mesh::MeshEntry;
use crate::model::ModelEntry;
use crate::node::Node;
use crate::registry::Registry;
use crate::scene::{Owned, Scene};
use crate::schema::{FieldKind, ReferenceKind};
use crate::skin::SkinEntry;
use crate::sound::SoundEntry;
use crate::value::Value;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub(crate) const FORMAT: &str = "belfry_prefab";

pub(crate) struct Loader {
    top: PathBuf,
    registry: Arc<Registry>,
    stack: Vec<PathBuf>,
    cache: HashMap<PathBuf, Scene>,
}

impl Loader {
    pub(crate) fn new(top: PathBuf, registry: Arc<Registry>) -> Self {
        Self {
            top,
            registry,
            stack: Vec::new(),
            cache: HashMap::new(),
        }
    }

    pub(crate) fn document(&mut self, text: &str, root: PathBuf, what: &str) -> Scene {
        let mut scene =
            crate::parse::document(text, root.clone(), what, Arc::clone(&self.registry));
        self.expand(&mut scene, &root, what);
        scene.validate(what);
        scene
    }

    pub(crate) fn load(
        &mut self,
        from: &Path,
        path: &Path,
        what: &str,
    ) -> Result<(Scene, PathBuf), String> {
        if path.as_os_str().is_empty() || !path.is_relative() {
            return Err(format!(
                "the prefab path {} is no relative path",
                path.display()
            ));
        }
        let file = normalize(&from.join(path));
        let key = relative(&self.top, &file);
        assert!(
            !self.stack.iter().any(|held| held == &key),
            "{what} includes the prefab {} into itself",
            key.display(),
        );
        if let Some(cached) = self.cache.get(&key) {
            return Ok((cached.clone(), key));
        }
        let text = fs::read_to_string(&file)
            .map_err(|error| format!("the prefab {} reads no text: {error}", key.display()))?;
        let root = file.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        self.stack.push(key.clone());
        let scene = self.document(&text, root, &format!("the prefab {}", key.display()));
        self.stack.pop();
        if !scene.is_prefab() {
            return Err(format!(
                "the prefab {} holds the scene format",
                key.display(),
            ));
        }
        self.cache.insert(key.clone(), scene.clone());
        Ok((scene, key))
    }

    fn expand(&mut self, scene: &mut Scene, root: &Path, what: &str) {
        let ids = scene
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let mut materialization = Materialization {
            root,
            what,
            ids,
            nodes: Vec::with_capacity(scene.nodes.len()),
        };
        for node in std::mem::take(&mut scene.nodes) {
            let prefab = node.prefab.clone();
            materialization.nodes.push(node.clone());
            let Some(path) = prefab else {
                continue;
            };
            let (template, key) = self
                .load(root, &path, what)
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            let from = root_of(root, &path);
            import(scene, &template, &key, &from, root, &node.id)
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            scene
                .instances
                .insert(node.id.clone(), owners(&template, &key));
            for (nested, held) in &template.instances {
                scene
                    .instances
                    .insert(format!("{}_{nested}", node.id), held.clone());
            }
            let generated = instantiate(&node, &template, &mut materialization.ids, &from, root)
                .unwrap_or_else(|error| panic!("{what}: {error}"));
            materialization.nodes.extend(generated);
            materialization.structure(self, scene, &node);
            apply_removals(scene, &node, &mut materialization.nodes, what);
        }
        scene.nodes = materialization.nodes;
        apply_overrides(scene);
    }
}

struct Materialization<'a> {
    root: &'a Path,
    what: &'a str,
    ids: HashSet<String>,
    nodes: Vec<Node>,
}

impl Materialization<'_> {
    fn structure(&mut self, loader: &mut Loader, scene: &mut Scene, marker: &Node) {
        for addition in marker.additions() {
            self.addition(loader, scene, marker, addition);
        }
    }

    fn addition(&mut self, loader: &mut Loader, scene: &mut Scene, marker: &Node, addition: &Node) {
        let prefix = format!("{}_", marker.id);
        let mut held = addition.clone();
        held.id = format!("{prefix}{}", addition.id);
        assert!(
            self.ids.insert(held.id.clone()),
            "{} materializes the addition {} of {}, which the document holds",
            self.what,
            addition.id,
            marker.id,
        );
        held.parent = Some(match &addition.parent {
            Some(parent) => format!("{prefix}{parent}"),
            None => marker.id.clone(),
        });
        held.owner = Some(marker.id.clone());
        let prefab = held.prefab.clone();
        self.nodes.push(held.clone());
        let Some(path) = prefab else {
            return;
        };
        let (template, key) = loader
            .load(self.root, &path, self.what)
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        let from = root_of(self.root, &path);
        import(scene, &template, &key, &from, self.root, &held.id)
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        let keys = owners(&template, &key);
        scene.instances.insert(held.id.clone(), keys.clone());
        for (nested, held_keys) in &template.instances {
            scene
                .instances
                .insert(format!("{}_{nested}", held.id), held_keys.clone());
        }
        if let Some(outer) = scene.instances.get_mut(&marker.id) {
            outer.extend(keys);
        }
        let generated = instantiate(&held, &template, &mut self.ids, &from, self.root)
            .unwrap_or_else(|error| panic!("{}: {error}", self.what));
        self.nodes.extend(generated);
    }
}

fn apply_removals(scene: &mut Scene, marker: &Node, nodes: &mut Vec<Node>, what: &str) {
    if marker.removals().is_empty() {
        return;
    }
    let prefix = format!("{}_", marker.id);
    let parent = nodes
        .iter()
        .filter_map(|node| {
            node.parent
                .as_deref()
                .map(|parent| (node.id.as_str(), parent))
        })
        .collect::<HashMap<_, _>>();
    let mut targets = Vec::new();
    let mut removed = HashSet::new();
    for removal in marker.removals() {
        let id = format!("{prefix}{removal}");
        if !nodes.iter().any(|node| node.id == id) {
            continue;
        }
        targets.push(id.clone());
        let mut stack = vec![id];
        while let Some(held) = stack.pop() {
            if !removed.insert(held.clone()) {
                continue;
            }
            stack.extend(
                nodes
                    .iter()
                    .filter(|node| node.parent.as_deref() == Some(held.as_str()))
                    .map(|node| node.id.clone()),
            );
        }
    }
    if removed.is_empty() {
        return;
    }
    let mut kept = Vec::new();
    for removal in marker.removals() {
        let id = format!("{prefix}{removal}");
        let subsumed = targets
            .iter()
            .any(|target| target != &id && descendant(&parent, &id, target));
        if targets.contains(&id) && !subsumed {
            kept.push(removal.clone());
        }
    }
    nodes.retain(|node| !removed.contains(&node.id));
    for id in &removed {
        scene.instances.remove(id);
    }
    let at = nodes
        .iter()
        .position(|node| node.id == marker.id)
        .unwrap_or_else(|| panic!("{what} removes the prefab instance {}", marker.id));
    let held = &mut nodes[at];
    held.removals = kept;
    held.additions
        .retain(|addition| !removed.contains(&format!("{prefix}{}", addition.id)));
    held.overrides
        .retain(|entry| !removed.contains(&format!("{prefix}{}", entry.node)));
}

fn descendant(parent: &HashMap<&str, &str>, of: &str, ancestor: &str) -> bool {
    let mut at = parent.get(of).copied();
    while let Some(held) = at {
        if held == ancestor {
            return true;
        }
        at = parent.get(held).copied();
    }
    false
}

pub(crate) fn apply_overrides(scene: &mut Scene) {
    let mut markers = scene
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.prefab().is_some() && !node.overrides().is_empty())
        .map(|(index, _)| (depth(scene, index), index))
        .collect::<Vec<_>>();
    markers.sort_by_key(|(depth, _)| Reverse(*depth));
    for (_, marker) in markers {
        let marker_id = scene.nodes()[marker].id.clone();
        let overrides = scene.nodes()[marker].overrides().to_vec();
        let mut applied = Vec::new();
        for entry in overrides {
            let id = format!("{marker_id}_{}", entry.node);
            let Some(index) = scene.node_index(&id) else {
                continue;
            };
            scene
                .apply_override(index, &entry.path, &entry.value)
                .unwrap_or_else(|error| {
                    panic!(
                        "the prefab instance {marker_id} overrides {} of {}: {error}",
                        entry.path, entry.node,
                    )
                });
            applied.push(entry);
        }
        scene.nodes[marker].overrides = applied;
    }
}

fn depth(scene: &Scene, index: usize) -> usize {
    let mut depth = 0;
    let mut at = Some(index);
    while let Some(current) = at {
        depth += 1;
        at = scene.nodes()[current]
            .owner()
            .and_then(|owner| scene.node_index(owner));
    }
    depth
}

pub(crate) fn instantiate(
    marker: &Node,
    template: &Scene,
    ids: &mut HashSet<String>,
    from: &Path,
    target: &Path,
) -> Result<Vec<Node>, String> {
    let prefix = format!("{}_", marker.id);
    let mut nodes = Vec::with_capacity(template.nodes.len());
    for node in template.nodes() {
        let mut held = node.clone();
        held.id = format!("{prefix}{}", node.id);
        if !ids.insert(held.id.clone()) {
            return Err(format!(
                "the prefab instance {} generates the node {}, which the document holds",
                marker.id, held.id,
            ));
        }
        held.parent = match &node.parent {
            Some(parent) => Some(format!("{prefix}{parent}")),
            None => Some(marker.id.clone()),
        };
        held.owner = Some(match &node.owner {
            Some(owner) => format!("{prefix}{owner}"),
            None => marker.id.clone(),
        });
        if let Some(prefab) = &mut held.prefab {
            *prefab = rebase(from, target, prefab);
        }
        for component in held.components.values_mut() {
            component.remap(&prefix);
        }
        remap_overrides(template, &mut held, &prefix);
        nodes.push(held);
    }
    Ok(nodes)
}

fn remap_overrides(template: &Scene, node: &mut Node, prefix: &str) {
    for entry in &mut node.overrides {
        let Some((component, rest)) = entry.path.split_once('.') else {
            continue;
        };
        let field = rest.split('.').next().unwrap_or(rest);
        let Some(registration) = template.registry().registration(component) else {
            continue;
        };
        let Some(field) = registration
            .spec
            .fields
            .iter()
            .find(|held| held.name == field)
        else {
            continue;
        };
        let kind = match field.kind {
            FieldKind::Reference(kind) | FieldKind::OptionalReference(kind) => kind,
            _ => continue,
        };
        if !matches!(
            kind,
            ReferenceKind::Node
                | ReferenceKind::Skin
                | ReferenceKind::Animation
                | ReferenceKind::Graph
                | ReferenceKind::AudioBus
        ) {
            continue;
        }
        let Value::Text(id) = &mut entry.value else {
            continue;
        };
        if !id.is_empty() {
            *id = format!("{prefix}{id}");
        }
    }
}

pub(crate) fn owners(template: &Scene, key: &Path) -> BTreeSet<PathBuf> {
    let mut keys = template
        .owners
        .iter()
        .map(|held| held.prefab.clone())
        .collect::<BTreeSet<_>>();
    keys.insert(key.to_path_buf());
    keys
}

pub(crate) fn import(
    scene: &mut Scene,
    template: &Scene,
    key: &Path,
    from: &Path,
    target: &Path,
    scope: &str,
) -> Result<(), String> {
    let prefix = format!("{scope}_");
    for entry in template.images() {
        let mut entry = entry.clone();
        if let ImageSource::Path(path) = &mut entry.source {
            *path = rebase(from, target, path);
        }
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Image,
                source: &source,
                scope: None,
            },
            entry,
            |scene| &mut scene.images,
        )?;
    }
    for entry in template.sounds() {
        let mut entry = entry.clone();
        entry.path = rebase(from, target, &entry.path);
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Sound,
                source: &source,
                scope: None,
            },
            entry,
            |scene| &mut scene.sounds,
        )?;
    }
    for entry in template.environments() {
        let mut entry = entry.clone();
        if let crate::environment::EnvironmentSource::Panorama(path) = &mut entry.source {
            *path = rebase(from, target, path);
        }
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Environment,
                source: &source,
                scope: None,
            },
            entry,
            |scene| &mut scene.environments,
        )?;
    }
    for entry in template.meshes() {
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Mesh,
                source: &source,
                scope: None,
            },
            entry.clone(),
            |scene| &mut scene.meshes,
        )?;
    }
    for entry in template.materials() {
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Material,
                source: &source,
                scope: None,
            },
            entry.clone(),
            |scene| &mut scene.materials,
        )?;
    }
    for entry in template.models() {
        let mut entry = entry.clone();
        entry.path = rebase(from, target, &entry.path);
        let source = String::from(entry.asset_id());
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Model,
                source: &source,
                scope: None,
            },
            entry,
            |scene| &mut scene.models,
        )?;
    }
    for entry in template.skins() {
        let source = entry.id.clone();
        let held = SkinEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Skin,
                source: &source,
                scope: Some(scope),
            },
            held,
            |scene| &mut scene.skins,
        )?;
    }
    for entry in template.animations() {
        let source = entry.id.clone();
        let held = AnimationEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Animation,
                source: &source,
                scope: Some(scope),
            },
            held,
            |scene| &mut scene.animations,
        )?;
    }
    for entry in template.graphs() {
        let source = entry.id.clone();
        let held = GraphEntry::new(format!("{prefix}{source}"), entry.asset.rebased(&prefix));
        import_entry(
            scene,
            Import {
                template,
                key,
                kind: ReferenceKind::Graph,
                source: &source,
                scope: Some(scope),
            },
            held,
            |scene| &mut scene.graphs,
        )?;
    }
    Ok(())
}

struct Import<'a> {
    template: &'a Scene,
    key: &'a Path,
    kind: ReferenceKind,
    source: &'a str,
    scope: Option<&'a str>,
}

trait Asset {
    fn asset_id(&self) -> &str;
}

impl Asset for ImageEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for crate::environment::EnvironmentEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for SoundEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for MeshEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for MaterialEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for ModelEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for SkinEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for AnimationEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

impl Asset for GraphEntry {
    fn asset_id(&self) -> &str {
        &self.id
    }
}

fn import_entry<T: Asset + Clone + PartialEq>(
    scene: &mut Scene,
    import: Import<'_>,
    entry: T,
    mut entries: impl FnMut(&mut Scene) -> &mut Vec<T>,
) -> Result<(), String> {
    let Import {
        template,
        key,
        kind,
        source,
        scope,
    } = import;
    let id = String::from(entry.asset_id());
    let owner = template
        .asset_owner(kind, source)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| key.to_path_buf());
    match scene.asset_owner(kind, &id).map(Path::to_path_buf) {
        Some(held) if held == owner => {
            let items = entries(scene);
            let at = items
                .iter()
                .position(|held| held.asset_id() == id)
                .expect("an imported asset keeps the document entry it names");
            items[at] = entry;
            let record = scene
                .owners
                .iter_mut()
                .find(|held| held.kind == kind && held.id == id)
                .expect("an imported asset keeps the owner it records");
            record.scope = scope.map(String::from);
        }
        Some(held) => {
            return Err(format!(
                "the {} {id} of the prefab {} already belongs to {}",
                kind.name(),
                owner.display(),
                held.display(),
            ));
        }
        None => {
            let items = entries(scene);
            if let Some(at) = items.iter().position(|held| held.asset_id() == id) {
                if items[at] == entry {
                    return Ok(());
                }
                return Err(format!(
                    "the {} {id} of the prefab {} differs from the entry of the document",
                    kind.name(),
                    owner.display(),
                ));
            }
            items.push(entry);
            scene.owners.push(Owned {
                kind,
                id,
                prefab: owner,
                scope: scope.map(String::from),
            });
        }
    }
    Ok(())
}

pub(crate) fn root_of(base: &Path, path: &Path) -> PathBuf {
    normalize(&base.join(path))
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf)
}

fn rebase(from: &Path, to: &Path, path: &Path) -> PathBuf {
    relative(to, from).join(path)
}

pub(crate) fn relative(base: &Path, path: &Path) -> PathBuf {
    let base = normalize(base);
    let path = normalize(path);
    let common = base
        .components()
        .zip(path.components())
        .take_while(|(left, right)| left == right)
        .count();
    let mut out = PathBuf::new();
    for _ in common..base.components().count() {
        out.push("..");
    }
    for part in path.components().skip(common) {
        out.push(part.as_os_str());
    }
    out
}

pub(crate) fn describe(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return String::from(*text);
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    String::from("an unknown panic")
}

pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                _ => parts.push(Component::ParentDir),
            },
            held => parts.push(held),
        }
    }
    let mut out = PathBuf::new();
    for part in parts {
        out.push(part.as_os_str());
    }
    out
}
