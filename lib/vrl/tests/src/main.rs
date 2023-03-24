use vrl_tests::{get_tests_from_functions, run_tests, Test, TestConfig};

use chrono_tz::Tz;
use clap::Parser;
use glob::glob;
use vrl::{CompileConfig, TimeZone, VrlRuntime};

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

fn should_run(name: &str, pat: &Option<String>) -> bool {
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

    run_tests(
        tests,
        &cfg,
        &stdlib::all(),
        || (CompileConfig::default(), ()),
        |_| {},
    );
}

fn get_tests(cmd: &Cmd) -> Vec<Test> {
    glob("tests/**/*.vrl")
        .expect("valid pattern")
        .filter_map(|entry| {
            let path = entry.ok()?;
            Some(Test::from_path(&path))
        })
        .chain(get_tests_from_functions(stdlib::all()))
        .filter(|test| should_run(&format!("{}/{}", test.category, test.name), &cmd.pattern))
        .collect::<Vec<_>>()
}
