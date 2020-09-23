use crate::Result;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::{fmt::Debug, path::Path};

#[derive(Derivative, Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[repr(transparent)]
pub struct Fingerprint(u64);

impl Fingerprint {
    pub fn new(file: impl AsRef<Path> + Debug) -> Result<Fingerprint> {
        let path = file.as_ref();
        let meta = std::fs::metadata(path)?;

        let modified = meta.modified()?;
        let age = modified.duration_since(std::time::UNIX_EPOCH)?;
        Ok(Self(age.as_secs()))
    }
}

impl TryFrom<&Path> for Fingerprint {
    type Error = crate::Error;

    fn try_from(file: &Path) -> Result<Self> {
        Self::new(file)
    }
}

impl TryFrom<&str> for Fingerprint {
    type Error = crate::Error;

    fn try_from(file: &str) -> Result<Self> {
        Self::new(file)
    }
}
