use crate::config;
use prettytable::{format, Table};
use std::collections::HashMap;
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

/// Row type containing individual metrics stats
pub struct TopologyRow {
    topology_type: String,
    events_processed: f64,
}

type TopologyTable = HashMap<String, TopologyRow>;

/// (Re)draw the table with the latest stats
fn draw_table(t: &TopologyTable, mut formatter: Box<dyn StatsWriter>) {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!("NAME", "TYPE", r->"EVENTS"));

    for (name, r) in t.iter() {
        table.add_row(row!(
            name,
            r.topology_type,
            r->formatter.kb(r.events_processed)
        ));
    }

    table.printstd();
}

async fn print_topology(client: &Client, mut formatter: Box<dyn StatsWriter>) -> Result<(), ()> {
    // Get initial topology, including aggregate stats, and build a table of rows
    let rows = client
        .topology_query()
        .await
        .map_err(|_| ())?
        .data
        .ok_or_else(|| ())?
        .iter()
        .map(|d| {
            (
                d.name,
                TopologyRow {
                    topology_type: d.on.to_string(),
                    events_processed: d
                        .events_processed
                        .map(|ep| ep.events_processed)
                        .unwrap_or(0.00),
                },
            )
        })
        .collect::<TopologyTable>();

    draw_table(rows, formatter);

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
