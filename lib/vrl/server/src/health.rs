use serde::Serialize;
use std::convert::Infallible;
use warp::{reply::json, Reply};

#[derive(Serialize)]
struct Health {
    ok: bool,
}

pub async fn healthy() -> Result<impl Reply, Infallible> {
    let healthy = Health { ok: true };

    Ok(json(&healthy))
}
