use serde::Serialize;
use warp::reply::json;
use warp::{Rejection, Reply};

#[derive(Serialize)]
struct Health {
    ok: bool,
}

// health handler, responds with { ok: true }
pub async fn health() -> Result<impl Reply, Rejection> {
    Ok(json(&Health { ok: true }))
}
