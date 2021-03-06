use anyhow::Result;
use structopt::StructOpt;
use warp::Filter;

#[derive(Debug, thiserror::Error)]
enum Error {}

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(short = "p", long, default_value = "8080", env = "PORT")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::from_args();

    let hello = warp::path!("hello" / String)
        .map(|name| format!("Hello, {}!", name));

    warp::serve(hello)
        .run(([127, 0, 0, 1], opts.port))
        .await;

    Ok(())
}
