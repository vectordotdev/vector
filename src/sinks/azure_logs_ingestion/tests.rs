
use super::config::AzureLogsIngestionConfig;


#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureLogsIngestionConfig>();
}

#[test]
fn basic_config_with_client_credentials() {
    let config: AzureLogsIngestionConfig = toml::from_str::<AzureLogsIngestionConfig>(
        r#"
            endpoint = "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"
            dcr_immutable_id = "dcr-00000000000000000000000000000000"
            stream_name = "Custom-UnitTest"
            
            [auth]
            azure_tenant_id = "00000000-0000-0000-0000-000000000000"
            azure_client_id = "mock-client-id"
            azure_client_secret = "mock-client-secret"
        "#)
        .expect("Config parsing failed");
    
    assert_eq!(config.endpoint, "https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com");
    assert_eq!(config.dcr_immutable_id, "dcr-00000000000000000000000000000000");
    assert_eq!(config.stream_name, "Custom-UnitTest");
    assert_eq!(config.token_scope, "https://monitor.azure.com/.default");
    assert_eq!(config.timestamp_field, "TimeGenerated");

    match &config.auth {
        crate::sinks::azure_logs_ingestion::config::AzureAuthentication::ClientSecretCredential {
            azure_tenant_id,
            azure_client_id,
            azure_client_secret,
        } => {
            assert_eq!(azure_tenant_id, "00000000-0000-0000-0000-000000000000");
            assert_eq!(azure_client_id, "mock-client-id");
            let secret: String = azure_client_secret.inner().into();
            assert_eq!(secret, "mock-client-secret");
        }
        _ => panic!("Expected ClientSecretCredential variant"),
    }
}

// TODO test config with ManagedIdentity (will need to mock env vars...)
