use crate::buffer::BufferRange;
use crate::pipeline::BindGroup;
use crate::table::{SamplerTable, TextureTable};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

const BINDINGS: usize = 16;
const CAPACITY: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ResourceKey {
    Buffer(BufferRange),
    Texture(usize),
    Sampler(usize),
    TextureTable(usize, u64),
    SamplerTable(usize, u64),
}

impl ResourceKey {
    pub(crate) fn buffer(range: BufferRange) -> Self {
        Self::Buffer(range)
    }

    pub(crate) fn texture(identity: usize) -> Self {
        Self::Texture(identity)
    }

    pub(crate) fn sampler(identity: usize) -> Self {
        Self::Sampler(identity)
    }

    pub(crate) fn textures(table: &TextureTable) -> Self {
        Self::TextureTable(table.identity(), table.generation())
    }

    pub(crate) fn samplers(table: &SamplerTable) -> Self {
        Self::SamplerTable(table.identity(), table.generation())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GroupKey {
    owner: usize,
    group: u32,
    len: u8,
    bindings: [Option<ResourceKey>; BINDINGS],
}

impl GroupKey {
    pub(crate) fn new(
        owner: usize,
        group: u32,
        bindings: impl Iterator<Item = ResourceKey>,
    ) -> Self {
        let mut held = [None; BINDINGS];
        let mut len = 0usize;
        for key in bindings {
            held[len] = Some(key);
            len += 1;
            assert!(
                len <= BINDINGS,
                "a bind group outruns the {BINDINGS} resources a cached group holds",
            );
        }
        Self {
            owner,
            group,
            len: len as u8,
            bindings: held,
        }
    }

    fn entries(&self) -> &[Option<ResourceKey>] {
        &self.bindings[..self.len as usize]
    }
}

impl PartialEq for GroupKey {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner && self.group == other.group && self.entries() == other.entries()
    }
}

impl Eq for GroupKey {}

impl Hash for GroupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
        self.group.hash(state);
        self.entries().hash(state);
    }
}

struct Entry {
    tick: u64,
    group: BindGroup,
}

#[derive(Default)]
struct Held {
    tick: u64,
    groups: HashMap<GroupKey, Entry>,
}

#[derive(Default)]
pub(crate) struct GroupCache {
    held: Mutex<Held>,
}

impl GroupCache {
    pub(crate) fn fetch(&self, key: GroupKey, build: impl FnOnce() -> BindGroup) -> BindGroup {
        let tick = {
            let mut held = self
                .held
                .lock()
                .expect("a bind group cache is never poisoned");
            held.tick += 1;
            let tick = held.tick;
            match held.groups.get_mut(&key) {
                Some(entry) => {
                    entry.tick = tick;
                    return entry.group.clone();
                }
                None => tick,
            }
        };
        let group = build();
        let mut held = self
            .held
            .lock()
            .expect("a bind group cache is never poisoned");
        if held.groups.len() >= CAPACITY {
            Self::evict(&mut held);
        }
        held.groups.insert(
            key,
            Entry {
                tick,
                group: group.clone(),
            },
        );
        group
    }

    fn evict(held: &mut Held) {
        let mut stamps = held
            .groups
            .values()
            .map(|entry| entry.tick)
            .collect::<Vec<_>>();
        stamps.sort_unstable();
        let cut = stamps[held.groups.len() / 2];
        held.groups.retain(|_, entry| entry.tick >= cut);
    }
}
