use vrl_server::server::serve;

#[tokio::main]
async fn main() {
    serve().await
}
