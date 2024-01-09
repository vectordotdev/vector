use std::sync::{
    atomic::{self, AtomicBool},
    Arc,
};

use serde_json::json;
use warp::{reply::json, Rejection, Reply};

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

#[cfg(not(feature = "jemalloc_pprof"))]
pub(super) async fn heap_profile() -> Result<impl Reply, Rejection> {
    Ok(warp::reply::with_status(
        "heap profiling not availale",
        warp::http::StatusCode::FORBIDDEN,
    ))
}

#[cfg(feature = "jemalloc_pprof")]
pub(super) async fn heap_profile() -> Result<impl Reply, Rejection> {
    let mut prof_ctl = jemalloc_pprof::PROF_CTL.as_ref().unwrap().lock().await;
    require_profiling_activated(&prof_ctl)?;
    let pprof = prof_ctl.dump_pprof().map_err(|err| {
        warp::reply::with_status(err.to_string(), warp::http::StatusCode::FORBIDDEN)
    })?;
    Ok(pprof)
}

/// Checks whether jemalloc profiling is activated an returns an error response if not.
#[cfg(feature = "jemalloc_pprof")]
fn require_profiling_activated(
    prof_ctl: &jemalloc_pprof::JemallocProfCtl,
) -> Result<(), Rejection> {
    if prof_ctl.activated() {
        Ok(())
    } else {
        Err(warp::reply::with_status(
            "heap profiling not activated",
            warp::http::StatusCode::FORBIDDEN,
        ))
    }
}
