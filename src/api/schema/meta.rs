use async_graphql::Object;

#[derive(Default)]
pub struct Meta;

#[Object]
impl Meta {
    /// Vector version
    async fn version_string(&self) -> String {
        crate::get_version()
    }

    /// Hostname
    async fn hostname(&self) -> Option<String> {
        crate::get_hostname().ok()
    }
}

#[derive(Default)]
pub struct MetaQuery;

#[Object]
impl MetaQuery {
    async fn meta(&self) -> Meta {
        Meta
    }
}
