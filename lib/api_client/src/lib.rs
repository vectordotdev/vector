mod client;
pub mod gql;
mod subscription;

use anyhow;
use graphql_client::{GraphQLQuery, Response};

pub use client::*;
pub use subscription::*;

pub type Result<T> = anyhow::Result<<T as GraphQLQuery>::ResponseData>;
