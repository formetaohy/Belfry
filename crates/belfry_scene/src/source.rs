use crate::digest::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
struct Held {
    stamp: Option<Stamp>,
    state: Result<Digest, String>,
}

#[derive(Clone)]
pub struct Sources {
    files: BTreeMap<PathBuf, Held>,
}

impl Sources {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    pub fn digest(&mut self, path: &Path) -> Result<Digest, String> {
        let stamp = stamp_of(path);
        if let Some(held) = self.files.get(path)
            && held.stamp == stamp
        {
            return held.state.clone();
        }
        let state = read_digest(path);
        self.files.insert(
            path.to_path_buf(),
            Held {
                stamp,
                state: state.clone(),
            },
        );
        state
    }

    pub fn read(&mut self, path: &Path) -> Result<(Digest, Vec<u8>), String> {
        let bytes = fs::read(path).map_err(|error| {
            format!(
                "the asset source {} reads no bytes: {error}",
                path.display()
            )
        })?;
        let digest = Digest::of(&bytes);
        self.files.insert(
            path.to_path_buf(),
            Held {
                stamp: stamp_of(path),
                state: Ok(digest),
            },
        );
        Ok((digest, bytes))
    }

    pub fn retain(&mut self, used: &BTreeSet<PathBuf>) {
        self.files.retain(|path, _| used.contains(path));
    }
}

impl Default for Sources {
    fn default() -> Self {
        Self::new()
    }
}

fn read_digest(path: &Path) -> Result<Digest, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "the asset source {} reads no bytes: {error}",
            path.display()
        )
    })?;
    Ok(Digest::of(&bytes))
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(Stamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::Sources;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn a_source_reports_the_bytes_it_rehashes() {
        let path =
            std::env::temp_dir().join(format!("belfry_scene_source_{}.bin", std::process::id()));
        fs::write(&path, b"first").expect("a test source writes its file");
        let mut sources = Sources::new();
        let first = sources.digest(&path).expect("a test source reads its file");
        assert_eq!(
            sources.read(&path).expect("a test source reads its file"),
            (first, b"first".to_vec())
        );
        fs::write(&path, b"second").expect("a test source rewrites its file");
        let second = sources.digest(&path).expect("a test source reads its file");
        assert_ne!(first, second);
        sources.retain(&std::collections::BTreeSet::from([PathBuf::from(&path)]));
        assert_eq!(
            sources
                .digest(&path)
                .expect("a retained source reads its file"),
            second
        );
        let _ = fs::remove_file(path);
    }
}
