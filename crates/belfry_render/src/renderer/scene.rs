use super::bounds::{joint_record, joint_world, skinned_bounds, static_bounds};
use super::*;

pub(super) struct Collection {
    pub(super) entities: Vec<Entity>,
    pub(super) batches: Vec<Batch>,
    pub(super) bases: Vec<u32>,
    pub(super) slots: u32,
    pub(super) depth_batches: u32,
    pub(super) depth_count: u32,
    pub(super) total: u32,
    pub(super) joint_bytes: u64,
    pub(super) spheres: Vec<(Vec3, f32)>,
    pub(super) highlight: u32,
    pub(super) records: Vec<InstanceRecord>,
}

impl Renderer {
    pub(super) fn collect(&mut self, world: &World, highlight: Option<Entity>) -> Collection {
        let mut gathered = Vec::new();
        world.for_each_pair::<Transform, MeshInstance>(|entity, transform, instance| {
            if world.contains::<Hidden>(entity) {
                return;
            }
            gathered.push((
                instance.mesh,
                instance.material,
                entity,
                *transform,
                *instance,
                self.materials.mode(instance.material) == AlphaMode::Blend,
            ));
        });
        let mut opaque = Vec::new();
        let mut transparent = Vec::new();
        for held in gathered {
            if held.5 {
                transparent.push(held);
            } else {
                opaque.push(held);
            }
        }
        opaque.sort_by_key(|(mesh, _, entity, ..)| (mesh.index(), entity.index()));
        transparent.sort_by_key(|(mesh, _, entity, ..)| (mesh.index(), entity.index()));
        let depth_count = opaque.len() as u32;
        let mut gathered = opaque;
        gathered.extend(transparent);
        let mut batches = Vec::new();
        let mut batch_of = Vec::with_capacity(gathered.len());
        let mut previous = None;
        let mut depth_batches = 0u32;
        for (mesh, _, _, _, _, blended) in &gathered {
            if previous != Some((*mesh, *blended)) {
                previous = Some((*mesh, *blended));
                batches.push(Batch {
                    mesh: *mesh,
                    count: 0,
                });
                if !blended {
                    depth_batches += 1;
                }
            }
            let batch = batches.len() - 1;
            batches[batch].count += 1;
            batch_of.push(batch as u32);
        }
        let counts = batches.iter().map(|batch| batch.count).collect::<Vec<_>>();
        let total = gathered.len() as u32;
        let highlight = highlight
            .and_then(|held| {
                gathered
                    .iter()
                    .position(|(_, _, entity, ..)| *entity == held)
            })
            .map_or(NO_HIGHLIGHT, |at| at as u32);
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let (bases, slots) = bases_of(&counts, span::blocks(alignment, DRAW_BYTES));
        self.palettes.begin();
        let mut poses = Vec::with_capacity(gathered.len());
        let mut previous_palettes = Vec::with_capacity(gathered.len());
        let mut scratch = Vec::new();
        for (_, _, entity, transform, instance, _) in &gathered {
            let geometry = self.meshes.geometry(instance.mesh);
            let Some(skin) = world.get::<Skin>(*entity) else {
                poses.push(palette::NONE);
                previous_palettes.push(0);
                continue;
            };
            assert!(
                geometry.skin_count > 0,
                "a skinned instance renders a mesh without skin weights",
            );
            assert_eq!(
                skin.joints().len(),
                skin.inverse_binds().len(),
                "a skin holds {} joints for {} inverse bind matrices",
                skin.joints().len(),
                skin.inverse_binds().len(),
            );
            let model = transform.matrix();
            let inverse = model.inverse();
            scratch.clear();
            for (joint, bind) in skin.joints().iter().zip(skin.inverse_binds()) {
                let joint = joint_world(world, *joint);
                scratch.push(inverse * joint * *bind);
            }
            let (skinning, previous) = self.palettes.push(*entity, &scratch);
            poses.push(skinning);
            previous_palettes.push(previous);
        }
        self.palettes.commit();
        let palette = self.palettes.matrices();
        let joint_bytes = u64::from(palette.len().max(1) as u32) * JOINT_BYTES;
        self.joints.fit(&self.device, joint_bytes);
        let reallocated = self.previous_joints.fit(&self.device, joint_bytes);
        if !palette.is_empty() {
            let records = palette.iter().map(joint_record).collect::<Vec<_>>();
            self.joints
                .buffer()
                .write(&self.queue, bytemuck::cast_slice(&records));
        }
        let total_joints = palette.len() as u32;
        let mut records = Vec::with_capacity(gathered.len());
        for (index, (_, _, entity, transform, instance, _)) in gathered.iter().enumerate() {
            let geometry = self.meshes.geometry(instance.mesh);
            let skinning = poses[index];
            let bounds = if skinning.joints > 0 {
                let at = skinning.palette as usize;
                let held = &palette[at..at + skinning.joints as usize];
                skinned_bounds(geometry, *transform, held)
            } else {
                static_bounds(geometry, *transform)
            };
            let previous = self
                .previous
                .get(entity)
                .copied()
                .unwrap_or_else(|| transform.matrix());
            let carried = previous_palettes[index];
            let previous_palette = if !reallocated && carried + skinning.joints <= total_joints {
                carried
            } else {
                skinning.palette
            };
            records.push(record_of(
                *entity,
                *transform,
                previous,
                instance,
                bounds,
                Placement {
                    batch: batch_of[index],
                    base: bases[batch_of[index] as usize],
                    skinning,
                    previous_palette,
                    hzb: index < depth_count as usize,
                },
            ));
        }
        self.previous = gathered
            .iter()
            .map(|(_, _, entity, transform, ..)| (*entity, transform.matrix()))
            .collect();
        let spheres = records
            .iter()
            .map(|record| {
                (
                    Vec3::new(record.bounds.x, record.bounds.y, record.bounds.z),
                    record.bounds.w,
                )
            })
            .collect::<Vec<_>>();
        Collection {
            batches,
            bases,
            slots,
            depth_batches,
            depth_count,
            total,
            joint_bytes,
            spheres,
            highlight,
            entities: gathered.iter().map(|(_, _, entity, ..)| *entity).collect(),
            records,
        }
    }
}
