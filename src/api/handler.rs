use serde::Serialize;
use warp::reply::json;
use warp::{Rejection, Reply};

static OK: bool = true;

#[derive(Serialize)]
struct Health {
    ok: bool,
}

impl Health {
    pub fn new() -> Health {
        Health { ok: OK }
    }
}

// health handler, responds with { ok: true }
pub async fn health() -> Result<impl Reply, Rejection> {
    Ok(json(&Health::new()))
}
