use super::{filter_result_sync, FilterList, HostMetricsConfig};
use crate::event::metric::Metric;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::btreemap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default)]
pub(super) struct CgroupsConfig {
    #[derivative(Default(value = "3"))]
    levels: usize,
    base: Option<PathBuf>,
    groups: FilterList,
}

impl HostMetricsConfig {
    pub async fn cgroups_metrics(&self) -> Vec<Metric> {
        let now = Utc::now();
        CGroup::root(self.cgroups.base.as_deref())
            .map(|root| self.recurse_cgroup(now, root, 1))
            .unwrap_or_else(Vec::new)
    }

    fn recurse_cgroup(&self, now: DateTime<Utc>, cgroup: CGroup, level: usize) -> Vec<Metric> {
        let mut result = Vec::new();

        let tags = btreemap! {
            "cgroup" => cgroup.name.to_string_lossy(),
            "collector" => "cgroups",
        };
        if let Some(cpu) = filter_result_sync(
            cgroup.load_cpu(),
            "Failed to load/parse cgroups CPU statistics",
        ) {
            result.push(self.counter(
                "cgroups_cpu_usage",
                now,
                cpu.usage_usec as f64 / 1_000_000.0,
                tags.clone(),
            ));
            result.push(self.counter(
                "cgroups_cpu_user",
                now,
                cpu.user_usec as f64 / 1_000_000.0,
                tags.clone(),
            ));
            result.push(self.counter(
                "cgroups_cpu_system",
                now,
                cpu.system_usec as f64 / 1_000_000.0,
                tags.clone(),
            ));
        }

        if !cgroup.is_root() {
            if let Some(current) = filter_result_sync(
                cgroup.load_memory_current(),
                "Failed to load/parse cgroups current memory",
            ) {
                result.push(self.gauge("cgroup_memory_current", now, current as f64, tags.clone()));
            }

            if let Some(stat) = filter_result_sync(
                cgroup.load_memory_stat(),
                "Failed to load/parse cgroups memory statistics",
            ) {
                result.push(self.gauge("cgroup_memory_anon", now, stat.anon as f64, tags.clone()));
                result.push(self.gauge("cgroup_memory_file", now, stat.file as f64, tags));
            }
        }

        if level < self.cgroups.levels {
            if let Some(children) =
                filter_result_sync(cgroup.children(), "Failed to load cgroups children")
            {
                for child in children {
                    if self.cgroups.groups.contains_path(Some(&child.name)) {
                        result.extend(self.recurse_cgroup(now, child, level + 1));
                    }
                }
            }
        }

        result
    }
}

#[derive(Debug)]
struct CGroup {
    root: PathBuf,
    name: PathBuf,
}

impl CGroup {
    fn root<P: AsRef<Path>>(base_group: Option<P>) -> Option<CGroup> {
        let base_dir = join_path(heim::os::linux::sysfs_root(), "fs/cgroup");
        let root = join_path(&base_dir, "unified");
        is_dir(&root)
            .then(|| root)
            .or_else(|| is_file(join_path(&base_dir, "cgroup.procs")).then(|| base_dir))
            .and_then(|root| match base_group {
                Some(group) => {
                    let group = group.as_ref();
                    let root = join_path(root, group);
                    is_dir(&root).then(|| CGroup {
                        root,
                        name: group.into(),
                    })
                }
                None => Some(CGroup {
                    root,
                    name: "/".into(),
                }),
            })
    }

    fn is_root(&self) -> bool {
        self.name == Path::new("/")
    }

    fn load_cpu(&self) -> io::Result<CpuStat> {
        let mut data = String::new();
        File::open(self.make_path("cpu.stat"))?.read_to_string(&mut data)?;
        data.parse()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn make_path(&self, filename: impl AsRef<Path>) -> PathBuf {
        join_path(&self.root, filename)
    }

    fn load_memory_current(&self) -> io::Result<u64> {
        let mut current = String::new();
        File::open(self.make_path("memory.current"))?.read_to_string(&mut current)?;
        current.trim().parse().map_err(map_parse_error)
    }

    fn load_memory_stat(&self) -> io::Result<MemoryStat> {
        let mut data = String::new();
        File::open(self.make_path("memory.stat"))?.read_to_string(&mut data)?;
        data.parse().map_err(map_parse_error)
    }

    fn children(&self) -> io::Result<Vec<CGroup>> {
        fs::read_dir(&self.root)?
            .map(|result| {
                result.map(|entry| (entry.path(), join_name(&self.name, entry.file_name())))
            })
            .filter(|result| !matches!(result.as_ref().map(|(path, _)| is_dir(path)), Ok(false)))
            .map(|result| result.map(|(root, name)| CGroup { root, name }))
            .collect()
    }
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

fn map_parse_error(error: ParseIntError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn is_dir(path: impl AsRef<Path>) -> bool {
    fs::metadata(path.as_ref())
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn is_file(path: impl AsRef<Path>) -> bool {
    fs::metadata(path.as_ref())
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
    use super::super::tests::{count_name, count_tag};
    use super::super::HostMetricsConfig;
    use super::{join_name, join_path};
    use pretty_assertions::assert_eq;
    use std::path::{Path, PathBuf};

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
        let metrics = config.cgroups_metrics().await;

        assert!(!metrics.is_empty());
        assert_eq!(count_tag(&metrics, "cgroup"), metrics.len());
        assert!(count_name(&metrics, "cgroups_cpu_usage") > 0);
        assert!(count_name(&metrics, "cgroups_cpu_user") > 0);
        assert!(count_name(&metrics, "cgroups_cpu_system") > 0);
    }
}
