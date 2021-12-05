extern crate vector;
use vector::app::Application;

#[cfg(unix)]
fn main() {
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
