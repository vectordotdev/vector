#![allow(clippy::doc_overindented_list_items)] // The generated code has this minor issue.
include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));

impl warp::reject::Reject for Status {}
