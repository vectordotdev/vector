use clap::{App, Arg};
use log::error;
use router::app::Error;

fn main() {
    router::setup_logger();

    let app = App::new("Router").version("1.0").author("timber.io")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .required(true)
                .takes_value(true),
        ).arg(
            Arg::with_name("require-healthy")
                .short("r")
                .long("require-healthy")
                .help("Causes router to immediate exit on startup if any sinks having failing healthchecks")
        );
    let matches = app.get_matches();

    let config = matches.value_of("config").unwrap();

    let file = match std::fs::File::open(config) {
        Ok(file) => file,
        Err(e) => {
            error!("Unable to open the config file: {}", e);
            std::process::exit(1);
        }
    };

    let config = match router::topology::Config::load(file) {
        Ok(c) => c,
        Err(errs) => {
            error!("Unable to parse config file");

            for e in errs {
                error!("{}", e);
            }

            std::process::exit(1);
        }
    };

    if let Err(err) = router::app::init(config, matches.is_present("require-healthy")) {
        match err {
            Error::Unhealthy => {
                error!("Sinks unhealthy; shutting down");
                std::process::exit(1);
            }
            Error::Config(errors) => {
                for error in errors {
                    error!("Configuration error: {}", error);
                }
                std::process::exit(1);
            }
        }
    }
}
