use crate::{api, config};
use graphql_client::GraphQLQuery;
use prettytable::{format, Table};
use reqwest;
use std::net::SocketAddr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// How often the screen refreshes (in milliseconds)
    #[structopt(default_value = "500", short = "i", long)]
    refresh_interval: i32,

    #[structopt(short, long)]
    remote: Option<SocketAddr>,
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

async fn query<T: GraphQLQuery>(
    addr: SocketAddr,
    request_body: &graphql_client::QueryBody<T::Variables>,
) -> Result<graphql_client::Response<T::ResponseData>, reqwest::Error> {
    let url = format!("http://{}:{}/graphql", addr.ip(), addr.port());
    let client = reqwest::Client::new();

    client
        .post(&url)
        .json(&request_body)
        .send()
        .await?
        .json()
        .await
}

fn topology_type(topology_on: topology_query::TopologyQueryTopologyOn) -> &'static str {
    match topology_on {
        topology_query::TopologyQueryTopologyOn::Source => "source",
        topology_query::TopologyQueryTopologyOn::Transform => "transform",
        topology_query::TopologyQueryTopologyOn::Sink => "sink",
    }
}

async fn healthcheck(addr: SocketAddr) -> Result<bool, ()> {
    let request_body = HealthQuery::build_query(health_query::Variables);
    let res = query::<HealthQuery>(addr, &request_body).await.m;

    res.
}

async fn get_topology(addr: SocketAddr) -> exitcode::ExitCode {
    let request_body = TopologyQuery::build_query(topology_query::Variables);
    let res = match query::<TopologyQuery>(addr, &request_body).await {
        Ok(res) => res,
        _ => {
            eprintln!("Vector GraphQL query failed");
            return exitcode::UNAVAILABLE;
        }
    };

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!("NAME", "TYPE"));

    for data in res.data.unwrap().topology {
        table.add_row(row!(data.name, topology_type(data.on)));
    }

    table.printstd();

    exitcode::OK
}

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let addr = opts.remote.or_else(|| config::api::default_bind());

    match config::load_from_paths(&opts.paths) {
        Ok(config) => match (opts.remote.is_some(), config.api.enabled) {
            // No remote; API not enabled locally
            (false, false) => {
                println!("To view topology, api.enabled must be set to `true`, or an explicit --remote provided.");
                exitcode::CONFIG
            }

            // No remote; API is enabled
            (false, true) => {
                let server = api::Server::start(&config);
                get_topology(server.addr()).await
            }

            // Remote
            (true, _) => get_topology(opts.remote.unwrap()).await,
        },
        _ => exitcode::CONFIG,
    }
}
