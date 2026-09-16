//! Reader ownership is independent of the fingerprint used to discover a file.
//!
//! The ordered reader table owns the watchers. The fingerprint table is only a lookup index;
//! changing a fingerprint cannot move a reader in the scheduling order or replace its identity.

use std::{
    collections::{HashMap, HashSet},
    ops::Index,
};

use file_source_common::FileFingerprint;
use indexmap::{
    IndexMap,
    map::{Values, ValuesMut},
};

use crate::file_watcher::{FileIdentity, FileWatcher};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ReaderId(u64);

pub(crate) struct TrackedReader {
    fingerprint: FileFingerprint,
    watcher: FileWatcher,
}

#[derive(Default)]
pub(crate) struct ReaderRegistry {
    readers: IndexMap<ReaderId, TrackedReader>,
    by_fingerprint: HashMap<FileFingerprint, ReaderId>,
    retired: HashSet<ReaderId>,
    retired_by_identity: HashMap<FileIdentity, HashSet<ReaderId>>,
    next_id: u64,
}

fn pair(reader: &TrackedReader) -> (&FileFingerprint, &FileWatcher) {
    (&reader.fingerprint, &reader.watcher)
}

fn pair_mut(reader: &mut TrackedReader) -> (&FileFingerprint, &mut FileWatcher) {
    (&reader.fingerprint, &mut reader.watcher)
}

fn discoverable(reader: &&TrackedReader) -> bool {
    !reader.watcher.retired
}

fn discoverable_mut(reader: &&mut TrackedReader) -> bool {
    !reader.watcher.retired
}

pub(crate) type Iter<'a> = std::iter::Map<
    std::iter::Filter<Values<'a, ReaderId, TrackedReader>, fn(&&TrackedReader) -> bool>,
    fn(&'a TrackedReader) -> (&'a FileFingerprint, &'a FileWatcher),
>;
pub(crate) type IterMut<'a> = std::iter::Map<
    std::iter::Filter<ValuesMut<'a, ReaderId, TrackedReader>, fn(&&mut TrackedReader) -> bool>,
    fn(&'a mut TrackedReader) -> (&'a FileFingerprint, &'a mut FileWatcher),
>;

impl ReaderRegistry {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn len(&self) -> usize {
        self.readers.len()
    }

    pub(crate) fn reader_id(&self, fingerprint: &FileFingerprint) -> Option<ReaderId> {
        self.by_fingerprint.get(fingerprint).copied()
    }

    pub(crate) fn contains_key(&self, fingerprint: &FileFingerprint) -> bool {
        self.by_fingerprint.contains_key(fingerprint)
    }

    pub(crate) fn get(&self, fingerprint: &FileFingerprint) -> Option<&FileWatcher> {
        self.readers
            .get(self.by_fingerprint.get(fingerprint)?)
            .map(|reader| &reader.watcher)
    }

    pub(crate) fn get_mut(&mut self, fingerprint: &FileFingerprint) -> Option<&mut FileWatcher> {
        let id = self.reader_id(fingerprint)?;
        self.get_by_id_mut(id)
    }

    pub(crate) fn get_by_id_mut(&mut self, id: ReaderId) -> Option<&mut FileWatcher> {
        self.readers.get_mut(&id).map(|reader| &mut reader.watcher)
    }

    pub(crate) fn insert(&mut self, fingerprint: FileFingerprint, watcher: FileWatcher) {
        // Preserve the current discovery policy for duplicate fingerprints. Rotation will use
        // explicit reader lifecycle operations rather than treating this index as ownership.
        let position = self
            .reader_id(&fingerprint)
            .map(|old_id| {
                let position = self
                    .readers
                    .get_index_of(&old_id)
                    .expect("indexed reader exists");
                self.readers.shift_remove(&old_id);
                position
            })
            .unwrap_or(self.readers.len());
        let id = ReaderId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("reader identity exhausted");
        self.readers.shift_insert(
            position,
            id,
            TrackedReader {
                fingerprint,
                watcher,
            },
        );
        self.by_fingerprint.insert(fingerprint, id);
    }

    pub(crate) fn rekey(&mut self, old: FileFingerprint, new: FileFingerprint) -> bool {
        if old == new {
            return self.contains_key(&old);
        }
        if self.contains_key(&new) {
            return false;
        }
        let Some(id) = self.by_fingerprint.remove(&old) else {
            return false;
        };
        self.readers
            .get_mut(&id)
            .expect("indexed reader exists")
            .fingerprint = new;
        self.by_fingerprint.insert(new, id);
        true
    }

    pub(crate) fn iter(&self) -> Iter<'_> {
        self.readers
            .values()
            .filter(discoverable as fn(&&TrackedReader) -> bool)
            .map(pair)
    }
    /// All opened readers, including displaced inodes, in their original scheduling order.
    pub(crate) fn reading_mut(
        &mut self,
    ) -> impl Iterator<Item = (&FileFingerprint, &mut FileWatcher)> {
        self.readers.values_mut().map(pair_mut)
    }

    #[cfg(test)]
    pub(crate) fn retired_ids(&self) -> Vec<ReaderId> {
        self.retired.iter().copied().collect()
    }

    pub(crate) fn has_retired_identities(&self) -> bool {
        !self.retired_by_identity.is_empty()
    }

    pub(crate) fn retired_with_identity(&self, identity: FileIdentity) -> Option<ReaderId> {
        self.retired_by_identity
            .get(&identity)?
            .iter()
            .next()
            .copied()
    }

    /// With oldest-first scheduling, a retired reader cannot bypass an earlier active reader,
    /// even during shutdown. Readers newer than the last pending retired inode are left alone.
    pub(crate) fn shutdown_readers_mut(
        &mut self,
        oldest_first: bool,
    ) -> impl Iterator<Item = (&FileFingerprint, &mut FileWatcher)> {
        let end = self
            .readers
            .values()
            .rposition(|reader| reader.watcher.retired && !reader.watcher.dead())
            .map_or(0, |index| index + 1);
        self.readers
            .values_mut()
            .take(end)
            .filter(move |reader| oldest_first || reader.watcher.retired)
            .map(pair_mut)
    }

    pub(crate) fn all_values(&self) -> impl Iterator<Item = &FileWatcher> {
        self.readers.values().map(|reader| &reader.watcher)
    }

    /// Release only the discovery key; ownership of the opened reader stays in this registry.
    pub(crate) fn retire(&mut self, fingerprint: FileFingerprint) -> bool {
        let Some(id) = self.by_fingerprint.remove(&fingerprint) else {
            return false;
        };
        let watcher = &mut self
            .readers
            .get_mut(&id)
            .expect("indexed reader exists")
            .watcher;
        watcher.retired = true;
        watcher.mark_found();
        watcher.mark_ready_to_read();
        self.retired.insert(id);
        if let Some(identity) = watcher.identity() {
            self.retired_by_identity
                .entry(identity)
                .or_default()
                .insert(id);
        }
        true
    }
    #[cfg(test)]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &FileFingerprint> {
        self.iter().map(|(fingerprint, _)| fingerprint)
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &FileWatcher> {
        self.iter().map(|(_, watcher)| watcher)
    }
    #[cfg(test)]
    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut FileWatcher> {
        self.readers
            .values_mut()
            .filter(discoverable_mut)
            .map(|reader| &mut reader.watcher)
    }

    pub(crate) fn retain(
        &mut self,
        mut keep: impl FnMut(&FileFingerprint, &mut FileWatcher) -> bool,
    ) {
        let by_fingerprint = &mut self.by_fingerprint;
        let retired = &mut self.retired;
        let retired_by_identity = &mut self.retired_by_identity;
        self.readers.retain(|id, reader| {
            if keep(&reader.fingerprint, &mut reader.watcher) {
                true
            } else {
                retired.remove(id);
                if let Some(identity) = reader.watcher.identity()
                    && let Some(ids) = retired_by_identity.get_mut(&identity)
                {
                    ids.remove(id);
                    if ids.is_empty() {
                        retired_by_identity.remove(&identity);
                    }
                }
                if by_fingerprint.get(&reader.fingerprint) == Some(id) {
                    by_fingerprint.remove(&reader.fingerprint);
                }
                false
            }
        });
    }
}

impl<'a> IntoIterator for &'a ReaderRegistry {
    type Item = (&'a FileFingerprint, &'a FileWatcher);
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut ReaderRegistry {
    type Item = (&'a FileFingerprint, &'a mut FileWatcher);
    type IntoIter = IterMut<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.readers
            .values_mut()
            .filter(discoverable_mut as fn(&&mut TrackedReader) -> bool)
            .map(pair_mut)
    }
}

impl Index<&FileFingerprint> for ReaderRegistry {
    type Output = FileWatcher;
    fn index(&self, fingerprint: &FileFingerprint) -> &Self::Output {
        self.get(fingerprint).expect("fingerprint is tracked")
    }
}

#[cfg(test)]
impl<const N: usize> From<[(FileFingerprint, FileWatcher); N]> for ReaderRegistry {
    fn from(readers: [(FileFingerprint, FileWatcher); N]) -> Self {
        let mut registry = Self::default();
        for (fingerprint, watcher) in readers {
            registry.insert(fingerprint, watcher);
        }
        registry
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use file_source_common::ReadFrom;

    use super::*;

    #[tokio::test]
    async fn retired_identity_index_survives_aliases_and_partial_removal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("archive.log");
        std::fs::write(&path, b"header\n").unwrap();
        let mut registry = ReaderRegistry::new();
        let mut ids = Vec::new();
        let key = FileFingerprint::FirstLinesChecksum(1);
        for _ in 0..2 {
            let watcher = FileWatcher::new(
                path.clone(),
                ReadFrom::Beginning,
                None,
                1024,
                Bytes::from_static(b"\n"),
                false,
            )
            .await
            .unwrap();
            registry.insert(key, watcher);
            ids.push(registry.reader_id(&key).unwrap());
            assert!(registry.retire(key));
        }
        std::fs::rename(&path, &archive).unwrap();
        let identity = crate::file_watcher::path_identity(&archive).await.unwrap();
        assert!(ids.contains(&registry.retired_with_identity(identity).unwrap()));
        // Identical content at the original path is not the archived inode.
        std::fs::write(&path, b"header\n").unwrap();
        let replacement = crate::file_watcher::path_identity(&path).await.unwrap();
        assert_eq!(registry.retired_with_identity(replacement), None);
        registry.get_by_id_mut(ids[0]).unwrap().set_dead();
        registry.retain(|_, watcher| !watcher.dead());
        assert_eq!(registry.retired_with_identity(identity), Some(ids[1]));
        registry.retain(|_, _| false);
        assert_eq!(registry.retired_with_identity(identity), None);
        assert!(!registry.has_retired_identities());
    }

    #[tokio::test]
    async fn replacing_a_reader_does_not_reuse_its_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"line\n").unwrap();
        let make = || {
            FileWatcher::new(
                path.clone(),
                ReadFrom::Beginning,
                None,
                1024,
                Bytes::from_static(b"\n"),
                false,
            )
        };
        let first = FileFingerprint::FirstLinesChecksum(1);
        let second = FileFingerprint::FirstLinesChecksum(2);
        let mut registry = ReaderRegistry::new();
        registry.insert(first, make().await.unwrap());
        registry.insert(second, make().await.unwrap());
        let retired_id = registry.reader_id(&first).unwrap();
        let second_id = registry.reader_id(&second).unwrap();
        registry.insert(first, make().await.unwrap());
        assert_ne!(registry.reader_id(&first), Some(retired_id));
        assert!(registry.get_by_id_mut(retired_id).is_none());
        assert_eq!(registry.reader_id(&second), Some(second_id));
        assert_eq!(
            registry.keys().copied().collect::<Vec<_>>(),
            vec![first, second]
        );
        assert!(!registry.rekey(first, second));
        assert_eq!(registry.reader_id(&second), Some(second_id));
        registry.retain(|fingerprint, _| *fingerprint == second);
        assert_eq!(registry.reader_id(&first), None);
        assert_eq!(registry.len(), 1);

        // Rotation releases the lookup key without releasing the opened reader. Removing the
        // retired generation afterwards must not remove its replacement's identical key.
        assert!(registry.retire(second));
        registry.insert(second, make().await.unwrap());
        let replacement_id = registry.reader_id(&second).unwrap();
        assert_ne!(replacement_id, second_id);
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.iter().count(), 1);
        assert_eq!(registry.reading_mut().count(), 2);
        registry.get_by_id_mut(second_id).unwrap().set_dead();
        registry.retain(|_, watcher| !watcher.dead());
        assert_eq!(registry.reader_id(&second), Some(replacement_id));
        assert!(registry.retired_ids().is_empty());
    }
}
