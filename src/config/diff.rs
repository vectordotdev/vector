use std::collections::HashSet;

use indexmap::IndexMap;

use super::{ComponentKey, Config};

#[derive(Debug)]
pub struct ConfigDiff {
    pub sources: Difference,
    pub transforms: Difference,
    pub sinks: Difference,
    pub enrichment_tables: Difference,
}

impl ConfigDiff {
    pub fn initial(initial: &Config) -> Self {
        Self::new(&Config::default(), initial)
    }

    pub fn new(old: &Config, new: &Config) -> Self {
        ConfigDiff {
            sources: Difference::new(&old.sources, &new.sources),
            transforms: Difference::new(&old.transforms, &new.transforms),
            sinks: Difference::new(&old.sinks, &new.sinks),
            enrichment_tables: Difference::new(&old.enrichment_tables, &new.enrichment_tables),
        }
    }

    /// Swaps removed with added in Differences.
    pub fn flip(mut self) -> Self {
        self.sources.flip();
        self.transforms.flip();
        self.sinks.flip();
        self
    }

    /// Checks whether or not the given component is present at all.
    pub fn contains(&self, key: &ComponentKey) -> bool {
        self.sources.contains(key) || self.transforms.contains(key) || self.sinks.contains(key)
    }

    /// Checks whether or not the given component is changed.
    pub fn is_changed(&self, key: &ComponentKey) -> bool {
        self.sources.is_changed(key)
            || self.transforms.is_changed(key)
            || self.sinks.is_changed(key)
    }

    /// Checks whether or not the given component is removed.
    pub fn is_removed(&self, key: &ComponentKey) -> bool {
        self.sources.is_removed(key)
            || self.transforms.is_removed(key)
            || self.sinks.is_removed(key)
    }
}

#[derive(Debug)]
pub struct Difference {
    pub to_remove: HashSet<ComponentKey>,
    pub to_change: HashSet<ComponentKey>,
    pub to_add: HashSet<ComponentKey>,
}

impl Difference {
    fn new<C>(old: &IndexMap<ComponentKey, C>, new: &IndexMap<ComponentKey, C>) -> Self
    where
        C: serde::Serialize + serde::Deserialize<'static>,
    {
        let old_names = old.keys().cloned().collect::<HashSet<_>>();
        let new_names = new.keys().cloned().collect::<HashSet<_>>();

        let to_change = old_names
            .intersection(&new_names)
            .filter(|&n| {
                // This is a hack around the issue of comparing two
                // trait objects. Json is used here over toml since
                // toml does not support serializing `None`
                // to_value is used specifically (instead of string)
                // to avoid problems comparing serialized HashMaps,
                // which can iterate in varied orders.
                let old_value = serde_json::to_value(&old[n]).unwrap();
                let new_value = serde_json::to_value(&new[n]).unwrap();
                old_value != new_value
            })
            .cloned()
            .collect::<HashSet<_>>();

        let to_remove = &old_names - &new_names;
        let to_add = &new_names - &old_names;

        Self {
            to_remove,
            to_change,
            to_add,
        }
    }

    /// Checks whether or not any components are being changed or added.
    pub fn any_changed_or_added(&self) -> bool {
        !(self.to_change.is_empty() && self.to_add.is_empty())
    }

    /// Checks whether or not any components are being changed or removed.
    pub fn any_changed_or_removed(&self) -> bool {
        !(self.to_change.is_empty() && self.to_remove.is_empty())
    }

    /// Checks whether the given component is present at all.
    pub fn contains(&self, id: &ComponentKey) -> bool {
        self.to_add.contains(id) || self.to_change.contains(id) || self.to_remove.contains(id)
    }

    /// Checks whether the given component is present as a change or addition.
    pub fn contains_new(&self, id: &ComponentKey) -> bool {
        self.to_add.contains(id) || self.to_change.contains(id)
    }

    /// Checks whether or not the given component is changed.
    pub fn is_changed(&self, key: &ComponentKey) -> bool {
        self.to_change.contains(key)
    }

    /// Checks whether the given component is present as an addition.
    pub fn is_added(&self, id: &ComponentKey) -> bool {
        self.to_add.contains(id)
    }

    /// Checks whether or not the given component is removed.
    pub fn is_removed(&self, key: &ComponentKey) -> bool {
        self.to_remove.contains(key)
    }

    fn flip(&mut self) {
        std::mem::swap(&mut self.to_remove, &mut self.to_add);
    }

    pub fn changed_and_added(&self) -> impl Iterator<Item = &ComponentKey> {
        self.to_change.iter().chain(self.to_add.iter())
    }

    pub fn removed_and_changed(&self) -> impl Iterator<Item = &ComponentKey> {
        self.to_change.iter().chain(self.to_remove.iter())
    }
}
