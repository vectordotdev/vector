use std::sync::{
    Arc,
    atomic::{self, AtomicBool},
};

use serde_json::json;
use warp::{Rejection, Reply, reply::json};

// Health handler, responds with '{ ok: true }' when running and '{ ok: false}'
// when shutting down
pub(super) async fn health(running: Arc<AtomicBool>) -> Result<impl Reply, Rejection> {
    if running.load(atomic::Ordering::Relaxed) {
        Ok(warp::reply::with_status(
            json(&json!({"ok": true})),
            warp::http::StatusCode::OK,
        ))
    } else {
        Ok(warp::reply::with_status(
            json(&json!({"ok": false})),
            warp::http::StatusCode::SERVICE_UNAVAILABLE,
        ))
    }
}
