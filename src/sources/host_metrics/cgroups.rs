use std::{
    io::{self, Read},
    num::ParseIntError,
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use vector_common::btreemap;

use super::{filter_result_sync, FilterList, HostMetrics};
use crate::event::metric::Metric;

const MICROSECONDS: f64 = 1.0 / 1_000_000.0;

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default)]
pub(super) struct CGroupsConfig {
    #[derivative(Default(value = "100"))]
    levels: usize,
    pub(super) base: Option<PathBuf>,
    groups: FilterList,
}

#[derive(Debug, Snafu)]
enum CGroupsError {
    #[snafu(display("Could not open cgroup data file {:?}.", filename))]
    Opening {
        filename: PathBuf,
        source: io::Error,
    },
    #[snafu(display("Could not read cgroup data file {:?}.", filename))]
    Reading {
        filename: PathBuf,
        source: io::Error,
    },
    #[snafu(display("Could not parse cgroup data file {:?}.", filename))]
    Parsing {
        filename: PathBuf,
        source: ParseIntError,
    },
}

type CGroupsResult<T> = Result<T, CGroupsError>;

impl HostMetrics {
    pub async fn cgroups_metrics(&self) -> Vec<Metric> {
        let now = Utc::now();
        let mut buffer = String::new();
        let mut output = Vec::new();
        if let Some(root) = self.root_cgroup.clone() {
            self.recurse_cgroup(&mut output, now, root, 1, &mut buffer)
                .await;
        }
        output
    }

    fn recurse_cgroup<'a>(
        &'a self,
        result: &'a mut Vec<Metric>,
        now: DateTime<Utc>,
        cgroup: CGroup,
        level: usize,
        buffer: &'a mut String,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let tags = btreemap! {
                "cgroup" => cgroup.name.to_string_lossy(),
                "collector" => "cgroups",
            };
            if let Some(cpu) = filter_result_sync(
                cgroup.load_cpu(buffer).await,
                "Failed to load cgroups CPU statistics.",
            ) {
                result.push(self.counter(
                    "cgroup_cpu_usage_seconds_total",
                    now,
                    cpu.usage_usec as f64 * MICROSECONDS,
                    tags.clone(),
                ));
                result.push(self.counter(
                    "cgroup_cpu_user_seconds_total",
                    now,
                    cpu.user_usec as f64 * MICROSECONDS,
                    tags.clone(),
                ));
                result.push(self.counter(
                    "cgroup_cpu_system_seconds_total",
                    now,
                    cpu.system_usec as f64 * MICROSECONDS,
                    tags.clone(),
                ));
            }

            if cgroup.has_memory_controller && !cgroup.is_root() {
                if let Some(current) = filter_result_sync(
                    cgroup.load_memory_current(buffer).await,
                    "Failed to load cgroups current memory.",
                ) {
                    result.push(self.gauge(
                        "cgroup_memory_current_bytes",
                        now,
                        current as f64,
                        tags.clone(),
                    ));
                }

                if let Some(stat) = filter_result_sync(
                    cgroup.load_memory_stat(buffer).await,
                    "Failed to load cgroups memory statistics.",
                ) {
                    result.push(self.gauge(
                        "cgroup_memory_anon_bytes",
                        now,
                        stat.anon as f64,
                        tags.clone(),
                    ));
                    result.push(self.gauge(
                        "cgroup_memory_file_bytes",
                        now,
                        stat.file as f64,
                        tags,
                    ));
                }
            }

            if level < self.config.cgroups.levels {
                if let Some(children) =
                    filter_result_sync(cgroup.children().await, "Failed to load cgroups children.")
                {
                    for child in children {
                        if self.config.cgroups.groups.contains_path(Some(&child.name)) {
                            self.recurse_cgroup(result, now, child, level + 1, buffer)
                                .await;
                        }
                    }
                }
            }
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct CGroup {
    root: PathBuf,
    name: PathBuf,
    has_memory_controller: bool,
}

const CGROUP_CONTROLLERS: &str = "cgroup.controllers";

impl CGroup {
    pub(super) fn root<P: AsRef<Path>>(base_group: Option<P>) -> Option<CGroup> {
        // There are three standard possibilities for cgroups setups
        // (`BASE` below is normally `/sys/fs/cgroup`, but containers
        // sometimes have `/sys` mounted elsewhere):
        // 1. Legacy v1 cgroups mounted at `BASE`
        // 2. Modern v2 cgroups mounted at `BASE`
        // 3. Hybrid cgroups, with v1 mounted at `BASE` and v2 mounted at `BASE/unified`.
        //
        // The `unified` directory only exists if cgroups is operating
        // in "hybrid" mode. Similarly, v2 cgroups will always have a
        // file named `cgroup.procs` in the base directory, and that
        // file is never present in v1 cgroups. By testing for either
        // the hybrid directory or the base file, we can uniquely
        // identify the current operating mode and, critically, the
        // location of the v2 cgroups root directory.
        //
        // Within that v2 root directory, each cgroup is a subdirectory
        // named for the cgroup identifier. Each group, including the
        // root, contains a set of files representing the controllers
        // for that group.

        let base_dir = join_path(heim::os::linux::sysfs_root(), "fs/cgroup");

        let (base_dir, controllers_file) = {
            let hybrid_root = join_path(&base_dir, "unified");
            let test_file = join_path(&hybrid_root, CGROUP_CONTROLLERS);
            if is_file(&test_file) {
                (hybrid_root, test_file)
            } else {
                let test_file = join_path(&base_dir, CGROUP_CONTROLLERS);
                if is_file(&test_file) {
                    (base_dir, test_file)
                } else {
                    return None;
                }
            }
        };

        debug!(message = "Detected cgroup base directory.", ?base_dir);

        let controllers = load_controllers(&controllers_file)
            .map_err(
                |error| error!(message = "Could not load root cgroup controllers list.", %error, ?controllers_file),
            )
            .ok()?;
        let has_memory_controller = controllers.iter().any(|name| name == "memory");
        if !has_memory_controller {
            warn!(
                message =
                    "CGroups memory controller is not active, there will be no memory metrics."
            );
        }

        match base_group {
            Some(group) => {
                let group = group.as_ref();
                let root = join_path(base_dir, group);
                is_dir(&root).then(|| CGroup {
                    root,
                    name: group.into(),
                    has_memory_controller,
                })
            }
            None => Some(CGroup {
                root: base_dir,
                name: "/".into(),
                has_memory_controller,
            }),
        }
    }

    fn is_root(&self) -> bool {
        self.name == Path::new("/")
    }

    async fn load_cpu(&self, buffer: &mut String) -> CGroupsResult<CpuStat> {
        self.open_read_parse("cpu.stat", buffer).await
    }

    fn make_path(&self, filename: impl AsRef<Path>) -> PathBuf {
        join_path(&self.root, filename)
    }

    async fn open_read(
        &self,
        filename: impl AsRef<Path>,
        buffer: &mut String,
    ) -> CGroupsResult<PathBuf> {
        buffer.clear();
        let filename = self.make_path(filename);
        File::open(&filename)
            .await
            .with_context(|_| OpeningSnafu {
                filename: filename.clone(),
            })?
            .read_to_string(buffer)
            .await
            .with_context(|_| ReadingSnafu {
                filename: filename.clone(),
            })?;
        Ok(filename)
    }

    async fn open_read_parse<T: FromStr<Err = ParseIntError>>(
        &self,
        filename: impl AsRef<Path>,
        buffer: &mut String,
    ) -> CGroupsResult<T> {
        let filename = self.open_read(filename, buffer).await?;
        buffer
            .trim()
            .parse()
            .with_context(|_| ParsingSnafu { filename })
    }

    async fn load_memory_current(&self, buffer: &mut String) -> CGroupsResult<u64> {
        self.open_read_parse("memory.current", buffer).await
    }

    async fn load_memory_stat(&self, buffer: &mut String) -> CGroupsResult<MemoryStat> {
        self.open_read_parse("memory.stat", buffer).await
    }

    async fn children(&self) -> io::Result<Vec<CGroup>> {
        let mut result = Vec::new();
        let mut dir = fs::read_dir(&self.root).await?;
        while let Some(entry) = dir.next_entry().await? {
            let root = entry.path();
            if is_dir(&root) {
                result.push(CGroup {
                    root,
                    name: join_name(&self.name, entry.file_name()),
                    has_memory_controller: self.has_memory_controller,
                });
            }
        }
        Ok(result)
    }
}

fn load_controllers(filename: &Path) -> CGroupsResult<Vec<String>> {
    let mut buffer = String::new();
    std::fs::File::open(&filename)
        .with_context(|_| OpeningSnafu {
            filename: filename.to_path_buf(),
        })?
        .read_to_string(&mut buffer)
        .with_context(|_| ReadingSnafu {
            filename: filename.to_path_buf(),
        })?;
    Ok(buffer.trim().split(' ').map(Into::into).collect())
}

macro_rules! define_stat_struct {
    ($name:ident ( $( $field:ident, )* )) => {
        #[derive(Clone, Copy, Debug, Default)]
        struct $name {
            $( $field: u64, )*
        }

        impl FromStr for $name {
            type Err = ParseIntError;
            fn from_str(text:&str)->Result<Self,Self::Err>{
                let mut result = Self::default();
                for line in text.lines(){
                    if false {}
                    $(
                        else if line.starts_with(concat!(stringify!($field), ' ')) {
                            result.$field = line[stringify!($field).len()+1..].parse()?;
                        }
                    )*
                }
                Ok(result)
            }
        }
    };
}

define_stat_struct! { CpuStat(
    usage_usec,
    user_usec,
    system_usec,
)}

define_stat_struct! { MemoryStat(
    // This file contains *way* more fields than defined here, these are
    // just the ones used to provide the metrics here. See the
    // documentation on `memory.stat` at
    // https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html#memory
    // for more details.
    anon,
    file,
)}

fn is_dir(path: impl AsRef<Path>) -> bool {
    std::fs::metadata(path.as_ref())
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn is_file(path: impl AsRef<Path>) -> bool {
    std::fs::metadata(path.as_ref())
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

/// Join a base directory path with a cgroup name.
fn join_path(base_path: impl AsRef<Path>, filename: impl AsRef<Path>) -> PathBuf {
    let filename = filename.as_ref();
    let base_path = base_path.as_ref();
    if filename == Path::new("/") {
        // `/` is the base cgroup name, no changes to the base path
        base_path.into()
    } else {
        [base_path, filename].iter().collect()
    }
}

fn join_name(base_name: &Path, filename: impl AsRef<Path>) -> PathBuf {
    let filename = filename.as_ref();
    // Joining cgroups names works a little differently than path
    // names. All names are relative paths except for the base, which is
    // the literal `/`. So, we have to check for the literal before joining.
    if base_name == Path::new("/") {
        filename.into()
    } else {
        [base_name, filename].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;

    use super::{
        super::{
            tests::{count_name, count_tag},
            HostMetrics, HostMetricsConfig,
        },
        join_name, join_path,
    };

    #[test]
    fn joins_names_and_paths() {
        assert_eq!(join_name(Path::new("/"), "foo"), PathBuf::from("foo"));
        assert_eq!(join_name(Path::new("/"), "/"), PathBuf::from("/"));
        assert_eq!(join_name(Path::new("foo"), "bar"), PathBuf::from("foo/bar"));

        assert_eq!(join_path("/sys", "foo"), PathBuf::from("/sys/foo"));
        assert_eq!(join_path("/sys", "/"), PathBuf::from("/sys"));
    }

    #[tokio::test]
    async fn generates_cgroups_metrics() {
        let config: HostMetricsConfig = toml::from_str(r#"collectors = ["cgroups"]"#).unwrap();
        let metrics = HostMetrics::new(config).cgroups_metrics().await;

        assert!(!metrics.is_empty());
        assert_eq!(count_tag(&metrics, "cgroup"), metrics.len());
        assert!(count_name(&metrics, "cgroup_cpu_usage_seconds_total") > 0);
        assert!(count_name(&metrics, "cgroup_cpu_user_seconds_total") > 0);
        assert!(count_name(&metrics, "cgroup_cpu_system_seconds_total") > 0);
    }
}
