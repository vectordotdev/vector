use crate::config;
use prettytable::{format, Table};
use structopt::StructOpt;
use url::Url;
use vector_api_client::{
    gql::{HealthQueryExt, TopologyQueryExt},
    Client,
};

trait StatsWriter {
    fn kb(&mut self, n: f64) -> String;
}

struct HumanWriter {
    f: human_format::Formatter,
}

impl HumanWriter {
    fn new() -> Self {
        Self {
            f: human_format::Formatter::new(),
        }
    }
}

impl StatsWriter for HumanWriter {
    fn kb(&mut self, n: f64) -> String {
        self.f.with_decimals(2).format(n)
    }
}

struct LocaleWriter {
    buf: num_format::Buffer,
}

impl LocaleWriter {
    fn new() -> Self {
        Self {
            buf: num_format::Buffer::new(),
        }
    }
}

impl StatsWriter for LocaleWriter {
    fn kb(&mut self, n: f64) -> String {
        self.buf
            .write_formatted(&(n as i64), &num_format::Locale::en);
        self.buf.to_string()
    }
}

fn new_formatter(humanize: bool) -> Box<dyn StatsWriter> {
    if humanize {
        Box::new(HumanWriter::new())
    } else {
        Box::new(LocaleWriter::new())
    }
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// How often the screen refreshes (in milliseconds)
    #[structopt(default_value = "500", short = "i", long)]
    refresh_interval: i32,

    #[structopt(short, long)]
    url: Option<Url>,

    #[structopt(short, long)]
    human: bool,
}

async fn print_topology(client: &Client, mut formatter: Box<dyn StatsWriter>) -> Result<(), ()> {
    let res = client.topology_query().await.map_err(|_| ())?;

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!("NAME", "TYPE", r->"EVENTS"));

    for data in res.data.unwrap().topology {
        table.add_row(row!(
            data.name,
            data.on.to_string(),
            r->formatter.kb(data
                .events_processed_total
                .map(|ep| ep.events_processed_total)
                .unwrap_or(0.00))));
    }

    table.printstd();

    Ok(())
}

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_bind().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr)).unwrap()
    });

    let client = Client::new(url);

    // Check that the GraphQL server is reachable
    match client.health_query().await {
        Ok(_) => (),
        _ => {
            eprintln!("Vector API server not reachable");
            return exitcode::UNAVAILABLE;
        }
    }

    // Print initial topology
    // TODO - make this auto-update!
    if print_topology(&client, new_formatter(opts.human))
        .await
        .is_err()
    {
        eprintln!("Couldn't retrieve topology");
        return exitcode::UNAVAILABLE;
    }

    exitcode::OK
}
