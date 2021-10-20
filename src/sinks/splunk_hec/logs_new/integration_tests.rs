#[cfg(all(test, feature = "splunk-integration-tests"))]
mod integration_tests {
    // use super::{self::*};
    use crate::{config::{SinkConfig, SinkContext}, sinks::{
            splunk_hec::{
                logs_new::{config::HecSinkLogsConfig, service::Encoding},
            },
            util::{
                encoding::{EncodingConfig, StandardEncodings},
                BatchConfig, Compression, TowerRequestConfig,
            },
        }, template::Template, test_util::components::{self, HTTP_SINK_TAGS}, test_util::{random_lines_with_stream, random_string}};
    use futures::stream;
    use serde_json::Value as JsonValue;
    use std::convert::TryFrom;
    use std::future::ready;
    use tokio::time::{sleep, Duration};
    use vector_core::event::{BatchNotifier, BatchStatus, Event, LogEvent};
    // use super::*;
    use crate::{assert_downcast_matches, tls::TlsSettings};
    use crate::test_util::retry_until;
    use std::net::SocketAddr;
    use warp::Filter;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    pub async fn get_token() -> String {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let res = retry_until(
            || {
                client
                    .get("https://localhost:8089/services/data/inputs/http?output_mode=json")
                    .basic_auth(USERNAME, Some(PASSWORD))
                    .send()
            },
            Duration::from_millis(500),
            Duration::from_secs(30),
        )
        .await;

        let json: JsonValue = res.json().await.unwrap();
        let entries = json["entry"].as_array().unwrap().clone();

        if entries.is_empty() {
            panic!("You don't have any HTTP Event Collector inputs set up in Splunk");
        }

        entries[0]["content"]["token"].as_str().unwrap().to_owned()
    }

    async fn recent_entries(index: Option<&str>) -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // https://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
        let search_query = match index {
            Some(index) => format!("search index={}", index),
            None => "search *".into(),
        };
        let res = client
            .post("https://localhost:8089/services/search/jobs?output_mode=json")
            .form(&vec![
                ("search", &search_query[..]),
                ("exec_mode", "oneshot"),
                ("f", "*"),
            ])
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .await
            .unwrap();
        let json: JsonValue = res.json().await.unwrap();

        json["results"].as_array().unwrap().clone()
    }

    // It usually takes ~1 second for the event to show up in search, so poll until
    // we see it.
    async fn find_entry(message: &str) -> serde_json::value::Value {
        for _ in 0..20usize {
            match recent_entries(None)
                .await
                .into_iter()
                .find(|entry| entry["_raw"].as_str().unwrap_or("").contains(&message))
            {
                Some(value) => return value,
                None => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        panic!("Didn't find event in Splunk");
    }

    async fn config(
        encoding: impl Into<EncodingConfig<Encoding>>,
        indexed_fields: Vec<String>,
    ) -> HecSinkLogsConfig {
        HecSinkLogsConfig {
            token: get_token().await,
            endpoint: "http://localhost:8088/".into(),
            host_key: "host".into(),
            indexed_fields,
            index: None,
            sourcetype: None,
            source: None,
            encoding: encoding.into(),
            compression: Compression::None,
            batch: BatchConfig {
                max_events: Some(1),
                ..Default::default()
            },
            request: TowerRequestConfig::default(),
            tls: None,
        }
    }

    #[tokio::test]
    async fn splunk_insert_message() {
        let cx = SinkContext::new_test();

        let config = config(Encoding::Text, vec![]).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from(message.clone())
            .with_batch_notifier(&batch)
            .into();
        drop(batch);
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert!(entry.get("message").is_none());
    }

    #[tokio::test]
    async fn splunk_insert_broken_token() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.token = "BROKEN_TOKEN".into();
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event = LogEvent::from(message.clone())
            .with_batch_notifier(&batch)
            .into();
        drop(batch);
        sink.run(stream::once(ready(event))).await.unwrap();
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Failed));
    }

    #[tokio::test]
    async fn splunk_insert_source() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.source = Template::try_from("/var/log/syslog".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(entry["source"].as_str(), Some("/var/log/syslog"));
    }

    #[tokio::test]
    async fn splunk_insert_index() {
        let cx = SinkContext::new_test();

        let mut config = config(Encoding::Text, vec![]).await;
        config.index = Template::try_from("custom_index".to_string()).ok();
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let event = Event::from(message.clone());
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(entry["index"].as_str().unwrap(), "custom_index");
    }

    #[tokio::test]
    async fn splunk_index_is_interpolated() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".to_string()];
        let mut config = config(Encoding::Json, indexed_fields).await;
        config.index = Template::try_from("{{ index_name }}".to_string()).ok();

        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("index_name", "custom_index");
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        let index = entry["index"].as_str().unwrap();
        assert_eq!("custom_index", index);
    }

    #[tokio::test]
    async fn splunk_insert_many() {
        let cx = SinkContext::new_test();

        let config = config(Encoding::Text, vec![]).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let (messages, events) = random_lines_with_stream(100, 10, None);
        components::run_sink(sink, events, &HTTP_SINK_TAGS).await;

        let mut found_all = false;
        for _ in 0..20 {
            let entries = recent_entries(None).await;

            found_all = messages.iter().all(|message| {
                entries
                    .iter()
                    .any(|entry| entry["_raw"].as_str().unwrap() == message)
            });

            if found_all {
                break;
            }

            sleep(Duration::from_millis(100)).await;
        }

        assert!(found_all);
    }

    #[tokio::test]
    async fn splunk_custom_fields() {
        let cx = SinkContext::new_test();

        let indexed_fields = vec!["asdf".into()];
        let config = config(Encoding::Json, indexed_fields).await;
        let (sink, _) = config.build(cx).await.unwrap();

        let message = random_string(100);
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("asdf", "hello");
        components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

        let entry = find_entry(message.as_str()).await;

        assert_eq!(message, entry["message"].as_str().unwrap());
        let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
        assert_eq!("hello", asdf);
    }

    // #[tokio::test]
    // async fn splunk_hostname() {
    //     let cx = SinkContext::new_test();

    //     let indexed_fields = vec!["asdf".into()];
    //     let config = config(Encoding::Json, indexed_fields).await;
    //     let (sink, _) = config.build(cx).await.unwrap();

    //     let message = random_string(100);
    //     let mut event = Event::from(message.clone());
    //     event.as_mut_log().insert("asdf", "hello");
    //     event.as_mut_log().insert("host", "example.com:1234");
    //     components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

    //     let entry = find_entry(message.as_str()).await;

    //     assert_eq!(message, entry["message"].as_str().unwrap());
    //     let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    //     assert_eq!("hello", asdf);
    //     let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
    //     assert_eq!("example.com:1234", host);
    // }

    // #[tokio::test]
    // async fn splunk_sourcetype() {
    //     let cx = SinkContext::new_test();

    //     let indexed_fields = vec!["asdf".to_string()];
    //     let mut config = config(Encoding::Json, indexed_fields).await;
    //     config.sourcetype = Template::try_from("_json".to_string()).ok();

    //     let (sink, _) = config.build(cx).await.unwrap();

    //     let message = random_string(100);
    //     let mut event = Event::from(message.clone());
    //     event.as_mut_log().insert("asdf", "hello");
    //     components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

    //     let entry = find_entry(message.as_str()).await;

    //     assert_eq!(message, entry["message"].as_str().unwrap());
    //     let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    //     assert_eq!("hello", asdf);
    //     let sourcetype = entry["sourcetype"].as_str().unwrap();
    //     assert_eq!("_json", sourcetype);
    // }

    // #[tokio::test]
    // async fn splunk_configure_hostname() {
    //     let cx = SinkContext::new_test();

    //     let config = HecSinkLogsConfig {
    //         host_key: "roast".into(),
    //         ..config(Encoding::Json, vec!["asdf".to_string()]).await
    //     };

    //     let (sink, _) = config.build(cx).await.unwrap();

    //     let message = random_string(100);
    //     let mut event = Event::from(message.clone());
    //     event.as_mut_log().insert("asdf", "hello");
    //     event.as_mut_log().insert("host", "example.com:1234");
    //     event.as_mut_log().insert("roast", "beef.example.com:1234");
    //     components::run_sink_event(sink, event, &HTTP_SINK_TAGS).await;

    //     let entry = find_entry(message.as_str()).await;

    //     assert_eq!(message, entry["message"].as_str().unwrap());
    //     let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    //     assert_eq!("hello", asdf);
    //     let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
    //     assert_eq!("beef.example.com:1234", host);
    // }
}
