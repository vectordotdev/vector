#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests

mod docs;
mod test_enrichment;

use std::env;
use std::path::PathBuf;
use vrl::test::{get_tests_from_functions, run_tests, Test, TestConfig};

use chrono_tz::Tz;
use clap::Parser;
use glob::glob;
use vrl::compiler::{CompileConfig, TimeZone, VrlRuntime};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser, Debug)]
#[clap(name = "VRL Tests", about = "Vector Remap Language Tests")]
pub struct Cmd {
    #[clap(short, long)]
    pattern: Option<String>,

    #[clap(short, long)]
    fail_early: bool,

    #[clap(short, long)]
    verbose: bool,

    #[clap(short, long)]
    no_diff: bool,

    /// When enabled, any log output at the INFO or above level is printed
    /// during the test run.
    #[clap(short, long)]
    logging: bool,

    /// When enabled, show run duration for each individual test.
    #[clap(short, long)]
    timings: bool,

    #[clap(short = 'z', long)]
    timezone: Option<String>,

    /// Should we use the VM to evaluate the VRL
    #[clap(short, long = "runtime", default_value_t)]
    runtime: VrlRuntime,

    /// Ignore the Cue tests (to speed up run)
    #[clap(long)]
    ignore_cue: bool,
}

impl Cmd {
    fn timezone(&self) -> TimeZone {
        if let Some(ref tz) = self.timezone {
            TimeZone::parse(tz).unwrap_or_else(|| panic!("couldn't parse timezone: {}", tz))
        } else {
            TimeZone::Named(Tz::UTC)
        }
    }
}

fn should_run(name: &str, pat: &Option<String>, _runtime: VrlRuntime) -> bool {
    if name == "tests/example.vrl" {
        return false;
    }

    if let Some(pat) = pat {
        if !name.contains(pat) {
            return false;
        }
    }

    true
}

fn main() {
    let cmd = Cmd::parse();

    if cmd.logging {
        tracing_subscriber::fmt::init();
    }

    let tests = get_tests(&cmd);

    let cfg = TestConfig {
        fail_early: cmd.fail_early,
        verbose: cmd.verbose,
        no_diff: cmd.no_diff,
        timings: cmd.timings,
        runtime: cmd.runtime,
        timezone: cmd.timezone(),
    };

    let mut functions = vrl::stdlib::all();
    functions.extend(vector_vrl_functions::all());
    functions.extend(enrichment::vrl_functions());
    functions.extend(vrl_cache::vrl_functions());

    run_tests(
        tests,
        &cfg,
        &functions,
        || {
            let mut config = CompileConfig::default();
            let enrichment_table = test_enrichment::test_enrichment_table();
            config.set_custom(enrichment_table.clone());
            (config, enrichment_table)
        },
        |registry| registry.finish_load(),
    );
}

pub fn test_dir() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
}

fn test_glob_pattern() -> String {
    test_dir().join("**/*.vrl").to_str().unwrap().to_string()
}
fn get_tests(cmd: &Cmd) -> Vec<Test> {
    glob(test_glob_pattern().as_str())
        .expect("valid pattern")
        .filter_map(|entry| {
            let path = entry.ok()?;
            Some(Test::from_path(&path))
        })
        .chain(docs::tests(cmd.ignore_cue))
        .chain(get_tests_from_functions(
            vector_vrl_functions::all()
                .into_iter()
                .chain(enrichment::vrl_functions())
                .chain(vrl_cache::vrl_functions())
                .collect(),
        ))
        .filter(|test| {
            should_run(
                &format!("{}/{}", test.category, test.name),
                &cmd.pattern,
                cmd.runtime,
            )
        })
        .collect::<Vec<_>>()
}
