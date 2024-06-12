use std::{collections::BTreeSet, collections::HashMap, ffi::OsStr, fs, path::Path};

use anyhow::{bail, Context, Result};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::Value;

type ComponentMap = HashMap<String, Component>;

// Use a BTree to keep the results in sorted order
type FeatureSet = BTreeSet<String>;

macro_rules! mapping {
    ( $( $key:ident => $value:ident, )* ) => {
        HashMap::from([
            $( (stringify!($key), stringify!($value)), )*
        ])
    };
}

// Mapping of component names to feature name exceptions.

static SOURCE_FEATURE_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    mapping!(
        prometheus_pushgateway => prometheus,
        prometheus_scrape => prometheus,
        prometheus_remote_write => prometheus,
    )
});

static TRANSFORM_FEATURE_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| mapping!());

static SINK_FEATURE_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    mapping!(
        gcp_pubsub => gcp,
        gcp_cloud_storage => gcp,
        gcp_stackdriver_logs => gcp,
        gcp_stackdriver_metrics => gcp,
        prometheus_exporter => prometheus,
        prometheus_remote_write => prometheus,
        splunk_hec_logs => splunk_hec,
    )
});

/// This is a ersatz copy of the Vector config, containing just the elements we are interested in
/// examining. Everything else is thrown away.
#[derive(Deserialize)]
pub struct VectorConfig {
    api: Option<Value>,

    #[serde(default)]
    sources: ComponentMap,
    #[serde(default)]
    transforms: ComponentMap,
    #[serde(default)]
    sinks: ComponentMap,
}

#[derive(Deserialize)]
struct Component {
    r#type: String,
}

pub fn load_and_extract(filename: &Path) -> Result<Vec<String>> {
    let config =
        fs::read_to_string(filename).with_context(|| format!("failed to read {filename:?}"))?;

    let config: VectorConfig = match filename
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase)
        .as_deref()
    {
        None => bail!("Invalid filename {filename:?}, no extension"),
        Some("json") => serde_json::from_str(&config)?,
        Some("toml") => toml::from_str(&config)?,
        Some("yaml" | "yml") => serde_yaml::from_str(&config)?,
        Some(_) => bail!("Invalid filename {filename:?}, unknown extension"),
    };

    Ok(from_config(config))
}

pub fn from_config(config: VectorConfig) -> Vec<String> {
    let mut features = FeatureSet::default();
    add_option(&mut features, "api", &config.api);

    get_features(
        &mut features,
        "sources",
        config.sources,
        &SOURCE_FEATURE_MAP,
    );
    get_features(
        &mut features,
        "transforms",
        config.transforms,
        &TRANSFORM_FEATURE_MAP,
    );
    get_features(&mut features, "sinks", config.sinks, &SINK_FEATURE_MAP);

    // Set of always-compiled components, in terms of their computed feature flag, that should
    // not be emitted as they don't actually have a feature flag because we always compile them.
    features.remove("transforms-log_to_metric");

    features.into_iter().collect()
}

fn add_option<T>(features: &mut FeatureSet, name: &str, field: &Option<T>) {
    if field.is_some() {
        features.insert(name.into());
    }
}

// Extract the set of features for a particular key from the config, using the exception mapping to
// rewrite component names to their feature names where needed.
fn get_features(
    features: &mut FeatureSet,
    key: &str,
    section: ComponentMap,
    exceptions: &HashMap<&str, &str>,
) {
    features.extend(
        section
            .into_values()
            .map(|component| component.r#type)
            .map(|name| {
                exceptions
                    .get(name.as_str())
                    .map_or(name, ToString::to_string)
            })
            .map(|name| format!("{key}-{name}")),
    );
}
