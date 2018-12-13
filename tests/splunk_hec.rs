use futures::{Future, Sink};
use router::{sinks, Record};
use serde_json::Value as JsonValue;

const TOKEN: &str = "7b440750-defd-4a64-8306-14027bd89368";

#[test]
fn test_insert_message_into_splunk() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let sink = sinks::splunk::hec(TOKEN.to_owned(), "http://localhost:8088".to_string());

    let message = random_string();
    let record = Record::new_from_line(message.clone());

    let pump = sink.and_then(|sink| sink.send(record));

    rt.block_on(pump).unwrap();

    // It usually takes ~1 second for the event to show up in search, so poll until
    // we see it.
    let entry = (0..20)
        .find_map(|_| {
            recent_entries()
                .into_iter()
                .find(|entry| entry["_raw"].as_str().unwrap() == message)
                .or_else(|| {
                    ::std::thread::sleep(std::time::Duration::from_millis(100));
                    None
                })
        })
        .expect("Didn't find event in Splunk");

    assert_eq!(message, entry["_raw"].as_str().unwrap());
}

#[test]
fn test_insert_many() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let sink = sinks::splunk::hec(TOKEN.to_owned(), "http://localhost:8088".to_string());

    let messages = (0..10).map(|_| random_string()).collect::<Vec<_>>();
    let records = messages
        .iter()
        .map(|l| Record::new_from_line(l.clone()))
        .collect::<Vec<_>>();

    let pump = sink.and_then(|sink| sink.send_all(futures::stream::iter_ok(records)));

    rt.block_on(pump).unwrap();

    let mut found_all = false;
    for _ in 0..20 {
        let entries = recent_entries();

        found_all = messages.iter().all(|message| {
            entries
                .iter()
                .any(|entry| entry["_raw"].as_str().unwrap() == message)
        });

        if found_all {
            break;
        }

        ::std::thread::sleep(std::time::Duration::from_millis(100));
    }

    assert!(found_all);
}

#[test]
fn test_custom_fields() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let sink = sinks::splunk::hec(TOKEN.to_owned(), "http://localhost:8088".to_string());

    let message = random_string();
    let mut record = Record::new_from_line(message.clone());
    record.custom.insert("asdf".into(), "hello".to_owned());

    let pump = sink.and_then(|sink| sink.send(record));

    rt.block_on(pump).unwrap();

    let entry = (0..20)
        .find_map(|_| {
            recent_entries()
                .into_iter()
                .find(|entry| entry["_raw"].as_str().unwrap() == message)
                .or_else(|| {
                    ::std::thread::sleep(std::time::Duration::from_millis(100));
                    None
                })
        })
        .expect("Didn't find event in Splunk");

    assert_eq!(message, entry["_raw"].as_str().unwrap());
    assert_eq!("hello", entry["asdf"].as_str().unwrap());
}

fn recent_entries() -> Vec<JsonValue> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    // http://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
    let mut res = client
        .post("https://localhost:8089/services/search/jobs?output_mode=json")
        .form(&[
            ("search", "search *"),
            ("exec_mode", "oneshot"),
            ("f", "_raw"),
            ("f", "asdf"),
        ])
        .basic_auth("admin", Some("password"))
        .send()
        .unwrap();
    let json: JsonValue = res.json().unwrap();

    json["results"].as_array().unwrap().clone()
}

fn random_string() -> String {
    use rand::distributions::Alphanumeric;
    use rand::{thread_rng, Rng};

    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(100)
        .collect::<String>()
}
