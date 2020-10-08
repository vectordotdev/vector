use crate::{
    api_client::{make_subscription_client, query},
    config,
};
use graphql_client::GraphQLQuery;
use prettytable::{format, Table};
use reqwest;
use structopt::StructOpt;
use url::Url;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// How often the screen refreshes (in milliseconds)
    #[structopt(default_value = "500", short = "i", long)]
    refresh_interval: i32,

    #[structopt(short, long)]
    remote: Option<Url>,
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

async fn print_topology(url: &Url) -> Result<(), reqwest::Error> {
    let request_body = TopologyQuery::build_query(topology_query::Variables);
    let res = query::<TopologyQuery>(url, &request_body).await?;

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!("NAME", "TYPE"));

    for data in res.data.unwrap().topology {
        table.add_row(row!(data.name, topology_type(data.on)));
    }

    table.printstd();

    Ok(())
}

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let url = opts.remote.clone().unwrap_or_else(|| {
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

    // Print topology
    if print_topology(&url).await.is_err() {
        eprintln!("Couldn't retrieve topology");
        return exitcode::UNAVAILABLE;
    }

    exitcode::OK
}
