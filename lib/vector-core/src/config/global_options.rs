use std::{fs::DirBuilder, path::PathBuf};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use vector_common::TimeZone;

use super::{proxy::ProxyConfig, AcknowledgementsConfig, LogSchema};
use crate::serde::bool_or_struct;

#[derive(Debug, Snafu)]
pub enum DataDirError {
    #[snafu(display("data_dir option required, but not given here or globally"))]
    MissingDataDir,
    #[snafu(display("data_dir {:?} does not exist", data_dir))]
    DoesNotExist { data_dir: PathBuf },
    #[snafu(display("data_dir {:?} is not writable", data_dir))]
    NotWritable { data_dir: PathBuf },
    #[snafu(display(
        "Could not create subdirectory {:?} inside of data dir {:?}: {}",
        subdir,
        data_dir,
        source
    ))]
    CouldNotCreate {
        subdir: PathBuf,
        data_dir: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct GlobalOptions {
    #[serde(default = "crate::default_data_dir")]
    pub data_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub log_schema: LogSchema,
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub timezone: TimeZone,
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub proxy: ProxyConfig,
    #[serde(skip)]
    pub enterprise: bool,
    #[serde(
        default,
        deserialize_with = "bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GlobalOptions {
    /// Resolve the `data_dir` option in either the global or local config, and
    /// validate that it exists and is writable.
    ///
    /// # Errors
    ///
    /// Function will error if it is unable to make data directory.
    pub fn resolve_and_validate_data_dir(
        &self,
        local_data_dir: Option<&PathBuf>,
    ) -> crate::Result<PathBuf> {
        let data_dir = local_data_dir
            .or_else(|| self.data_dir.as_ref())
            .ok_or(DataDirError::MissingDataDir)
            .map_err(Box::new)?
            .clone();
        if !data_dir.exists() {
            return Err(DataDirError::DoesNotExist { data_dir }.into());
        }
        let readonly = std::fs::metadata(&data_dir)
            .map(|meta| meta.permissions().readonly())
            .unwrap_or(true);
        if readonly {
            return Err(DataDirError::NotWritable { data_dir }.into());
        }
        Ok(data_dir)
    }

    /// Resolve the `data_dir` option using `resolve_and_validate_data_dir` and
    /// then ensure a named subdirectory exists.
    ///
    /// # Errors
    ///
    /// Function will error if it is unable to make data subdirectory.
    pub fn resolve_and_make_data_subdir(
        &self,
        local: Option<&PathBuf>,
        subdir: &str,
    ) -> crate::Result<PathBuf> {
        let data_dir = self.resolve_and_validate_data_dir(local)?;

        let mut data_subdir = data_dir.clone();
        data_subdir.push(subdir);

        DirBuilder::new()
            .recursive(true)
            .create(&data_subdir)
            .with_context(|_| CouldNotCreateSnafu { subdir, data_dir })?;
        Ok(data_subdir)
    }
}
