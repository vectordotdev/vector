use crate::config::{DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceOutput};
use crate::event::Event;
use crate::tls::TlsSourceConfig;
use crate::sources::Source;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::InternalEvent;
use vector_lib::schema::Definition;

mod parser;
mod template_cache;
mod errors;

pub use parser::*;
pub use template_cache::*;
pub use errors::*;

/// Configuration for the `netflow` source.
#[configurable_component(source("netflow"))]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetflowConfig {
    /// The address to listen for NetFlow packets on.
    #[configurable(metadata(docs::examples = "0.0.0.0:2055"))]
    pub address: SocketAddr,

    /// The maximum size of incoming NetFlow packets.
    #[configurable(metadata(docs::examples = 1500))]
    #[serde(default = "default_max_length")]
    pub max_length: usize,

    /// The maximum length of field values before truncation.
    #[configurable(metadata(docs::examples = 1024))]
    #[serde(default = "default_max_field_length")]
    pub max_field_length: usize,

    /// The maximum number of templates to cache per peer.
    #[configurable(metadata(docs::examples = 1000))]
    #[serde(default = "default_max_templates")]
    pub max_templates: usize,

    /// The timeout for template cache entries in seconds.
    #[configurable(metadata(docs::examples = 3600))]
    #[serde(default = "default_template_timeout")]
    pub template_timeout: u64,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsSourceConfig>,

    /// The namespace for the source. This value is used to namespace the source's metrics.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub namespace: Option<String>,
}

const fn default_max_length() -> usize {
    1500
}

const fn default_max_field_length() -> usize {
    1024
}

const fn default_max_templates() -> usize {
    1000
}

const fn default_template_timeout() -> u64 {
    3600
}

impl GenerateConfig for NetflowConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:2055".parse().unwrap(),
            max_length: default_max_length(),
            max_field_length: default_max_field_length(),
            max_templates: default_max_templates(),
            template_timeout: default_template_timeout(),
            tls: None,
            namespace: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "netflow")]
impl SourceConfig for NetflowConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let address = self.address;
        let max_length = self.max_length;
        let max_field_length = self.max_field_length;
        let max_templates = self.max_templates;
        let template_timeout = self.template_timeout;

        let socket = UdpSocket::bind(address).await?;
        let template_cache = template_cache::new_template_cache(max_templates);

        let out = cx.out;
        let template_cache_clone = template_cache.clone();

        let source = async move {
            let mut buf = vec![0; max_length];
            let mut template_cache = template_cache_clone;

            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, peer_addr)) => {
                        if len > max_length {
                            errors::NetflowParseError {
                                error: "Packet too large",
                                protocol: "unknown",
                                peer_addr,
                            }
                            .emit();
                            continue;
                        }

                        let data = &buf[..len];
                        match parse_flow_data(data, peer_addr, &template_cache, max_field_length) {
                            Ok(events) => {
                                if !events.is_empty() {
                                    if let Err(error) = out.send_batch(events).await {
                                        error!(message = "Error sending events", %error);
                                        break;
                                    }
                                }
                            }
                            Err(error) => {
                                errors::NetflowParseError {
                                    error,
                                    protocol: "unknown",
                                    peer_addr,
                                }
                                .emit();
                            }
                        }
                    }
                    Err(error) => {
                        error!(message = "Error receiving packet", %error);
                        break;
                    }
                }
            }
        };

        // Start template cleanup task
        let template_cache_clone = template_cache.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                cleanup_expired_templates(&template_cache_clone, template_timeout);
            }
        });

        Ok(Box::pin(source))
    }

    fn outputs(&self, _global_log_namespace: vector_lib::config::LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_maybe_logs(DataType::Log, Definition::any())]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::udp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::collect_ready;
    use std::net::UdpSocket as StdUdpSocket;
    use std::time::Duration;
    use template_cache::{Template, TemplateField, cache_put, cache_get, cache_len};

    #[tokio::test]
    async fn test_netflow_v5() {
        let config = NetflowConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            max_length: 1500,
            max_field_length: 1024,
            max_templates: 1000,
            template_timeout: 3600,
            tls: None,
            namespace: None,
        };

        let (tx, rx) = SourceContext::new_test();
        let source = config.build(tx).await.unwrap();
        let _source_task = tokio::spawn(source);

        // Wait for source to start
        sleep(Duration::from_millis(100)).await;

        // Create a simple NetFlow v5 packet
        let mut packet = vec![0u8; 24 + 48]; // Header + 1 record
        
        // Version 5
        packet[0] = 0;
        packet[1] = 5;
        
        // Count = 1
        packet[2] = 0;
        packet[3] = 1;
        
        // Rest of header...
        for i in 4..24 {
            packet[i] = i as u8;
        }
        
        // Record data
        for i in 24..72 {
            packet[i] = i as u8;
        }

        // Send packet
        let socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        socket.send_to(&packet, "127.0.0.1:2055").unwrap();

        // Wait for events
        sleep(Duration::from_millis(100)).await;

        let events = collect_ready(rx).await;
        assert!(!events.is_empty());
        
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("protocol").unwrap().as_str().unwrap(), "netflow_v5");
        }
    }

    #[test]
    fn test_parse_netflow_v5() {
        let mut packet = vec![0u8; 24 + 48]; // Header + 1 record
        
        // Version 5
        packet[0] = 0;
        packet[1] = 5;
        
        // Count = 1
        packet[2] = 0;
        packet[3] = 1;
        
        // Rest of header...
        for i in 4..24 {
            packet[i] = i as u8;
        }
        
        // Record data
        for i in 24..72 {
            packet[i] = i as u8;
        }

        let template_cache = template_cache::new_template_cache(1000);
        let peer_addr = "127.0.0.1:12345".parse().unwrap();
        
        let result = parse_flow_data(&packet, peer_addr, &template_cache, 1024);
        assert!(result.is_ok());
        
        let events = result.unwrap();
        assert_eq!(events.len(), 1);
        
        if let Event::Log(log) = &events[0] {
            assert_eq!(log.get("protocol").unwrap().as_str().unwrap(), "netflow_v5");
            assert_eq!(log.get("peer_addr").unwrap().as_str().unwrap(), "127.0.0.1:12345");
        }
    }

    #[test]
    fn test_parse_ipfix() {
        let mut packet = vec![0u8; 16 + 8 + 4]; // Header + template set header + template
        
        // Version 10 (IPFIX)
        packet[0] = 0;
        packet[1] = 10;
        
        // Length = 28
        packet[2] = 0;
        packet[3] = 28;
        
        // Rest of header...
        for i in 4..16 {
            packet[i] = i as u8;
        }
        
        // Template set (ID = 2)
        packet[16] = 0;
        packet[17] = 2;
        
        // Set length = 8
        packet[18] = 0;
        packet[19] = 8;
        
        // Template ID = 256
        packet[20] = 1;
        packet[21] = 0;
        
        // Field count = 1
        packet[22] = 0;
        packet[23] = 1;
        
        // Field type = 8 (sourceIPv4Address)
        packet[24] = 0;
        packet[25] = 8;
        
        // Field length = 4
        packet[26] = 0;
        packet[27] = 4;

        let template_cache = template_cache::new_template_cache(1000);
        let peer_addr = "127.0.0.1:12345".parse().unwrap();
        
        let result = parse_flow_data(&packet, peer_addr, &template_cache, 1024);
        assert!(result.is_ok());
        
        // Should not produce events for template-only packets
        let events = result.unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_template_cache() {
        let cache = template_cache::new_template_cache(2);
        
        let template = Template {
            template_id: 1,
            fields: vec![TemplateField {
                field_type: 8,
                field_length: 4,
                enterprise_number: None,
            }],
            created: std::time::Instant::now(),
        };
        
        let key = ("127.0.0.1:12345".parse().unwrap(), 0, 1);
        
        // Test put and get
        cache_put(&cache, key, template.clone());
        assert_eq!(cache_len(&cache), 1);
        
        let retrieved = cache_get(&cache, &key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().template_id, 1);
        
        // Test cache size limit
        let template2 = Template {
            template_id: 2,
            fields: vec![],
            created: std::time::Instant::now(),
        };
        let template3 = Template {
            template_id: 3,
            fields: vec![],
            created: std::time::Instant::now(),
        };
        
        let key2 = ("127.0.0.1:12345".parse().unwrap(), 0, 2);
        let key3 = ("127.0.0.1:12345".parse().unwrap(), 0, 3);
        
        cache_put(&cache, key2, template2);
        cache_put(&cache, key3, template3);
        
        // Should have evicted the oldest entry
        assert_eq!(cache_len(&cache), 2);
        assert!(cache_get(&cache, &key).is_none());
    }
}




