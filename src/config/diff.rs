use std::collections::HashSet;

use indexmap::IndexMap;
use vector_lib::config::OutputId;

use super::{ComponentKey, Config, EnrichmentTableOuter};

#[derive(Debug)]
pub struct ConfigDiff {
    pub sources: Difference,
    pub transforms: Difference,
    pub sinks: Difference,
    /// This difference does not only contain the actual enrichment_tables keys, but also keys that
    /// may be used for their source and sink components (if available).
    pub enrichment_tables: Difference,
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
            enrichment_tables: Difference::from_enrichment_tables(
                &old.enrichment_tables,
                &new.enrichment_tables,
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
            || self.enrichment_tables.contains(key)
    }

    /// Checks whether the given component is removed.
    pub fn is_removed(&self, key: &ComponentKey) -> bool {
        self.sources.is_removed(key)
            || self.transforms.is_removed(key)
            || self.sinks.is_removed(key)
            || self.enrichment_tables.contains(key)
    }

    /// True when no components need add/change/remove (forced reloads already in `to_change`).
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.transforms.is_empty()
            && self.sinks.is_empty()
            && self.enrichment_tables.is_empty()
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

    fn from_enrichment_tables(
        old: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
        new: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
    ) -> Self {
        // Include table keys; standalone tables (e.g. file) have no derived source/sink.
        let table_diff = Self::new(old, new, &HashSet::new());

        let old_table_keys = extract_table_component_keys(old);
        let new_table_keys = extract_table_component_keys(new);

        let derived_to_change = old_table_keys
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
                old_value != new_value
            })
            .cloned()
            .map(|(_table_key, derived_component_key)| derived_component_key)
            .collect::<HashSet<_>>();

        // Also track derived source/sink keys (e.g. memory tables).
        let old_component_keys = old_table_keys
            .into_iter()
            .map(|(_table_key, component_key)| component_key)
            .collect::<HashSet<_>>();
        let new_component_keys = new_table_keys
            .into_iter()
            .map(|(_table_key, component_key)| component_key)
            .collect::<HashSet<_>>();

        let derived_to_remove = &old_component_keys - &new_component_keys;
        let derived_to_add = &new_component_keys - &old_component_keys;

        Self {
            to_remove: table_diff
                .to_remove
                .union(&derived_to_remove)
                .cloned()
                .collect(),
            to_change: table_diff
                .to_change
                .union(&derived_to_change)
                .cloned()
                .collect(),
            to_add: table_diff.to_add.union(&derived_to_add).cloned().collect(),
        }
    }

    /// Returns true when there are no additions, changes, or removals.
    pub fn is_empty(&self) -> bool {
        self.to_add.is_empty() && self.to_change.is_empty() && self.to_remove.is_empty()
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
fn extract_table_component_keys(
    tables: &IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
) -> HashSet<(&ComponentKey, ComponentKey)> {
    tables
        .iter()
        .flat_map(|(table_key, table)| {
            vec![
                table
                    .as_source(table_key)
                    .map(|(component_key, _)| (table_key, component_key)),
                table
                    .as_sink(table_key)
                    .map(|(component_key, _)| (table_key, component_key)),
            ]
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod is_empty_tests {
    use crate::config::ConfigBuilder;
    use indoc::indoc;

    use super::*;

    fn basic_config() -> Config {
        serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
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
        .unwrap()
    }

    #[test]
    fn is_empty_when_configs_are_identical() {
        let config = basic_config();
        let diff = ConfigDiff::new(&config, &config, HashSet::new());
        assert!(diff.is_empty());
    }

    #[test]
    fn is_not_empty_when_sink_added() {
        let old = basic_config();
        let new: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
              other_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.sinks.is_added(&ComponentKey::from("other_sink")));
    }

    #[test]
    fn is_not_empty_when_sink_removed() {
        let old: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
              other_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();
        let new = basic_config();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.sinks.is_removed(&ComponentKey::from("other_sink")));
    }

    #[test]
    fn is_not_empty_when_sink_inputs_changed() {
        let old = basic_config();
        let new: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"
              test2:
                type: "test_basic"
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test2"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.sinks.is_changed(&ComponentKey::from("test_sink")));
        assert!(diff.sources.is_added(&ComponentKey::from("test2")));
    }

    #[test]
    fn is_not_empty_when_source_added() {
        let old = basic_config();
        let new: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"
              test2:
                type: "test_basic"
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.sources.is_added(&ComponentKey::from("test2")));
    }

    #[test]
    fn is_not_empty_when_transform_added() {
        let old = basic_config();
        let new: Config = serde_yaml::from_str::<ConfigBuilder>(indoc! {r#"
            sources:
              test:
                type: "test_basic"
            transforms:
              xform:
                type: "test_basic"
                inputs: ["test"]
                suffix: "-x"
                increase: 0.0
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["xform"]
        "#})
        .unwrap()
        .build()
        .unwrap();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.transforms.is_added(&ComponentKey::from("xform")));
        assert!(diff.sinks.is_changed(&ComponentKey::from("test_sink")));
    }

    #[test]
    fn is_not_empty_when_component_forced_to_reload() {
        let config = basic_config();
        let forced = HashSet::from([ComponentKey::from("test_sink")]);
        let diff = ConfigDiff::new(&config, &config, forced);
        assert!(!diff.is_empty());
        assert!(diff.sinks.is_changed(&ComponentKey::from("test_sink")));
    }

    fn config_with_file_enrichment(path: &str) -> Config {
        serde_yaml::from_str::<ConfigBuilder>(&format!(
            r#"
            sources:
              test:
                type: "test_basic"
            sinks:
              test_sink:
                type: "test_basic"
                inputs: ["test"]
            enrichment_tables:
              geo:
                type: "file"
                file:
                  path: "{path}"
                  encoding:
                    type: "csv"
        "#
        ))
        .unwrap()
        .build()
        .unwrap()
    }

    #[test]
    fn is_empty_when_standalone_file_enrichment_table_unchanged() {
        let config = config_with_file_enrichment("/tmp/vector-enrichment-same.csv");
        let diff = ConfigDiff::new(&config, &config, HashSet::new());
        assert!(diff.is_empty());
    }

    #[test]
    fn is_not_empty_when_standalone_file_enrichment_table_added() {
        let old = basic_config();
        let new = config_with_file_enrichment("/tmp/vector-enrichment-test.csv");

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty(), "file enrichment table add should not be empty");
        assert!(diff.enrichment_tables.is_added(&ComponentKey::from("geo")));
    }

    #[test]
    fn is_not_empty_when_standalone_file_enrichment_table_changed() {
        let old = config_with_file_enrichment("/tmp/vector-enrichment-old.csv");
        let new = config_with_file_enrichment("/tmp/vector-enrichment-new.csv");

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.enrichment_tables.is_changed(&ComponentKey::from("geo")));
    }

    #[test]
    fn is_not_empty_when_standalone_file_enrichment_table_removed() {
        let old = config_with_file_enrichment("/tmp/vector-enrichment-test.csv");
        let new = basic_config();

        let diff = ConfigDiff::new(&old, &new, HashSet::new());
        assert!(!diff.is_empty());
        assert!(diff.enrichment_tables.is_removed(&ComponentKey::from("geo")));
    }
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

        let diff = Difference::from_enrichment_tables(
            &old_config.enrichment_tables,
            &new_config.enrichment_tables,
        );

        assert_eq!(diff.to_add, HashSet::from_iter(["memory_table_new".into()]));
        assert_eq!(
            diff.to_remove,
            HashSet::from_iter(["memory_table_old".into()])
        );
        assert_eq!(
            diff.to_change,
            HashSet::from_iter(["memory_table".into(), "memory_table_source".into()])
        );
    }
}
