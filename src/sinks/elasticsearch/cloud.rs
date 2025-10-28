use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum CloudIdError {
    #[snafu(display("Invalid cloud_id format: expected 'label:base64data', got '{}'", cloud_id))]
    InvalidFormat { cloud_id: String },

    #[snafu(display("Failed to base64 decode cloud_id: {}", source))]
    Base64Decode {
        source: base64::DecodeError,
    },

    #[snafu(display("Cloud_id contains invalid UTF-8: {}", source))]
    InvalidUtf8 {
        source: std::string::FromUtf8Error,
    },

    #[snafu(display(
        "Invalid cloud_id components: expected 'domain$es_uuid$kibana_uuid', got '{}'",
        decoded
    ))]
    InvalidComponents { decoded: String },

    #[snafu(display("Invalid port in cloud_id: {}", source))]
    InvalidPort {
        source: std::num::ParseIntError,
    },

    #[snafu(display("Invalid cloud auth format: {}", message))]
    InvalidCloudAuth { message: String },
}

/// Decoded cloud configuration from cloud_id
#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub elasticsearch_url: String,
    pub kibana_url: String,
}

impl CloudConfig {
    /// Decode Elastic Cloud ID into Elasticsearch and Kibana URLs
    ///
    /// Cloud ID format: "label:base64(domain$es_uuid$kibana_uuid)"
    /// or with ports: "label:base64(domain:port$es_uuid:port$kibana_uuid:port)"
    ///
    /// Examples:
    /// - "my-deployment:dXMtZWFzdC0xLmF3cy5mb3VuZC5pbyRjZWM2ZjI2MWE3NGJmMjRjZTMzYmI4ODExYjg0Mjk0ZiQ="
    ///   -> ES: https://cec6f261a74bf24ce33bb8811b84294f.us-east-1.aws.found.io:443
    pub fn decode(cloud_id: &str) -> Result<Self, CloudIdError> {
        // Split on last colon to separate label from base64 data
        let base64_part = cloud_id
            .rsplit_once(':')
            .map(|(_, b64)| b64)
            .unwrap_or(cloud_id);

        // Base64 decode
        let decoded_bytes = BASE64_STANDARD
            .decode(base64_part)
            .map_err(|e| CloudIdError::Base64Decode { source: e })?;

        let decoded = String::from_utf8(decoded_bytes)
            .map_err(|e| CloudIdError::InvalidUtf8 { source: e })?;

        // Parse components: domain$es_uuid$kibana_uuid
        let parts: Vec<&str> = decoded.split('$').collect();
        if parts.len() < 2 {
            return Err(CloudIdError::InvalidComponents {
                decoded: decoded.clone(),
            });
        }

        let domain_part = parts[0];
        let es_part = parts[1];
        let kibana_part = parts.get(2).unwrap_or(&es_part); // Kibana optional, defaults to ES

        // Parse domain and optional port (format: "domain:port" or "domain")
        let (domain, default_port) = Self::parse_host_port(domain_part, "443")?;

        // Parse ES UUID and optional port
        let (es_uuid, es_port) = Self::parse_host_port(es_part, default_port)?;

        // Parse Kibana UUID and optional port
        let (kibana_uuid, kibana_port) = Self::parse_host_port(kibana_part, default_port)?;

        Ok(CloudConfig {
            elasticsearch_url: format!("https://{}.{}:{}", es_uuid, domain, es_port),
            kibana_url: format!("https://{}.{}:{}", kibana_uuid, domain, kibana_port),
        })
    }

    /// Parse a component into host and port parts
    /// Returns (host, port) where port is either explicitly specified or the default
    fn parse_host_port<'a>(
        component: &'a str,
        default_port: &str,
    ) -> Result<(&'a str, String), CloudIdError> {
        match component.rsplit_once(':') {
            Some((host, port_str)) => {
                // Validate port is numeric
                port_str
                    .parse::<u16>()
                    .map_err(|e| CloudIdError::InvalidPort { source: e })?;
                Ok((host, port_str.to_string()))
            }
            None => Ok((component, default_port.to_string())),
        }
    }
}

/// Parse Elastic Cloud authentication credentials
///
/// Format: "username:password"
/// This matches the Elastic Beats cloud.auth format
pub fn parse_cloud_auth(credentials: &str) -> Result<(String, String), CloudIdError> {
    let (username, password) = credentials
        .split_once(':')
        .ok_or_else(|| CloudIdError::InvalidCloudAuth {
            message: "cloud auth must be in format 'username:password'".to_string(),
        })?;

    if username.is_empty() {
        return Err(CloudIdError::InvalidCloudAuth {
            message: "username cannot be empty".to_string(),
        });
    }

    if password.is_empty() {
        return Err(CloudIdError::InvalidCloudAuth {
            message: "password cannot be empty".to_string(),
        });
    }

    Ok((username.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_cloud_id_basic() {
        // Cloud ID from Filebeat tests
        let cloud_id = "staging:dXMtZWFzdC0xLmF3cy5mb3VuZC5pbyRjZWM2ZjI2MWE3NGJmMjRjZTMzYmI4ODExYjg0Mjk0ZiRjNmMyY2E2ZDA0MjI0OWFmMGNjN2Q3YTllOTYyNTc0Mw==";

        let config = CloudConfig::decode(cloud_id).unwrap();

        assert_eq!(
            config.elasticsearch_url,
            "https://cec6f261a74bf24ce33bb8811b84294f.us-east-1.aws.found.io:443"
        );
        assert_eq!(
            config.kibana_url,
            "https://c6c2ca6d042249af0cc7d7a9e9625743.us-east-1.aws.found.io:443"
        );
    }

    #[test]
    fn test_decode_cloud_id_custom_port() {
        // Cloud ID with custom port
        let cloud_id = "custom-port:dXMtY2VudHJhbDEuZ2NwLmNsb3VkLmVzLmlvOjkyNDMkYWMzMWViYjkwMjQxNzczMTU3MDQzYzM0ZmQyNmZkNDYkYTRjMDYyMzBlNDhjOGZjZTdiZTg4YTA3NGEzYmIzZTA=";

        let config = CloudConfig::decode(cloud_id).unwrap();

        assert_eq!(
            config.elasticsearch_url,
            "https://ac31ebb90241773157043c34fd26fd46.us-central1.gcp.cloud.es.io:9243"
        );
        assert_eq!(
            config.kibana_url,
            "https://a4c06230e48c8fce7be88a074a3bb3e0.us-central1.gcp.cloud.es.io:9243"
        );
    }

    #[test]
    fn test_decode_cloud_id_no_label() {
        // Cloud ID without label
        let cloud_id = "dXMtZWFzdC0xLmF3cy5mb3VuZC5pbyRjZWM2ZjI2MWE3NGJmMjRjZTMzYmI4ODExYjg0Mjk0ZiQ=";

        let config = CloudConfig::decode(cloud_id).unwrap();

        assert_eq!(
            config.elasticsearch_url,
            "https://cec6f261a74bf24ce33bb8811b84294f.us-east-1.aws.found.io:443"
        );
    }

    #[test]
    fn test_decode_cloud_id_invalid_base64() {
        let cloud_id = "invalid:not-valid-base64!!!";
        assert!(CloudConfig::decode(cloud_id).is_err());
    }

    #[test]
    fn test_decode_cloud_id_invalid_format() {
        // Valid base64 but invalid format (missing $ separator)
        let cloud_id = "test:aGVsbG93b3JsZA=="; // "helloworld"
        assert!(CloudConfig::decode(cloud_id).is_err());
    }

    #[test]
    fn test_parse_cloud_auth_valid() {
        let (user, pass) = parse_cloud_auth("elastic:my-secret-password").unwrap();
        assert_eq!(user, "elastic");
        assert_eq!(pass, "my-secret-password");
    }

    #[test]
    fn test_parse_cloud_auth_with_colon_in_password() {
        let (user, pass) = parse_cloud_auth("elastic:pass:with:colons").unwrap();
        assert_eq!(user, "elastic");
        assert_eq!(pass, "pass:with:colons");
    }

    #[test]
    fn test_parse_cloud_auth_invalid_no_colon() {
        assert!(parse_cloud_auth("elasticpassword").is_err());
    }

    #[test]
    fn test_parse_cloud_auth_empty_username() {
        assert!(parse_cloud_auth(":password").is_err());
    }

    #[test]
    fn test_parse_cloud_auth_empty_password() {
        assert!(parse_cloud_auth("elastic:").is_err());
    }
}
