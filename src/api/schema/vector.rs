use async_graphql::Object;

#[derive(Default)]
pub struct VectorQuery;

#[Object]
impl VectorQuery {
    /// Vector version
    async fn version(&self) -> String {
        crate::get_version()
    }

    /// Hostname
    async fn hostname(&self) -> async_graphql::Result<String> {
        crate::get_hostname().map_err(|_| async_graphql::Error::new("Couldn't get hostname"))
    }
}
