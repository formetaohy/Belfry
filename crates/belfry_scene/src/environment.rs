use crate::reference::EnvironmentId;
use belfry_render::environment::Atmosphere;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum EnvironmentSource {
    Panorama(PathBuf),
    Atmosphere(Atmosphere),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentEntry {
    pub id: String,
    pub source: EnvironmentSource,
}

impl EnvironmentEntry {
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            source: EnvironmentSource::Panorama(path.into()),
        }
    }

    pub fn with_atmosphere(id: impl Into<String>, atmosphere: Atmosphere) -> Self {
        Self {
            id: id.into(),
            source: EnvironmentSource::Atmosphere(atmosphere),
        }
    }

    pub fn panorama(&self) -> Option<&Path> {
        match &self.source {
            EnvironmentSource::Panorama(path) => Some(path),
            EnvironmentSource::Atmosphere(_) => None,
        }
    }

    pub fn atmosphere(&self) -> Option<&Atmosphere> {
        match &self.source {
            EnvironmentSource::Panorama(_) => None,
            EnvironmentSource::Atmosphere(atmosphere) => Some(atmosphere),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentRef {
    pub environment: EnvironmentId,
    pub intensity: f32,
    pub sky: bool,
    pub local: bool,
    pub blend: f32,
}

impl EnvironmentRef {
    pub fn new(environment: EnvironmentId) -> Self {
        Self {
            environment,
            intensity: 1.0,
            sky: true,
            local: false,
            blend: 1.0,
        }
    }

    pub fn probe(mut self, blend: f32) -> Self {
        self.local = true;
        self.blend = blend;
        self
    }
}
