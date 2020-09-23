use super::fingerprint::Fingerprint;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
};

#[derive(Derivative, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[repr(C)]
pub struct ArtifactCache {
    fingerprints: HashMap<PathBuf, Fingerprint>,
    root: PathBuf,
}

impl ArtifactCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();

        match std::fs::File::open(root.clone().join(".fingerprints")) {
            Ok(fingerprint_file) => {
                let reader = std::io::BufReader::new(fingerprint_file);
                let fingerprints = serde_json::from_reader(reader)?;
                Ok(Self { fingerprints, root })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                fingerprints: HashMap::new(),
                root,
            }),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Returns true if the artifact cache is fresh and the artifact compilation can be skipped.
    pub fn has_fresh(&self, file: impl AsRef<Path> + Debug) -> Result<bool> {
        let file = file.as_ref();
        let fingerprint = Fingerprint::new(file)?;
        Ok(self.fingerprints.get(file) == Some(&fingerprint))
    }

    /// Parse `$ARTIFACT_CACHE/.fingerprints` and add the given fingerprint.
    pub fn upsert(
        &mut self,
        file: impl AsRef<Path> + Debug,
        fingerprint: Fingerprint,
    ) -> Result<()> {
        let file_path = file.as_ref();
        self.fingerprints
            .insert(file_path.to_path_buf(), fingerprint);

        let fingerprint_file = std::fs::File::create(self.root.clone().join(".fingerprints"))?;
        let writer = std::io::BufWriter::new(fingerprint_file);
        serde_json::to_writer_pretty(writer, &self.fingerprints)?;
        Ok(())
    }
}
