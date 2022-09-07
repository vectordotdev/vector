use std::{fs::DirBuilder, path::PathBuf};

use snafu::{ResultExt, Snafu};
use vector_common::TimeZone;
use vector_config::configurable_component;

use super::{proxy::ProxyConfig, AcknowledgementsConfig, LogSchema};
use crate::serde::bool_or_struct;

#[derive(Debug, Snafu)]
pub(crate) enum DataDirError {
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

/// Global configuration options.
//
// If this is modified, make sure those changes are reflected in the `ConfigBuilder::append`
// function!
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GlobalOptions {
    /// The directory used for persisting Vector state data.
    ///
    /// This is the directory where Vector will store any state data, such as disk buffers, file
    /// checkpoints, and more.
    ///
    /// Vector must have write permissions to this directory.
    #[serde(default = "crate::default_data_dir")]
    pub data_dir: Option<PathBuf>,

    /// Default log schema for all events.
    ///
    /// This is used if a component does not have its own specific log schema. All events use a log
    /// schema, whether or not the default is used, to assign event fields on incoming events.
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub log_schema: LogSchema,

    /// The name of the timezone to apply to timestamp conversions that do not contain an explicit timezone.
    ///
    /// The timezone name may be any name in the [TZ database][tzdb], or `local` to indicate system
    /// local time.
    ///
    /// [tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub timezone: TimeZone,

    #[configurable(derived)]
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub proxy: ProxyConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    /// The amount of time, in seconds, that internal metrics will persist after having not been
    /// updated before they expire and are removed.
    ///
    /// Not set by default, which allows all internal metrics to grow unbounded over time. If you
    /// have a configuration that emits many high-cardinality metrics, you may want to consider
    /// setting this to a value that ensures that metrics live long enough to be emitted and
    /// captured, but not so long that they continue to build up indefinitely, as this will consume
    /// a small amount of memory for each metric.
    #[configurable(deprecated)]
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub expire_metrics: Option<f64>,

    /// The amount of time, in seconds, that internal metrics will persist after having not been
    /// updated before they expire and are removed.
    ///
    /// Not set by default, which allows all internal metrics to grow unbounded over time. If you
    /// have a configuration that emits many high-cardinality metrics, you may want to consider
    /// setting this to a value that ensures that metrics live long enough to be emitted and
    /// captured, but not so long that they continue to build up indefinitely, as this will consume
    /// a small amount of memory for each metric.
    #[serde(skip_serializing_if = "crate::serde::skip_serializing_if_default")]
    pub expire_metrics_secs: Option<f64>,
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
            .or(self.data_dir.as_ref())
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
