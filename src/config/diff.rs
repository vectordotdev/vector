use std::collections::HashSet;

use indexmap::IndexMap;
use vector_lib::config::OutputId;

use super::{ComponentKey, Config, EnrichmentTableOuter};

#[derive(Debug)]
pub struct ConfigDiff {
    pub sources: Difference,
    pub transforms: Difference,
    pub sinks: Difference,
    pub enrichment_tables: EnrichmentTableDiff,
    pub components_to_reload: HashSet<ComponentKey>,
}

impl ConfigDiff {
    pub fn initial(initial: &Config) -> Self {
        Self::new(&Config::default(), initial, HashSet::new())
    }

    pub fn new(old: &Config, new: &Config, components_to_reload: HashSet<ComponentKey>) -> Self {
        ConfigDiff {
            sources: Difference::new(&old.sources, &new.sources, &components_to_reload),
            transforms: Difference::new(&old.transforms, &new.transforms, &components_to_reload),
            sinks: Difference::new(&old.sinks, &new.sinks, &components_to_reload),
            enrichment_tables: EnrichmentTableDiff::new(
                &old.enrichment_tables,
                &new.enrichment_tables,
                &components_to_reload,
            ),
            components_to_reload,
        }
    }

    /// Swaps removed with added in Differences.
    pub const fn flip(mut self) -> Self {
        self.sources.flip();
        self.transforms.flip();
        self.sinks.flip();
        self.enrichment_tables.flip();
        self
    }

    /// Checks whether the given component is present at all.
    pub fn contains(&self, key: &ComponentKey) -> bool {
        self.sources.contains(key)
            || self.transforms.contains(key)
            || self.sinks.contains(key)
            || self.enrichment_tables.contains(key)
    }

    /// Checks whether the given component is changed.
    pub fn is_changed(&self, key: &ComponentKey) -> bool {
        self.sources.is_changed(key)
            || self.transforms.is_changed(key)
            || self.sinks.is_changed(key)
            || self.enrichment_tables.is_changed(key)
    }

    /// Checks whether the given component is removed.
    pub fn is_removed(&self, key: &ComponentKey) -> bool {
        self.sources.is_removed(key)
            || self.transforms.is_removed(key)
            || self.sinks.is_removed(key)
            || self.enrichment_tables.is_removed(key)
    }
}

#[derive(Debug)]
pub struct EnrichmentTableDiff {
    /// Difference for the enrichment table configuration keyed by table name.
    pub tables: Difference,
    /// Difference for source components derived from enrichment tables.
    pub sources: Difference,
    /// Difference for sink components derived from enrichment tables.
    pub sinks: Difference,
}

impl EnrichmentTableDiff {
    fn new(
        old: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
        new: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
        need_change: &HashSet<ComponentKey>,
    ) -> Self {
        let tables = Difference::new(old, new, need_change);
        let sources = Difference::from_enrichment_table_components(
            old,
            new,
            need_change,
            enrichment_table_source_key,
        );
        let sinks = Difference::from_enrichment_table_components(
            old,
            new,
            need_change,
            enrichment_table_sink_key,
        );

        Self {
            tables,
            sources,
            sinks,
        }
    }

    /// Checks whether or not any enrichment table-derived component is being changed or added.
    pub fn any_changed_or_added(&self) -> bool {
        self.sources.any_changed_or_added() || self.sinks.any_changed_or_added()
    }

    /// Checks whether or not any enrichment table-derived component is being changed or removed.
    pub fn any_changed_or_removed(&self) -> bool {
        self.sources.any_changed_or_removed() || self.sinks.any_changed_or_removed()
    }

    /// Checks whether the given enrichment table-derived component is present at all.
    pub fn contains(&self, id: &ComponentKey) -> bool {
        self.sources.contains(id) || self.sinks.contains(id)
    }

    /// Checks whether the given enrichment table-derived component is present as a change or addition.
    pub fn contains_new(&self, id: &ComponentKey) -> bool {
        self.sources.contains_new(id) || self.sinks.contains_new(id)
    }

    /// Checks whether or not the given enrichment table-derived component is changed.
    pub fn is_changed(&self, key: &ComponentKey) -> bool {
        self.sources.is_changed(key) || self.sinks.is_changed(key)
    }

    /// Checks whether the given enrichment table-derived component is present as an addition.
    pub fn is_added(&self, id: &ComponentKey) -> bool {
        self.sources.is_added(id) || self.sinks.is_added(id)
    }

    /// Checks whether or not the given enrichment table-derived component is removed.
    pub fn is_removed(&self, key: &ComponentKey) -> bool {
        self.sources.is_removed(key) || self.sinks.is_removed(key)
    }

    const fn flip(&mut self) {
        self.tables.flip();
        self.sources.flip();
        self.sinks.flip();
    }
}

#[derive(Debug)]
pub struct Difference {
    pub to_remove: HashSet<ComponentKey>,
    pub to_change: HashSet<ComponentKey>,
    pub to_add: HashSet<ComponentKey>,
}

impl Difference {
    fn new<C>(
        old: &IndexMap<ComponentKey, C>,
        new: &IndexMap<ComponentKey, C>,
        need_change: &HashSet<ComponentKey>,
    ) -> Self
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
                old_value != new_value || need_change.contains(n)
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

    fn from_enrichment_table_components<F>(
        old: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
        new: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
        need_change: &HashSet<ComponentKey>,
        component_key: F,
    ) -> Self
    where
        F: Fn(&ComponentKey, &EnrichmentTableOuter<OutputId>) -> Option<ComponentKey>,
    {
        let old_table_keys = extract_table_component_keys(old, &component_key);
        let new_table_keys = extract_table_component_keys(new, &component_key);

        let to_change = old_table_keys
            .intersection(&new_table_keys)
            .filter(|(table_key, _derived_component_key)| {
                // This is a hack around the issue of comparing two
                // trait objects. Json is used here over toml since
                // toml does not support serializing `None`
                // to_value is used specifically (instead of string)
                // to avoid problems comparing serialized HashMaps,
                // which can iterate in varied orders.
                let old_value = serde_json::to_value(&old[*table_key]).unwrap();
                let new_value = serde_json::to_value(&new[*table_key]).unwrap();
                old_value != new_value || need_change.contains(*table_key)
            })
            .cloned()
            .map(|(_table_key, derived_component_key)| derived_component_key)
            .collect::<HashSet<_>>();

        // Extract only the derived component keys for the final difference calculation
        let old_component_keys = old_table_keys
            .into_iter()
            .map(|(_table_key, component_key)| component_key)
            .collect::<HashSet<_>>();
        let new_component_keys = new_table_keys
            .into_iter()
            .map(|(_table_key, component_key)| component_key)
            .collect::<HashSet<_>>();

        let to_remove = &old_component_keys - &new_component_keys;
        let to_add = &new_component_keys - &old_component_keys;

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

    const fn flip(&mut self) {
        std::mem::swap(&mut self.to_remove, &mut self.to_add);
    }

    pub fn changed_and_added(&self) -> impl Iterator<Item = &ComponentKey> {
        self.to_change.iter().chain(self.to_add.iter())
    }

    pub fn removed_and_changed(&self) -> impl Iterator<Item = &ComponentKey> {
        self.to_change.iter().chain(self.to_remove.iter())
    }
}

/// Helper function to extract component keys from enrichment tables.
fn extract_table_component_keys<'a, F>(
    tables: &'a IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
    component_key: &F,
) -> HashSet<(&'a ComponentKey, ComponentKey)>
where
    F: Fn(&ComponentKey, &EnrichmentTableOuter<OutputId>) -> Option<ComponentKey>,
{
    tables
        .iter()
        .filter_map(|(table_key, table)| {
            component_key(table_key, table).map(|component_key| (table_key, component_key))
        })
        .collect()
}

fn enrichment_table_source_key(
    table_key: &ComponentKey,
    table: &EnrichmentTableOuter<OutputId>,
) -> Option<ComponentKey> {
    table
        .as_source(table_key)
        .map(|(component_key, _)| component_key)
}

fn enrichment_table_sink_key(
    table_key: &ComponentKey,
    table: &EnrichmentTableOuter<OutputId>,
) -> Option<ComponentKey> {
    table
        .as_sink(table_key)
        .map(|(component_key, _)| component_key)
}

#[cfg(all(test, feature = "enrichment-tables-memory"))]
mod tests {
    use crate::config::ConfigBuilder;
    use indoc::indoc;

    use super::*;

    #[test]
    fn diff_enrichment_tables_uses_correct_keys() {
        let old_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            enrichment_tables:
              memory_table:
                type: "memory"
                ttl: 10
                inputs: []
                source_config:
                  source_key: "memory_table_source"
                  export_expired_items: true
                  export_interval: 50

              memory_table_unchanged:
                type: "memory"
                ttl: 10
                inputs: []

              memory_table_old:
                type: "memory"
                ttl: 10
                inputs: []

            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let new_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            enrichment_tables:
              memory_table:
                type: "memory"
                ttl: 20
                inputs: []
                source_config:
                  source_key: "memory_table_source"
                  export_expired_items: true
                  export_interval: 50

              memory_table_unchanged:
                type: "memory"
                ttl: 10
                inputs: []

              memory_table_new:
                type: "memory"
                ttl: 1000
                inputs: []

            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = EnrichmentTableDiff::new(
            &old_config.enrichment_tables,
            &new_config.enrichment_tables,
            &Default::default(),
        );

        assert_eq!(
            diff.tables.to_add,
            HashSet::from_iter(["memory_table_new".into()])
        );
        assert_eq!(
            diff.tables.to_remove,
            HashSet::from_iter(["memory_table_old".into()])
        );
        assert_eq!(
            diff.tables.to_change,
            HashSet::from_iter(["memory_table".into()])
        );

        assert_eq!(
            diff.sources.to_change,
            HashSet::from_iter(["memory_table_source".into()])
        );
        assert!(diff.sources.to_add.is_empty());
        assert!(diff.sources.to_remove.is_empty());

        assert_eq!(
            diff.sinks.to_add,
            HashSet::from_iter(["memory_table_new".into()])
        );
        assert_eq!(
            diff.sinks.to_remove,
            HashSet::from_iter(["memory_table_old".into()])
        );
        assert_eq!(
            diff.sinks.to_change,
            HashSet::from_iter(["memory_table".into()])
        );
    }

    #[test]
    fn diff_enrichment_table_component_helpers_ignore_table_config_keys() {
        let old_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let new_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            enrichment_tables:
              file_table:
                type: "file"
                file:
                  path: ./tests/data/enrichment.csv
                  encoding:
                    type: "csv"
                schema:
                  id: integer

            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = EnrichmentTableDiff::new(
            &old_config.enrichment_tables,
            &new_config.enrichment_tables,
            &Default::default(),
        );
        let table_key = ComponentKey::from("file_table");

        assert_eq!(diff.tables.to_add, HashSet::from_iter([table_key.clone()]));
        assert!(diff.sources.to_add.is_empty());
        assert!(diff.sinks.to_add.is_empty());

        assert!(!diff.any_changed_or_added());
        assert!(!diff.contains(&table_key));
        assert!(!diff.contains_new(&table_key));
        assert!(!diff.is_added(&table_key));
    }

    #[test]
    fn diff_enrichment_tables_tracks_source_key_renames() {
        let old_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            enrichment_tables:
              memory_table:
                type: "memory"
                ttl: 10
                inputs: []
                source_config:
                  source_key: "memory_table_source_old"
                  export_interval: 50

            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let new_config: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            enrichment_tables:
              memory_table:
                type: "memory"
                ttl: 10
                inputs: []
                source_config:
                  source_key: "memory_table_source_new"
                  export_interval: 50

            sources:
              test:
                type: "test_basic"

            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = EnrichmentTableDiff::new(
            &old_config.enrichment_tables,
            &new_config.enrichment_tables,
            &Default::default(),
        );

        assert_eq!(
            diff.tables.to_change,
            HashSet::from_iter(["memory_table".into()])
        );
        assert!(diff.tables.to_add.is_empty());
        assert!(diff.tables.to_remove.is_empty());

        assert_eq!(
            diff.sources.to_add,
            HashSet::from_iter(["memory_table_source_new".into()])
        );
        assert_eq!(
            diff.sources.to_remove,
            HashSet::from_iter(["memory_table_source_old".into()])
        );
        assert!(diff.sources.to_change.is_empty());

        assert_eq!(
            diff.sinks.to_change,
            HashSet::from_iter(["memory_table".into()])
        );
        assert!(diff.sinks.to_add.is_empty());
        assert!(diff.sinks.to_remove.is_empty());
    }
}
