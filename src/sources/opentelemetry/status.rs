#![allow(clippy::doc_overindented_list_items)] // The generated code has this minor issue.
#![expect(
    unreachable_pub,
    reason = "tonic-build generates `pub` items and has no option to narrow their visibility"
)]
include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));

impl warp::reject::Reject for Status {}
