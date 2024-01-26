#![deny(warnings)]

extern crate vector;
use vector::{app::Application, extra_context::ExtraContext};

use std::process::ExitCode;

#[cfg(unix)]
fn main() -> ExitCode {
    #[cfg(feature = "allocation-tracing")]
    {
        use crate::vector::internal_telemetry::allocations::{
            init_allocation_tracing, REPORTING_INTERVAL_MS, TRACK_ALLOCATIONS,
        };
        use std::sync::atomic::Ordering;
        let opts = vector::cli::Opts::get_matches()
            .map_err(|error| {
                // Printing to stdout/err can itself fail; ignore it.
                _ = error.print();
                exitcode::USAGE
            })
            .unwrap_or_else(|code| {
                std::process::exit(code);
            });
        let allocation_tracing = opts.root.allocation_tracing;
        REPORTING_INTERVAL_MS.store(
            opts.root.allocation_tracing_reporting_interval_ms,
            Ordering::Relaxed,
        );
        drop(opts);
        // At this point, we make the following assumption:
        // The heap does not contain any allocations that have a shorter lifetime than the program.
        if allocation_tracing {
            // Start tracking allocations
            TRACK_ALLOCATIONS.store(true, Ordering::Relaxed);
            init_allocation_tracing();
        }
    }

    let exit_code = Application::run(ExtraContext::default())
        .code()
        .unwrap_or(exitcode::UNAVAILABLE) as u8;
    ExitCode::from(exit_code)
}

#[cfg(windows)]
pub fn main() -> ExitCode {
    // We need to be able to run vector in User Interactive mode. We first try
    // to run vector as a service. If we fail, we consider that we are in
    // interactive mode and then fallback to console mode.  See
    // https://docs.microsoft.com/en-us/dotnet/api/system.environment.userinteractive?redirectedfrom=MSDN&view=netcore-3.1#System_Environment_UserInteractive
    let exit_code = vector::vector_windows::run().unwrap_or_else(|_| {
        Application::run(ExtraContext::default())
            .code()
            .unwrap_or(exitcode::UNAVAILABLE)
    });
    ExitCode::from(exit_code as u8)
}
