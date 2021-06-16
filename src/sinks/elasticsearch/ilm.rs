use super::ElasticSearchCommon;
use crate::http::HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexLifecycleManagementConfig {
    enabled: Option<bool>,
    #[serde(default = "IndexLifecycleManagementConfig::default_rollover_alias")]
    rollover_alias: String,
    #[serde(default = "IndexLifecycleManagementConfig::default_pattern")]
    pattern: String,
    #[serde(default = "IndexLifecycleManagementConfig::default_policy")]
    policy: String,
}

impl Default for IndexLifecycleManagementConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            rollover_alias: Self::default_rollover_alias(),
            pattern: Self::default_pattern(),
            policy: Self::default_policy(),
        }
    }
}

impl IndexLifecycleManagementConfig {
    fn default_rollover_alias() -> String {
        "hello".into()
    }
    fn default_pattern() -> String {
        "hello".into()
    }
    fn default_policy() -> String {
        "hello".into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XPackFeature {
    available: bool,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XPackResponse {
    features: HashMap<String, XPackFeature>,
}

impl ElasticSearchCommon {
    async fn get_xpack_features(
        &self,
        client: &HttpClient,
    ) -> crate::Result<HashMap<String, XPackFeature>> {
        let url = format!("{}/_xpack", self.base_url);
        let response = self.execute_get_request(client, url).await?;

        let body = response.into_body();
        let bytes = hyper::body::to_bytes(body).await?;
        let body: XPackResponse = serde_json::from_slice(bytes.as_ref())?;

        Ok(body.features)
    }
}
