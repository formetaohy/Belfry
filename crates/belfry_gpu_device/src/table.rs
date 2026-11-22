use crate::capability::TextureUsages;
use crate::context::Device;
use crate::native::{NativeSamplerTable, NativeTextureTable};
use crate::sampler::{Sampler, SamplerDesc};
use crate::texture::Texture;
use std::sync::Arc;

struct TextureTableState {
    native: NativeTextureTable,
}

pub struct TextureTable {
    device: Device,
    capacity: u32,
    entries: Vec<Texture>,
    filled: u32,
    state: Arc<TextureTableState>,
}

impl Clone for TextureTable {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            capacity: self.capacity,
            entries: self.entries.clone(),
            filled: self.filled,
            state: Arc::clone(&self.state),
        }
    }
}

impl TextureTable {
    pub fn new(device: &Device, capacity: u32, fallback: &Texture) -> Self {
        assert!(capacity > 0, "a texture table holds no image");
        assert!(
            fallback.device().same(device),
            "a texture table fills from another device"
        );
        assert!(
            fallback.usages().contains(TextureUsages::SAMPLED),
            "a texture table fills from an unsampled texture"
        );
        let native = device.native().create_texture_table(capacity, fallback);
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            filled: 0,
            state: Arc::new(TextureTableState { native }),
        }
    }

    pub fn reset(&mut self, fallback: &Texture) {
        for index in 0..self.filled {
            self.device
                .native()
                .fill_texture_table(&self.state.native, index, fallback);
            self.entries[index as usize] = fallback.clone();
        }
        self.filled = 0;
    }

    pub fn add(&mut self, texture: &Texture) -> u32 {
        assert!(
            self.filled < self.capacity,
            "a texture table of {} images is full",
            self.capacity
        );
        let index = self.filled;
        self.set(index, texture);
        self.filled = index + 1;
        index
    }

    pub fn set(&mut self, index: u32, texture: &Texture) {
        assert!(
            index < self.capacity,
            "a texture table of {} images holds no slot {index}",
            self.capacity
        );
        assert!(
            texture.device().same(&self.device),
            "a texture table fills from another device"
        );
        assert!(
            texture.usages().contains(TextureUsages::SAMPLED),
            "a texture table holds sampled textures"
        );
        self.entries[index as usize] = texture.clone();
        self.device
            .native()
            .fill_texture_table(&self.state.native, index, texture);
    }

    pub fn add_shared(&mut self, texture: &Texture) -> u32 {
        match self.position(texture) {
            Some(index) => index,
            None => self.add(texture),
        }
    }

    pub fn position(&self, texture: &Texture) -> Option<u32> {
        self.entries[..self.filled as usize]
            .iter()
            .position(|held| held.identity() == texture.identity())
            .map(|index| index as u32)
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn filled(&self) -> u32 {
        self.filled
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    pub(crate) fn entries(&self) -> &[Texture] {
        &self.entries
    }

    pub(crate) fn native(&self) -> &NativeTextureTable {
        &self.state.native
    }
}

struct SamplerTableState {
    native: NativeSamplerTable,
}

pub struct SamplerTable {
    device: Device,
    capacity: u32,
    entries: Vec<Sampler>,
    filled: u32,
    state: Arc<SamplerTableState>,
}

impl Clone for SamplerTable {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            capacity: self.capacity,
            entries: self.entries.clone(),
            filled: self.filled,
            state: Arc::clone(&self.state),
        }
    }
}

impl SamplerTable {
    pub fn new(device: &Device, capacity: u32, fallback: &Sampler) -> Self {
        assert!(capacity > 0, "a sampler table holds no sampler");
        assert!(
            fallback.device().same(device),
            "a sampler table fills from another device"
        );
        let native = device.native().create_sampler_table(capacity, fallback);
        Self {
            device: device.clone(),
            capacity,
            entries: vec![fallback.clone(); capacity as usize],
            filled: 0,
            state: Arc::new(SamplerTableState { native }),
        }
    }

    pub fn reset(&mut self, fallback: &Sampler) {
        for index in 0..self.filled {
            self.device
                .native()
                .fill_sampler_table(&self.state.native, index, fallback);
            self.entries[index as usize] = fallback.clone();
        }
        self.filled = 0;
    }

    pub fn add(&mut self, sampler: &Sampler) -> u32 {
        assert!(
            self.filled < self.capacity,
            "a sampler table of {} samplers is full",
            self.capacity
        );
        assert!(
            sampler.device().same(&self.device),
            "a sampler table fills from another device"
        );
        let index = self.filled;
        self.entries[index as usize] = sampler.clone();
        self.filled += 1;
        self.device
            .native()
            .fill_sampler_table(&self.state.native, index, sampler);
        index
    }

    pub fn add_shared(&mut self, sampler: &Sampler) -> u32 {
        assert!(
            sampler.device().same(&self.device),
            "a sampler table fills from another device"
        );
        match self.position(sampler.desc()) {
            Some(index) => index,
            None => self.add(sampler),
        }
    }

    pub fn position(&self, desc: SamplerDesc) -> Option<u32> {
        self.entries[..self.filled as usize]
            .iter()
            .position(|held| held.desc() == desc)
            .map(|index| index as u32)
    }

    pub fn set(&mut self, index: u32, sampler: &Sampler) {
        assert!(
            index < self.capacity,
            "a sampler table of {} samplers holds no slot {index}",
            self.capacity
        );
        assert!(
            sampler.device().same(&self.device),
            "a sampler table fills from another device"
        );
        self.entries[index as usize] = sampler.clone();
        self.device
            .native()
            .fill_sampler_table(&self.state.native, index, sampler);
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn filled(&self) -> u32 {
        self.filled
    }

    pub(crate) fn device(&self) -> &Device {
        &self.device
    }

    #[cfg(not(metal_backend))]
    pub(crate) fn entries(&self) -> &[Sampler] {
        &self.entries
    }

    pub(crate) fn native(&self) -> &NativeSamplerTable {
        &self.state.native
    }
}
