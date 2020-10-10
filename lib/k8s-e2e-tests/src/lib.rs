use k8s_openapi::{
    api::core::v1::{Container, Pod, PodSpec},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use k8s_test_framework::{Framework, Interface, Reader};

pub const BUSYBOX_IMAGE: &str = "busybox:1.28";

pub fn make_framework() -> Framework {
    let interface = Interface::from_env().expect("interface is not ready");
    Framework::new(interface)
}

pub fn make_test_pod<'a>(
    namespace: &'a str,
    name: &'a str,
    command: &'a str,
    labels: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
) -> Pod {
    let labels: std::collections::BTreeMap<String, String> = labels
        .into_iter()
        .map(|(key, val)| (key.to_owned(), val.to_owned()))
        .collect();
    let labels = if labels.is_empty() {
        None
    } else {
        Some(labels)
    };
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels,
            ..ObjectMeta::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: name.to_owned(),
                image: Some(BUSYBOX_IMAGE.to_owned()),
                command: Some(vec!["sh".to_owned()]),
                args: Some(vec!["-c".to_owned(), command.to_owned()]),
                ..Container::default()
            }],
            restart_policy: Some("Never".to_owned()),
            ..PodSpec::default()
        }),
        ..Pod::default()
    }
}

pub fn parse_json(s: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(s)?)
}

pub fn generate_long_string(a: usize, b: usize) -> String {
    (0..a).fold(String::new(), |mut acc, i| {
        let istr = i.to_string();
        for _ in 0..b {
            acc.push_str(&istr);
        }
        acc
    })
}

/// Read the first line from vector logs and assert that it matches the expected
/// one.
/// This allows detecting the situations where things have gone very wrong.
pub async fn smoke_check_first_line(log_reader: &mut Reader) {
    // Wait for first line as a smoke check.
    let first_line = log_reader
        .read_line()
        .await
        .expect("unable to read first line");
    let expected_pat = "INFO vector::app: Log level \"info\" is enabled.\n";
    assert!(
        first_line.ends_with(expected_pat),
        "Expected a line ending with {:?} but got {:?}; vector might be malfunctioning",
        expected_pat,
        first_line
    );
}

pub enum FlowControlCommand {
    GoOn,
    Terminate,
}

pub async fn look_for_log_line<P>(
    log_reader: &mut Reader,
    mut predicate: P,
) -> Result<(), Box<dyn std::error::Error>>
where
    P: FnMut(serde_json::Value) -> FlowControlCommand,
{
    let mut lines_till_we_give_up = 10000;
    while let Some(line) = log_reader.read_line().await {
        println!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up <= 0 {
            println!("Giving up");
            log_reader.kill()?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        match predicate(val) {
            FlowControlCommand::GoOn => {
                // Not what we were looking for, go on.
            }
            FlowControlCommand::Terminate => {
                // We are told we should stop, request that log reader is
                // killed.
                // This doesn't immediately stop the reading because we want to
                // process the pending buffers first.
                log_reader.kill()?;
            }
        }
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    Ok(())
}
