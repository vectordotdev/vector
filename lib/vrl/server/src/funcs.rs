use serde::Serialize;
use std::convert::Infallible;
use warp::{reply::json, Reply};

#[derive(Serialize)]
struct FunctionList {
    functions: Vec<&'static str>,
}

pub async fn function_metadata() -> Result<impl Reply, Infallible> {
    let functions: Vec<&'static str> = stdlib::all().iter().map(|f| f.identifier()).collect();
    let functions_list = FunctionList { functions };

    Ok(json(&functions_list))
}
