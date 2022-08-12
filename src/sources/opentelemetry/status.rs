include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));

impl warp::reject::Reject for Status {}
