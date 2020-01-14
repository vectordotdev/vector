use crate::{
    event::Event,
    sources::{
        file::{FileConfig, FingerprintingConfig},
        Source,
    },
    topology::config::{GlobalOptions, SourceConfig},
};
use futures::sync::mpsc;

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
        self.file_source_include()?;
        self.file_source_exclude();

        self.file_config.start_at_beginning = true;

        // Filter out files that certainly don't have logs newer than now timestamp.
        self.file_config.ignore_older = Some(10);

        // oldest_first false, having all pods equaly serviced is of greater importance
        //                     than having time order guarantee.

        // CRI standard ensures unique naming.
        self.file_config.fingerprinting = FingerprintingConfig::DevInode;

        // Have a subdirectory for this source to avoid collision of naming its file source.
        self.file_config.data_dir = Some(globals.resolve_and_make_data_subdir(None, kube_name)?);

        let (file_send, file_recv) = mpsc::channel(1000);
        let file_source = self
            .file_config
            .build("file_source", globals, file_send)
            .map_err(|e| format!("Failed in creating file source with error: {:?}", e))?;

        Ok((file_recv, file_source))
    }

    /// Configures include in FileConfig
    fn file_source_include(&mut self) -> crate::Result<()> {
        // Prepare include patterns

        // Contains patterns that match on all possible included UIDs.
        let include_pod_uids = self
            .numerical_pod_uids()?
            .into_iter()
            .flat_map(|uid| to_uid_forms(uid.as_str()))
            .collect();

        // Pattern that matches to all UIDs, and only UIDs.
        let any_uid = to_uid_forms("");
        let pod_uids = not_empty_or_else(&include_pod_uids, &any_uid);

        // Contains patterns: container_name*
        let include_container_names = self
            .config
            .include_container_names
            .iter()
            .map(|name| name.clone() + "*")
            .collect();

        let any_name = vec!["*".to_string()];
        let container_names = not_empty_or_else(&include_container_names, &any_name);

        let any_namespace = vec!["*".to_string()];
        // Will match only on exactly included namespaces, or any if none specified.
        let namespaces = not_empty_or_else(&self.config.include_namespaces, &any_namespace);

        // The following creates and adds path includes of form:
        // LOG_DIRECTORY/uid/container_name/*.log
        //
        // To construct the paths, it's necessary to make every
        // possible combination of uid and container_name.
        for uid in pod_uids {
            for container_name in container_names {
                self.file_config.include.push(
                    (LOG_DIRECTORY.to_owned()
                        + format!("{}/{}/*.log", uid, container_name).as_str())
                    .into(),
                );
            }
        }

        // The following creates and adds path includes of form:
        // LOG_DIRECTORY/namespace_*_uid/container_name/*.log
        //
        // To construct the paths, it's necessary to make every
        // possible combination of uid, container_name, and namespace.
        for uid in pod_uids {
            for container_name in container_names {
                for namespace in namespaces {
                    self.file_config.include.push(
                        (LOG_DIRECTORY.to_owned()
                            + format!("{}_*_{}/{}/*.log", namespace, uid, container_name).as_str())
                        .into(),
                    );
                }
            }
        }

        Ok(())
    }

    /// Configures exclude in FileConfig
    fn file_source_exclude(&mut self) {
        // True if there is no includes
        let no_include = self.config.include_container_names.is_empty()
            && self.config.include_namespaces.is_empty()
            && self.config.include_pod_uids.is_empty();

        // Default excludes
        if no_include {
            // Since there is no user intention in including specific namespace/pod/container,
            // exclude kuberenetes and vector logs.

            // This is correct, but on best effort basis filtering out of logs from kuberentes system components.
            // More specificly, it will work for all Kubernetes 1.14 and higher, and for some bellow that.
            self.file_config
                .exclude
                .push((LOG_DIRECTORY.to_owned() + r"kube-system_*").into());

            // NOTE: for now exclude images with name vector, it's a rough solution, but necessary for now
            self.file_config
                .exclude
                .push((LOG_DIRECTORY.to_owned() + r"*/vector*").into());
        }
    }

    /// Checks included uids for validity, and transforms them to numerical format.
    fn numerical_pod_uids(&self) -> Result<Vec<String>, BuildError> {
        let mut include_pod_uids = Vec::new();
        for pod_uid in &self.config.include_pod_uids {
            let mut pod_uid = pod_uid.clone();

            // Check if pod_uid contains only UID valid characters
            if !pod_uid.chars().all(|c| {
                c.is_numeric() || ('A'..='F').contains(&c) || ('a'..='f').contains(&c) || c == '-'
            }) {
                error!(message = "Configuration 'include_pod_uids' contains not UID", uid = ?pod_uid);
                return Err(BuildError::IllegalCharacterInUid {
                    uid: pod_uid.clone(),
                });
            }

            // Remove dashes from uid so to always have numerical format
            // of uid.
            pod_uid.retain(|c| c != '-');

            if pod_uid.chars().count() > 32 {
                return Err(BuildError::UidToLarge {
                    uid: pod_uid.clone(),
                });
            }

            include_pod_uids.push(pod_uid);
        }

        Ok(include_pod_uids)
    }
}

/// Transforms uid to two UID forms defined by https://tools.ietf.org/html/rfc4122.
///  * 32 character number (numercial format)
///  * 8_char-4_char-4_char-4_char-12_char
/// If uid has less than 32 hexadecimal digits, if will be filled up
/// with "[0-9A-Fa-f]" per digit up to 32 .
fn to_uid_forms(uid: &str) -> Vec<String> {
    let mut it = uid.chars();
    vec![
        format!(
            "{}-{}-{}-{}-{}",
            char_or_hexadecimal(&mut it, 8),
            char_or_hexadecimal(&mut it, 4),
            char_or_hexadecimal(&mut it, 4),
            char_or_hexadecimal(&mut it, 4),
            char_or_hexadecimal(&mut it, 12)
        ),
        char_or_hexadecimal(&mut uid.chars(), 32),
    ]
}

/// Joins n next characters from iterator.
/// If there is not enough of characters, uses "[0-9A-Fa-f]" instead of a character.
fn char_or_hexadecimal(it: &mut impl Iterator<Item = char>, n: usize) -> String {
    let mut tmp = String::new();
    for _ in 0..n {
        if let Some(item) = it.next() {
            tmp.push(item);
        } else {
            tmp.push_str("[0-9A-Fa-f]")
        }
    }
    tmp
}

/// A helper method to remove duplication.
fn not_empty_or_else<'a>(o: &'a Vec<String>, default: &'a Vec<String>) -> &'a Vec<String> {
    if o.is_empty() {
        default
    } else {
        o
    }
}
