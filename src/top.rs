use crate::{
    api_client::{make_subscription_client, query},
    config,
};
use graphql_client::GraphQLQuery;
use human_format;
use prettytable::{format, Table};
use reqwest;
use structopt::StructOpt;
use url::Url;

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
    humanize: bool,
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/health.graphql",
    response_derives = "Debug"
)]
struct HealthQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/queries/topology.graphql",
    response_derives = "Debug"
)]
struct TopologyQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.json",
    query_path = "graphql/subscriptions/uptime_metrics.graphql",
    response_derives = "Debug"
)]
struct UptimeMetricsSubscription;

fn topology_type(topology_on: topology_query::TopologyQueryTopologyOn) -> &'static str {
    match topology_on {
        topology_query::TopologyQueryTopologyOn::Source => "source",
        topology_query::TopologyQueryTopologyOn::Transform => "transform",
        topology_query::TopologyQueryTopologyOn::Sink => "sink",
    }
}

async fn healthcheck(url: &Url) -> Result<bool, ()> {
    let request_body = HealthQuery::build_query(health_query::Variables);
    let res = query::<HealthQuery>(url, &request_body)
        .await
        .map_err(|_| ())?;

    // Health (currently) always returns `true`, so there should be no instance where
    // a server is both accessible and also returns `false`. However, this may change in the
    // future, where the health is a more inclusive indicator of overall topological health,
    // so I think this is worth leaving in.
    match res.data.ok_or(())?.health {
        true => Ok(true),
        false => Err(()),
    }
}

async fn print_topology(
    url: &Url,
    mut formatter: Box<dyn StatsWriter>,
) -> Result<(), reqwest::Error> {
    let request_body = TopologyQuery::build_query(topology_query::Variables);
    let res = query::<TopologyQuery>(url, &request_body).await?;

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!("NAME", "TYPE", r->"EVENTS"));

    for data in res.data.unwrap().topology {
        table.add_row(row!(
            data.name,
            topology_type(data.on),
            r->formatter.kb(data
                .events_processed
                .map(|ep| ep.events_processed)
                .unwrap_or(0.00))));
    }

    table.printstd();

    Ok(())
}

// async fn metrics(url: &Url) -> Result<(), ()> {
//     let client = make_subscription_client(&url).await.map_err(|_| ())?;
//
//     let request_body =
//         UptimeMetricsSubscription::build_query(uptime_metrics_subscription::Variables);
//
//     let subscription = client
//         .start::<UptimeMetricsSubscription>(&request_body)
//         .await
//         .map_err(|_| ())?;
//
//     for data in subscription.stream().iter() {
//         println!("{:?}", data)
//     }
//
//     Ok(())
// }

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let url = opts.url.clone().unwrap_or_else(|| {
        let addr = config::api::default_bind().unwrap();
        Url::parse(&*format!("http://{}/graphql", addr)).unwrap()
    });

    // Check that the GraphQL server is reachable
    match healthcheck(&url).await {
        Ok(t) if t => (),
        _ => {
            eprintln!("Vector API server not reachable");
            return exitcode::UNAVAILABLE;
        }
    }

    if print_topology(&url, new_formatter(opts.humanize))
        .await
        .is_err()
    {
        eprintln!("Couldn't retrieve topology");
        return exitcode::UNAVAILABLE;
    }

    exitcode::OK
}
