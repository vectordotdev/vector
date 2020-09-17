use super::{handler, schema};
use crate::config::api::Options;
use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    QueryBuilder,
};
use async_graphql_warp::{graphql_subscription, GQLResponse};
use std::convert::Infallible;
use tokio::sync::oneshot;
use warp::filters::BoxedFilter;
use warp::{http::Response, Filter, Reply};

pub struct Server {
    _shutdown: oneshot::Sender<()>,
}

impl Server {
    /// Start the API server. This creates the routes and spawns a Warp server. The server is
    /// gracefully shut down when Self falls out of scope by way of the oneshot sender closing
    pub fn start(config: Options) -> Self {
        let bind = config.bind.expect("Invalid socket address");
        let routes = make_routes(config.playground);

        let (_shutdown, rx) = oneshot::channel();
        let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(bind, async {
            rx.await.ok();
        });

        tokio::spawn(server);

        Self { _shutdown }
    }
}

fn make_routes(playground: bool) -> BoxedFilter<(impl Reply,)> {
    // Build the GraphQL schema
    let schema = schema::build_schema().finish();

    // Routes...

    // Health
    let health = warp::path("health").and_then(handler::health);

    // 404
    let not_found = warp::any().and_then(|| async { Err(warp::reject::not_found()) });

    // GraphQL query and subscription handler
    let graphql_handler = warp::path("graphql").and(graphql_subscription(schema.clone()).or(
        async_graphql_warp::graphql(schema).and_then(
            |(schema, builder): (_, QueryBuilder)| async move {
                let resp = builder.execute(&schema).await;
                Ok::<_, Infallible>(GQLResponse::from(resp))
            },
        ),
    ));

    // GraphQL playground
    let graphql_playground = if playground {
        warp::path("playground")
            .map(move || {
                Response::builder()
                    .header("content-type", "text/html")
                    .body(playground_source(
                        GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/graphql"),
                    ))
            })
            .boxed()
    } else {
        not_found.boxed()
    };

    health
        .or(graphql_handler)
        .or(graphql_playground)
        .or(not_found)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_headers(vec![
                    "User-Agent",
                    "Sec-Fetch-Mode",
                    "Referer",
                    "Origin",
                    "Access-Control-Request-Method",
                    "Access-Control-Allow-Origin",
                    "Access-Control-Request-Headers",
                    "Content-Type",
                    "X-Apollo-Tracing", // for Apollo GraphQL clients
                    "Pragma",
                    "Host",
                    "Connection",
                    "Cache-Control",
                ])
                .allow_methods(vec!["POST", "GET"]),
        )
        .boxed()
}
