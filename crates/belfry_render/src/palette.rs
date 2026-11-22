use belfry_ecs::Entity;
use belfry_math::Mat4;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Skinning {
    pub(crate) palette: u32,
    pub(crate) joints: u32,
}

pub(crate) const NONE: Skinning = Skinning {
    palette: 0,
    joints: 0,
};

pub(crate) struct Palettes {
    current: Vec<Mat4>,
    last: HashMap<Entity, Skinning>,
    staged: HashMap<Entity, Skinning>,
    fresh: Vec<Skinning>,
}

impl Palettes {
    pub(crate) fn new() -> Self {
        Self {
            current: Vec::new(),
            last: HashMap::new(),
            staged: HashMap::new(),
            fresh: Vec::new(),
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    pub(crate) fn begin(&mut self) {
        self.current.clear();
        self.staged.clear();
        self.fresh.clear();
    }

    pub(crate) fn push(&mut self, entity: Entity, joints: &[Mat4]) -> (Skinning, u32) {
        assert!(
            !joints.is_empty(),
            "a skinned instance poses at least one joint",
        );
        assert!(
            !self.staged.contains_key(&entity),
            "an entity poses one palette in a frame",
        );
        let skinning = Skinning {
            palette: self.current.len() as u32,
            joints: joints.len() as u32,
        };
        self.current.extend_from_slice(joints);
        let previous = match self.last.get(&entity) {
            Some(held) if held.joints == skinning.joints => held.palette,
            _ => {
                self.fresh.push(skinning);
                skinning.palette
            }
        };
        self.staged.insert(entity, skinning);
        (skinning, previous)
    }

    pub(crate) fn matrices(&self) -> &[Mat4] {
        &self.current
    }

    pub(crate) fn fresh(&self) -> &[Skinning] {
        &self.fresh
    }

    pub(crate) fn commit(&mut self) {
        std::mem::swap(&mut self.last, &mut self.staged);
    }
}

#[cfg(test)]
mod tests {
    use super::{Palettes, Skinning};
    use belfry_ecs::World;
    use belfry_math::Mat4;

    #[test]
    fn a_posed_entity_keeps_the_palette_it_held_last_frame() {
        let mut world = World::new();
        let posed = world.spawn();
        let later = world.spawn();
        let mut palettes = Palettes::new();
        palettes.begin();
        let (first, previous) = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            first,
            Skinning {
                palette: 0,
                joints: 2
            }
        );
        assert_eq!(previous, 0, "a new entity carries its own palette back");
        assert_eq!(palettes.fresh().len(), 1);
        palettes.commit();
        palettes.begin();
        palettes.push(later, &[Mat4::IDENTITY]);
        let (shifted, previous) = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            shifted,
            Skinning {
                palette: 1,
                joints: 2
            }
        );
        assert_eq!(
            previous, 0,
            "a posed entity keeps the palette it held while another shifts it",
        );
        palettes.commit();
        palettes.begin();
        let (grown, previous) = palettes.push(posed, &[Mat4::IDENTITY; 3]);
        assert_eq!(
            grown,
            Skinning {
                palette: 0,
                joints: 3
            }
        );
        assert_eq!(
            previous, grown.palette,
            "a rig of another joint count carries no palette forward",
        );
        assert_eq!(palettes.fresh().len(), 1);
        assert_eq!(palettes.fresh()[0], grown);
    }
}
