//! Source/toolchain identities invalidate bounded video working sets after in-place rewrites.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

impl FileIdentity {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.canonicalize().ok()?,
            bytes: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodeKey {
    source: FileIdentity,
    ffmpeg: FileIdentity,
    rate: Option<u32>,
}

impl DecodeKey {
    pub(crate) fn read(source: &Path, ffmpeg: &Path, rate: Option<u32>) -> Option<Self> {
        Some(Self {
            source: FileIdentity::read(source)?,
            ffmpeg: FileIdentity::read(ffmpeg)?,
            rate,
        })
    }
}

pub(crate) struct WorkingSet<K, V> {
    entries: VecDeque<(K, V)>,
    limit: usize,
}

impl<K: PartialEq, V: Clone> WorkingSet<K, V> {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit,
        }
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<V> {
        let index = self.entries.iter().position(|(old, _)| old == key)?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if let Some(index) = self.entries.iter().position(|(old, _)| *old == key) {
            self.entries.remove(index);
        }
        self.entries.push_back((key, value));
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkingSet;

    #[test]
    fn working_set_refreshes_recency_and_replaces_without_growth() {
        let mut cache = WorkingSet::new(2);
        cache.insert(1, "one");
        cache.insert(2, "two");
        assert_eq!(cache.get(&1), Some("one"));
        cache.insert(3, "three");
        assert_eq!(cache.get(&2), None);
        cache.insert(1, "replacement");
        assert_eq!(cache.get(&1), Some("replacement"));
        assert_eq!(cache.entries.len(), 2);
    }
}
