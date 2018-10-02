extern crate router;

#[macro_use]
extern crate log;
extern crate fern;

fn main() {
    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .unwrap();

    info!("Hello, world!");
}
