#![deny(warnings)]

extern crate vector;
use vector::app::Application;

#[cfg(unix)]
fn main() {
    #[cfg(feature = "allocation-tracing")]
    {
        use crate::vector::internal_telemetry::allocations::{
            init_allocation_tracing_reporter, REPORTING_INTERVAL_MS, TRACE_ALLOCATIONS,
        };
        use std::sync::atomic::Ordering;
        let opts = vector::cli::Opts::get_matches()
            .map_err(|error| {
                // Printing to stdout/err can itself fail; ignore it.
                let _ = error.print();
                exitcode::USAGE
            })
            .unwrap_or_else(|code| {
                std::process::exit(code);
            });
        let enable_allocation_tracing = opts.root.allocation_tracing;
        let reporter_interval_ms = opts.root.allocation_tracing_reporting_interval_ms;
        drop(opts);

        // At this point, we're making the assumption that no other heap allocations exist/are live,
        // since we've dropped everything related to parsing the command-line arguments. This is our
        // invariant for knowing that if we turn on allocation tracing, no previous allocations
        // exist where, when deallocated, we'd try to extract the wrapper trailer reference to the
        // source allocation group and trigger instant UB.
        if enable_allocation_tracing {
            // Start tracing allocations and configure the reporting interval for the reporter thread.
            TRACE_ALLOCATIONS.store(true, Ordering::Relaxed);
            REPORTING_INTERVAL_MS.store(reporter_interval_ms, Ordering::Relaxed);
            init_allocation_tracing_reporter();
        }
    }

    let app = Application::prepare().unwrap_or_else(|code| {
        std::process::exit(code);
    });

    app.run();
}

#[cfg(windows)]
pub fn main() {
    // We need to be able to run vector in User Interactive mode. We first try
    // to run vector as a service. If we fail, we consider that we are in
    // interactive mode and then fallback to console mode.  See
    // https://docs.microsoft.com/en-us/dotnet/api/system.environment.userinteractive?redirectedfrom=MSDN&view=netcore-3.1#System_Environment_UserInteractive
    vector::vector_windows::run().unwrap_or_else(|_| {
        let app = Application::prepare().unwrap_or_else(|code| {
            std::process::exit(code);
        });

        app.run();
    });
}
