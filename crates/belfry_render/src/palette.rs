use belfry_ecs::Entity;
use belfry_gpu_device::{BufferBinding, BufferUsages, Device, GpuBuffer, Queue, Submission};
use belfry_math::Mat4;
use std::collections::HashMap;

const JOINT_BYTES: u64 = std::mem::size_of::<Mat4>() as u64;
const POOL_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Skinning {
    pub(crate) palette: u32,
    pub(crate) joints: u32,
}

pub(crate) const NONE: Skinning = Skinning {
    palette: 0,
    joints: 0,
};

#[derive(Clone, Copy)]
struct Range {
    palette: u32,
    joints: u32,
}
struct Staged {
    range: Range,
    first: usize,
    joints: usize,
    fresh: bool,
}

pub(crate) struct Palettes {
    placed: HashMap<Entity, Range>,
    free: Vec<Range>,
    matrices: Vec<Mat4>,
    staged: Vec<Staged>,
    joints: Option<[GpuBuffer; 2]>,
    bytes: u64,
    used: u32,
    front: usize,
    posed: usize,
}

impl Palettes {
    pub(crate) fn new() -> Self {
        Self {
            placed: HashMap::new(),
            free: Vec::new(),
            matrices: Vec::new(),
            staged: Vec::new(),
            joints: None,
            bytes: POOL_BYTES,
            used: 0,
            front: 0,
            posed: 0,
        }
    }

    pub(crate) fn begin(&mut self) {
        self.matrices.clear();
        self.staged.clear();
    }

    pub(crate) fn push(&mut self, entity: Entity, joints: &[Mat4]) -> Skinning {
        assert!(
            !joints.is_empty(),
            "a skinned instance poses at least one joint",
        );
        let wanted = joints.len() as u32;
        let fresh = match self.placed.get(&entity) {
            Some(range) if range.joints == wanted => false,
            Some(range) => {
                self.release(range.palette, range.joints);
                self.claim(entity, wanted);
                true
            }
            None => {
                self.claim(entity, wanted);
                true
            }
        };
        let range = self
            .placed
            .get(&entity)
            .copied()
            .expect("a posed entity holds the range it claimed");
        let first = self.matrices.len();
        self.matrices.extend_from_slice(joints);
        self.staged.push(Staged {
            range,
            first,
            joints: joints.len(),
            fresh,
        });
        Skinning {
            palette: range.palette,
            joints: range.joints,
        }
    }

    pub(crate) fn commit(&mut self, device: &Device, queue: &Queue) {
        let bytes = u64::from(self.used) * JOINT_BYTES;
        self.fit(device, queue, bytes);
        self.posed = self.front;
        let joints = self
            .joints
            .as_ref()
            .expect("a committed frame holds the buffers its poses write");
        let front = &joints[self.front];
        let back = &joints[1 - self.front];
        let mut fresh = Vec::new();
        for staged in &self.staged {
            let matrices = &self.matrices[staged.first..staged.first + staged.joints];
            let records = matrices.iter().map(joint_record).collect::<Vec<_>>();
            let offset = u64::from(staged.range.palette) * JOINT_BYTES;
            front.write_at(queue, offset, bytemuck::cast_slice(&records));
            if staged.fresh {
                fresh.push(staged);
            }
        }
        for staged in fresh {
            let matrices = &self.matrices[staged.first..staged.first + staged.joints];
            let records = matrices.iter().map(joint_record).collect::<Vec<_>>();
            let offset = u64::from(staged.range.palette) * JOINT_BYTES;
            back.write_at(queue, offset, bytemuck::cast_slice(&records));
        }
    }

    pub(crate) fn finish(&mut self) {
        self.front = 1 - self.front;
    }

    pub(crate) fn binding(&self) -> BufferBinding<'_> {
        self.joints
            .as_ref()
            .expect("a committed frame holds the palette it poses")[self.front]
            .binding(0, self.bytes)
    }

    pub(crate) fn previous(&self) -> BufferBinding<'_> {
        self.joints
            .as_ref()
            .expect("a committed frame holds the palette it poses")[1 - self.front]
            .binding(0, self.bytes)
    }

    pub(crate) fn posed(&self) -> BufferBinding<'_> {
        self.joints
            .as_ref()
            .expect("a committed frame holds the palette it poses")[self.posed]
            .binding(0, self.bytes)
    }

    fn claim(&mut self, entity: Entity, joints: u32) {
        let range = match self.take(joints) {
            Some(range) => range,
            None => {
                let range = Range {
                    palette: self.used,
                    joints,
                };
                self.used += joints;
                range
            }
        };
        self.placed.insert(entity, range);
    }

    fn release(&mut self, palette: u32, joints: u32) {
        self.free.push(Range { palette, joints });
    }

    fn take(&mut self, joints: u32) -> Option<Range> {
        let at = self.free.iter().position(|held| held.joints >= joints)?;
        let held = self.free.swap_remove(at);
        if held.joints == joints {
            return Some(held);
        }
        self.free.push(Range {
            palette: held.palette + joints,
            joints: held.joints - joints,
        });
        Some(Range {
            palette: held.palette,
            joints,
        })
    }

    fn fit(&mut self, device: &Device, queue: &Queue, bytes: u64) {
        if self.joints.is_none() {
            let grown = bytes.next_multiple_of(POOL_BYTES).max(POOL_BYTES);
            self.joints = Some([
                GpuBuffer::new(device, "belfry joints", grown, storage()),
                GpuBuffer::new(device, "belfry previous joints", grown, storage()),
            ]);
            self.bytes = grown;
            return;
        }
        if bytes <= self.bytes {
            return;
        }
        let grown = bytes
            .next_multiple_of(POOL_BYTES)
            .max(self.bytes.saturating_mul(2));
        let joints = self
            .joints
            .as_mut()
            .expect("a grown palette pool replaces the buffers it holds");
        for held in joints.iter_mut() {
            let grown_buffer = GpuBuffer::new(device, "belfry joints", grown, storage());
            let mut submission = Submission::new(device, "belfry palette growth");
            submission.copy(held, 0, &grown_buffer, 0, self.bytes);
            submission.submit(queue);
            *held = grown_buffer;
        }
        self.bytes = grown;
    }
}

fn storage() -> BufferUsages {
    BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC
}

fn joint_record(matrix: &Mat4) -> crate::abi::JointRecord {
    crate::abi::JointRecord {
        col0: crate::frame::vector(matrix.col(0)),
        col1: crate::frame::vector(matrix.col(1)),
        col2: crate::frame::vector(matrix.col(2)),
        col3: crate::frame::vector(matrix.col(3)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Palettes, Skinning};
    use belfry_ecs::World;
    use belfry_math::Mat4;

    #[test]
    fn a_posed_entity_keeps_the_palette_range_it_claimed_first() {
        let mut world = World::new();
        let posed = world.spawn();
        let later = world.spawn();
        let mut palettes = Palettes::new();
        palettes.begin();
        let first = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            first,
            Skinning {
                palette: 0,
                joints: 2
            }
        );
        palettes.begin();
        let second = palettes.push(later, &[Mat4::IDENTITY]);
        assert_eq!(
            second,
            Skinning {
                palette: 2,
                joints: 1
            }
        );
        palettes.begin();
        let again = palettes.push(posed, &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            again, first,
            "a posed entity keeps the range it claimed while another rig claims beside it",
        );
        palettes.begin();
        let grown = palettes.push(posed, &[Mat4::IDENTITY; 3]);
        assert_eq!(
            grown,
            Skinning {
                palette: 3,
                joints: 3
            },
            "a wider rig claims a range that holds it",
        );
        palettes.begin();
        let reused = palettes.push(world.spawn(), &[Mat4::IDENTITY, Mat4::IDENTITY]);
        assert_eq!(
            reused, first,
            "the range a wider rig released returns to the pool",
        );
    }
}
