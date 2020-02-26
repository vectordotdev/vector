use crate::{
    event::Event,
    sources::{file::FileConfig, Source},
    topology::config::{GlobalOptions, SourceConfig},
};
use futures01::sync::mpsc;
use std::iter::FromIterator;

use super::{BuildError, KubernetesConfig, LOG_DIRECTORY};

/// Structure used for building FileSource.
pub struct FileSourceBuilder<'a> {
    config: &'a KubernetesConfig,
    file_config: FileConfig,
}

impl<'a> FileSourceBuilder<'a> {
    pub fn new(config: &'a KubernetesConfig) -> Self {
        Self {
            config,
            file_config: FileConfig::default(),
        }
    }

    pub fn build(
        mut self,
        kube_name: &str,
        globals: &GlobalOptions,
    ) -> crate::Result<(mpsc::Receiver<Event>, Source)> {
        self.file_config.include.extend(
            Self::file_source_include(self.config)?
                .into_iter()
                .map(Into::into),
        );
        self.file_config.exclude.extend(
            Self::file_source_exclude(self.config)
                .into_iter()
                .map(Into::into),
        );

        self.file_config.start_at_beginning = true;

        // Filter out files that certainly don't have logs newer than now timestamp.
        self.file_config.ignore_older = Some(10);

        // oldest_first false, having all pods equaly serviced is of greater importance
        //                     than having time order guarantee.

        // CRI standard ensures unique naming, but Docker log rotation tends to re-use inodes,
        // so we keep the default checksum fingerprinting strategy.

        // Have a subdirectory for this source to avoid collision of naming its file source.
        self.file_config.data_dir = Some(globals.resolve_and_make_data_subdir(None, kube_name)?);

        let (file_send, file_recv) = mpsc::channel(1000);
        let file_source = self
            .file_config
            .build("file_source", globals, file_send)
            .map_err(|e| format!("Failed in creating file source with error: {}", e))?;

        Ok((file_recv, file_source))
    }

    /// Creates includes for FileConfig
    fn file_source_include(config: &KubernetesConfig) -> crate::Result<Vec<String>> {
        // Paths to log files can contain: namespace, pod uid, and container name.
        // This property of paths is exploited by embeding config.include_* filters into globs.
        // https://en.wikipedia.org/wiki/Glob_(programming)
        //
        // These globs are passed to file_source which will then only listen for
        // log files that satisfy those globs.
        //
        // This method constructs those globs and adds them to the file source configuration.
        //
        // We are using globs to read only the log files of containers whose
        // metadata (namespace, pod uid, name) passes include_* filters. That way, we are
        // reading less, and processing less, so that's a performace win.

        // For constructing globs it's important how include filters interact:
        //  - Inside same filter, different values are alteratives. OR
        //    Because of this it's necessary to have at least one glob per value per filter.
        //  - Between filters, values are necessary. AND
        //    Because of this all globs of one filter need to be paired up with all globs of other filter.
        //    And then those pairs with globs of third filter.

        let namespaces = if config.include_namespaces.is_empty() {
            // Any namespace
            vec!["*".to_string()]
        } else {
            // Globs that match only on included namespaces.
            // Example:
            // With include_namespaces = ["telemetry","app"],
            // this will be:[
            //      "telemetry"
            //      "app"
            // ]
            config.include_namespaces.clone()
        };

        let pod_uids = if config.include_pod_uids.is_empty() {
            // Pattern that matches to all UIDs, and only UIDs per https://tools.ietf.org/html/rfc4122.
            // Will be:[
            //     "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]"
            //     "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]"
            // ]
            PartialUid::default().uid_globs()
        } else {
            // Constructs globs that match on uids starting with any partial uid in include_pod_uid.
            // Example:
            // With include_pod_uid = ["a027f09d8f18234519fa930f8fa71234","8f0290f83cb28fff201a8fcc310928"],
            // this will be:[
            //      "a027f09d-8f18-2345-19fa-930f8fa71234"
            //      "a027f09d8f18234519fa930f8fa71234"
            //      "8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]"
            //      "8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]"
            // ]
            config
                .include_pod_uids
                .clone()
                .into_iter()
                .map(PartialUid::new)
                .collect::<Result<Vec<_>, BuildError>>()?
                .iter()
                .flat_map(PartialUid::uid_globs)
                .collect()
        };

        let container_names = if config.include_container_names.is_empty() {
            // Any name
            vec!["*".to_string()]
        } else {
            // Constructs globs that match on container names starting with any prefix in
            // include_container_names.
            // Example:
            // With include_container_names = ["busybox","redis"],
            // this will be:[
            //      "busybox*"
            //      "redis*"
            // ]
            config
                .include_container_names
                .iter()
                .map(|name| name.clone() + "*")
                .collect()
        };

        // Glob includes
        let mut include = Vec::new();

        // The following creates and adds globs of form:
        // LOG_DIRECTORY/uid/container_name/*.log
        //
        // Files matching those globs will be monitored for logs.
        //
        // To construct the paths, it's necessary to make every
        // possible combination of uid globs and container_name globs.
        //
        // Examples:
        //
        // 1.
        //  - Empty include_pod_uid
        //  - Empty include_container_names
        // -------------------------------------------------------------------------------------------
        // Globs:
        //      LOG_DIRECTORY/[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log
        //      LOG_DIRECTORY/[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log
        //
        // 2.
        //  - include_pod_uid = ["a027f09d8f18234519fa930f8fa71234","8f0290f83cb28fff201a8fcc310928"]
        //  - Empty include_container_names
        // -------------------------------------------------------------------------------------------
        // Globs:
        //      LOG_DIRECTORY/a027f09d-8f18-2345-19fa-930f8fa71234/*/*.log
        //      LOG_DIRECTORY/a027f09d8f18234519fa930f8fa71234/*/*.log
        //      LOG_DIRECTORY/8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log
        //      LOG_DIRECTORY/8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log
        //
        // 3.
        //  - include_pod_uid = ["a027f09d8f18234519fa930f8fa71234","8f0290f83cb28fff201a8fcc310928"]
        //  - include_container_names = ["busybox","redis"]
        // -------------------------------------------------------------------------------------------
        // Globs:
        //      LOG_DIRECTORY/a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/a027f09d-8f18-2345-19fa-930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/a027f09d8f18234519fa930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/a027f09d8f18234519fa930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        //      LOG_DIRECTORY/8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        for uid in &pod_uids {
            for container_name in &container_names {
                include.push(
                    (LOG_DIRECTORY.to_owned()
                        + format!("{}/{}/*.log", uid, container_name).as_str())
                    .into(),
                );
            }
        }

        // The following creates and adds globs of form:
        // LOG_DIRECTORY/namespace_*_uid/container_name/*.log
        //
        // To construct the paths, it's necessary to make every
        // possible combination of namespace globs, uid globs, and container name globs.
        //
        // Example:
        //  - include_namespaces = ["telemetry","app"]
        //  - include_pod_uid = ["a027f09d8f18234519fa930f8fa71234","8f0290f83cb28fff201a8fcc310928"]
        //  - include_container_names = ["busybox","redis"]
        // -------------------------------------------------------------------------------------------
        // Globs:
        //      LOG_DIRECTORY/telemetry_*_a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/telemetry_*_a027f09d-8f18-2345-19fa-930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/telemetry_*_a027f09d8f18234519fa930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/telemetry_*_a027f09d8f18234519fa930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/telemetry_*_8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/telemetry_*_8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        //      LOG_DIRECTORY/telemetry_*_8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/telemetry_*_8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        //      LOG_DIRECTORY/app_*_a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/app_*_a027f09d-8f18-2345-19fa-930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/app_*_a027f09d8f18234519fa930f8fa71234/busybox*/*.log
        //      LOG_DIRECTORY/app_*_a027f09d8f18234519fa930f8fa71234/redis*/*.log
        //      LOG_DIRECTORY/app_*_8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/app_*_8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        //      LOG_DIRECTORY/app_*_8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log
        //      LOG_DIRECTORY/app_*_8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/redis*/*.log
        for namespace in &namespaces {
            for uid in &pod_uids {
                for container_name in &container_names {
                    include.push(
                        (LOG_DIRECTORY.to_owned()
                            + format!("{}_*_{}/{}/*.log", namespace, uid, container_name).as_str())
                        .into(),
                    );
                }
            }
        }

        Ok(include)
    }

    /// Returns excludes for FileConfig.
    ///
    /// By default it's good to exclude "kube-system" namespace and "vector*" container name.
    /// But that default can/must be turned off if the user has anything included. The reason being:
    /// a) if user hasn't included  "kube-system" or "vector*", than they will be filtered out
    ///    with include, so exclude isn't necessary.
    /// b) if user has included "kube-system" or "vector*", then that is a sign that user wants
    ///    to log it so excluding it is not valid.
    fn file_source_exclude(config: &KubernetesConfig) -> Vec<String> {
        // True if there is no includes
        let no_include = config.include_container_names.is_empty()
            && config.include_namespaces.is_empty()
            && config.include_pod_uids.is_empty();

        let mut exclude = Vec::new();
        // Default excludes
        if no_include {
            // Since there is no user intention in including specific namespace/pod/container,
            // exclude kuberenetes and vector logs.

            // This is correct, but on best effort basis filtering out of logs from kuberentes system components.
            // More specificly, it will work for all Kubernetes 1.14 and higher, and for some bellow that.
            exclude.push((LOG_DIRECTORY.to_owned() + r"kube-system_*").into());

            // NOTE: for now exclude images with name vector, it's a rough solution, but necessary for now
            exclude.push((LOG_DIRECTORY.to_owned() + r"*/vector*").into());
        }

        exclude
    }
}

/// Partial UUID in numerical format (without the '-') as defined by https://tools.ietf.org/html/rfc4122.
/// Has [0,32] UUID characters.
#[derive(Clone, Debug, Default)]
struct PartialUid {
    partial_uid: String,
}

impl PartialUid {
    /// Checks partial_uid for validity, and transforms them to numerical format.
    fn new(mut partial_uid: String) -> Result<Self, BuildError> {
        // Check if partial_uid contains only UUID valid characters as defined by https://tools.ietf.org/html/rfc4122.
        if !partial_uid.chars().all(|c| {
            c.is_numeric() || ('A'..='F').contains(&c) || ('a'..='f').contains(&c) || c == '-'
        }) {
            return Err(BuildError::IllegalCharacterInUid {
                uid: partial_uid.clone(),
            });
        }

        // Remove dashes from uid so to always have numerical format
        // of uid.
        partial_uid.retain(|c| c != '-');

        if partial_uid.chars().count() > 32 {
            return Err(BuildError::UidToLarge {
                uid: partial_uid.clone(),
            });
        }

        Ok(Self { partial_uid })
    }

    /// Transforms partial uid to two globs that match on two UUID forms
    /// as defined by https://tools.ietf.org/html/rfc4122:
    ///  - 8char-4char-4char-4char-12char
    ///  - 32 character number (numerical format)
    /// If partial_uid has less than 32 UUID characters, if will be filled up
    /// with "[0-9A-Fa-f]" glob pattern so to match exactly 32 UUID characters.
    fn uid_globs(&self) -> Vec<String> {
        let parts = self
            .partial_uid
            .chars()
            .map(|ch| String::from_iter(Some(ch)))
            .chain((0..32).into_iter().map(|_| "[0-9A-Fa-f]".to_owned()))
            .collect::<Vec<String>>();

        vec![
            format!(
                "{}-{}-{}-{}-{}",
                parts[0..8].concat(),
                parts[8..12].concat(),
                parts[12..16].concat(),
                parts[16..20].concat(),
                parts[20..32].concat(),
            ),
            parts[0..32].concat(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{FileSourceBuilder, KubernetesConfig, PartialUid, LOG_DIRECTORY};
    use std::collections::HashSet;
    use std::iter::FromIterator;

    #[test]
    fn invalid_uuid_char() {
        assert!(PartialUid::new("afp".to_owned()).is_err());
    }

    #[test]
    fn uuid_to_big() {
        assert!(PartialUid::new((0..33).map(|_| "0").collect::<String>()).is_err());
    }

    #[test]
    fn valid_partial_uuid() {
        assert!(PartialUid::new("af04".to_owned()).is_ok());
    }

    #[test]
    fn full_globs() {
        assert_eq!(
            HashSet::<String>::from_iter(
                PartialUid::new("a027f09d8f18234519fa930f8fa71234".to_owned())
                    .unwrap()
                    .uid_globs()
            ),
            HashSet::<String>::from_iter(vec![
                "a027f09d8f18234519fa930f8fa71234".to_owned(),
                "a027f09d-8f18-2345-19fa-930f8fa71234".to_owned()
            ])
        );
    }

    #[test]
    fn partial_globs() {
        assert_eq!(
            HashSet::<String>::from_iter(
                PartialUid::new("8f0290f83cb28fff201a8fcc310928".to_owned())
                    .unwrap()
                    .uid_globs()
            ),
            HashSet::<String>::from_iter(vec![
                "8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]".to_owned(),
                "8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]".to_owned()
            ])
        );
    }

    #[test]
    fn empty_filters() {
        let config = KubernetesConfig {
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
              LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
              LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
              LOG_DIRECTORY.to_owned() + "*_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
              LOG_DIRECTORY.to_owned() + "*_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log"
            ])
        );
    }

    #[test]
    fn pod_uid_filter() {
        let config = KubernetesConfig {
            include_pod_uids: vec![
                "a027f09d8f18234519fa930f8fa71234".to_owned(),
                "8f0290f83cb28fff201a8fcc310928".to_owned(),
            ],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "a027f09d-8f18-2345-19fa-930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d8f18234519fa930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_a027f09d-8f18-2345-19fa-930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_a027f09d8f18234519fa930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "*_*_8f0290f8-3cb2-8fff-201a-8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "*_*_8f0290f83cb28fff201a8fcc310928[0-9A-Fa-f][0-9A-Fa-f]/*/*.log"
            ])
        );
    }

    #[test]
    fn name_filter() {
        let config = KubernetesConfig {
            include_container_names: vec!["busybox".to_owned()],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log"
            ])
        );
    }

    #[test]
    fn namespace_filter() {
        let config = KubernetesConfig {
            include_namespaces: vec!["telemetry".to_owned()],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned() + "telemetry_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log",
                LOG_DIRECTORY.to_owned() + "telemetry_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/*/*.log"
            ])
        );
    }

    #[test]
    fn pod_uid_and_name_filter() {
        let config = KubernetesConfig {
            include_pod_uids: vec!["a027f09d8f18234519fa930f8fa71234".to_owned()],
            include_container_names: vec!["busybox".to_owned(), "redis".to_owned()],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d8f18234519fa930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "*_*_a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_a027f09d8f18234519fa930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d-8f18-2345-19fa-930f8fa71234/redis*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d8f18234519fa930f8fa71234/redis*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_a027f09d-8f18-2345-19fa-930f8fa71234/redis*/*.log",
                LOG_DIRECTORY.to_owned() + "*_*_a027f09d8f18234519fa930f8fa71234/redis*/*.log"
            ])
        );
    }

    #[test]
    fn pod_uid_and_namespace_filter() {
        let config = KubernetesConfig {
            include_pod_uids: vec!["a027f09d8f18234519fa930f8fa71234".to_owned()],
            include_namespaces: vec!["telemetry".to_owned()],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "a027f09d-8f18-2345-19fa-930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d8f18234519fa930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "telemetry_*_a027f09d-8f18-2345-19fa-930f8fa71234/*/*.log",
                LOG_DIRECTORY.to_owned() + "telemetry_*_a027f09d8f18234519fa930f8fa71234/*/*.log"
            ])
        );
    }

    #[test]
    fn name_and_namespace_filter() {
        let config = KubernetesConfig {
            include_container_names: vec!["busybox".to_owned()],
            include_namespaces: vec!["telemetry".to_owned()],
            ..KubernetesConfig::default()
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "telemetry_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]-[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "telemetry_*_[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]/busybox*/*.log"
            ])
        );
    }

    #[test]
    fn all_filters() {
        let config = KubernetesConfig {
            include_pod_uids: vec!["a027f09d8f18234519fa930f8fa71234".to_owned()],
            include_container_names: vec!["busybox".to_owned()],
            include_namespaces: vec!["telemetry".to_owned(), "app".to_owned()],
        };

        assert_eq!(
            HashSet::<String>::from_iter(FileSourceBuilder::file_source_include(&config).unwrap()),
            HashSet::from_iter(vec![
                LOG_DIRECTORY.to_owned() + "a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "a027f09d8f18234519fa930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "telemetry_*_a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "telemetry_*_a027f09d8f18234519fa930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned()
                    + "app_*_a027f09d-8f18-2345-19fa-930f8fa71234/busybox*/*.log",
                LOG_DIRECTORY.to_owned() + "app_*_a027f09d8f18234519fa930f8fa71234/busybox*/*.log",
            ])
        );
    }
}
