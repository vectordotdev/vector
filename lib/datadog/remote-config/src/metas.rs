use crate::Version;
use indexmap::IndexMap;
use serde::Deserialize;
use std::{collections::HashMap, fmt};

#[derive(Debug, Deserialize)]
pub struct ConfigMeta {
    pub signatures: Vec<Signature>,
    pub signed: Role,
}

#[derive(Debug, Deserialize)]
pub struct Signature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_type", rename_all = "snake_case")]
pub enum Role {
    Root(Root),
    Snapshot(Snapshot),
    Targets(Targets),
    Timestamp(Timestamp),
}

#[derive(Default)]
pub struct ConfigMetas {
    pub root: IndexMap<Version, Root>,
    pub timestamp: IndexMap<Version, Timestamp>,
    pub snapshot: IndexMap<Version, Snapshot>,
    pub top_targets: IndexMap<Version, Targets>,
    pub delegated_targets: Vec<DelegatedTargets>,
}

impl fmt::Debug for ConfigMetas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metas")
            .field("timestamp", &self.timestamp.last().map(|(_, v)| v))
            .field("snapshot", &self.snapshot.last().map(|(_, v)| v))
            .field("top_targets", &self.top_targets)
            .field("delegated_targets", &self.delegated_targets)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct DirectorMetas {
    pub root: IndexMap<Version, Root>,
    pub timestamp: IndexMap<Version, Timestamp>,
    pub snapshot: IndexMap<Version, Snapshot>,
    pub targets: IndexMap<Version, Targets>,
}

impl fmt::Debug for DirectorMetas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metas")
            .field("timestamp", &self.timestamp.last().map(|(_, v)| v))
            .field("snapshot", &self.snapshot.last().map(|(_, v)| v))
            .field("targets", &self.targets.last().map(|(_, v)| v))
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
pub struct Root {
    pub spec_version: String,
    pub consistent_snapshot: bool,
    pub version: u64,
    pub expires: String,
    pub keys: HashMap<String, KeyInner>,
    pub roles: HashMap<String, RoleInner>,
}

#[derive(Debug, Deserialize)]
pub struct Snapshot {
    pub spec_version: String,
    pub version: u64,
    pub expires: String,
    pub meta: HashMap<String, MetaFile>,
}

#[derive(Debug, Deserialize)]
pub struct Targets {
    pub spec_version: String,
    pub version: u64,
    pub expires: String,
    pub targets: HashMap<String, TargetFile>,
    // TODO: structs for these
    pub delegations: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Timestamp {
    pub spec_version: String,
    pub version: u64,
    pub expires: String,
    pub meta: HashMap<String, MetaFile>,
}

#[derive(Debug, Deserialize)]
pub struct KeyInner {
    pub keytype: String,
    pub scheme: String,
    pub keyval: KeyVal,
    // this is not in the TUF spec
    pub keyid_hash_algorithms: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct KeyVal {
    pub public: String,
}

#[derive(Debug, Deserialize)]
pub struct RoleInner {
    pub keyids: Vec<String>,
    pub threshold: u64,
}

#[derive(Debug, Deserialize)]
pub struct MetaFile {
    pub version: u64,
    pub length: u64,
    pub hashes: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct TargetFile {
    pub length: u64,
    pub hashes: HashMap<String, String>,
    pub custom: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct DelegatedTargets {
    pub version: Version,
    pub role: String,
    pub targets: Targets,
}
