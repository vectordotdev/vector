use futures::{Future, Sink, Stream};
use router::{sinks, Record};
use serde_json::Value as JsonValue;

const TOKEN: &str = "7b440750-defd-4a64-8306-14027bd89368";

#[test]
fn test_insert_message_into_splunk() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let sink = sinks::splunk::hec(TOKEN.to_owned(), "http://localhost:8088".to_string());

    let message = random_string();
    let message2 = message.clone();

    let pump = sink.and_then(|sink| sink.send(Record::new_from_line(message2)));

    rt.block_on(pump).unwrap();

    // It usually takes ~1 second for the event to show up in search, so poll until
    // we see it.
    let entry = (0..20)
        .find_map(|_| {
            let entry = recent_entries().remove(0);
            if entry["_raw"].as_str().unwrap() == message {
                Some(entry)
            } else {
                ::std::thread::sleep(std::time::Duration::from_millis(100));
                None
            }
        })
        .expect("Didn't find event in Splunk");

    assert_eq!(message, entry["_raw"].as_str().unwrap());
}

fn recent_entries() -> Vec<JsonValue> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    // http://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
    let mut res = client
        .post("https://localhost:8089/services/search/jobs?output_mode=json")
        .form(&[("search", "search *"), ("exec_mode", "oneshot")])
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
