#![deny(warnings)]

use std::{collections::BTreeMap, env};

use indoc::formatdoc;
use k8s_openapi::{
    api::core::v1::{Affinity, Container, Pod, PodAffinity, PodAffinityTerm, PodSpec},
    apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta},
};
use k8s_test_framework::{
    test_pod, wait_for_resource::WaitFor, CommandBuilder, Framework, Interface, Manager, Reader,
};
use tracing::{debug, error, info};

pub mod metrics;

pub const BUSYBOX_IMAGE: &str = "busybox:1.28";

pub fn init() {
    _ = env_logger::builder().is_test(true).try_init();
}

pub fn get_namespace() -> String {
    use rand::Rng;

    // Generate a random alphanumeric (lowercase) string to ensure each test is run with unique
    // names.
    // There is a 36 ^ 5 chance of a name collision, which is likely to be an acceptable risk.
    let id: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(5)
        .map(|num| (num as char).to_ascii_lowercase())
        .collect();

    format!("vector-{}", id)
}

pub fn get_namespace_appended(namespace: &str, suffix: &str) -> String {
    format!("{}-{}", namespace, suffix)
}

/// Gets a name we can use for roles to prevent them conflicting with other tests.
/// Uses the provided namespace as the root.
pub fn get_override_name(namespace: &str, suffix: &str) -> String {
    format!("{}-{}", namespace, suffix)
}

/// Is the MULTINODE environment variable set?
pub fn is_multinode() -> bool {
    env::var("MULTINODE").is_ok()
}

/// Create config adding fullnameOverride entry. This allows multiple tests
/// to be run against the same cluster without the role names clashing.
pub fn config_override_name(name: &str, cleanup: bool) -> String {
    let vectordir = if is_multinode() {
        format!("{}-vector", name)
    } else {
        "vector".to_string()
    };

    let volumeconfig = if is_multinode() {
        formatdoc!(
            r#"
            dataVolume:
              hostPath:
                path: /var/lib/{}/
            "#,
            vectordir,
        )
    } else {
        String::new()
    };

    let cleanupconfig = if cleanup {
        formatdoc!(
            r#"
        extraVolumeMounts:
          - name: var-lib
            mountPath: /var/writablelib
            readOnly: false

        lifecycle:
          preStop:
            exec:
              command:
                - sh
                - -c
                - rm -rf /var/writablelib/{}
                "#,
            vectordir,
        )
    } else {
        String::new()
    };

    formatdoc!(
        r#"
        fullnameOverride: "{}"
        {}
        {}
        "#,
        name,
        volumeconfig,
        cleanupconfig,
    )
}

pub fn make_framework() -> Framework {
    let interface = Interface::from_env().expect("interface is not ready");
    Framework::new(interface)
}

pub fn collect_btree<'a>(
    items: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
) -> Option<std::collections::BTreeMap<String, String>> {
    let collected: std::collections::BTreeMap<String, String> = items
        .into_iter()
        .map(|(key, val)| (key.to_owned(), val.to_owned()))
        .collect();
    if collected.is_empty() {
        return None;
    }
    Some(collected)
}

pub fn make_test_container<'a>(name: &'a str, command: &'a str) -> Container {
    Container {
        name: name.to_owned(),
        image: Some(BUSYBOX_IMAGE.to_owned()),
        command: Some(vec!["sh".to_owned()]),
        args: Some(vec!["-c".to_owned(), command.to_owned()]),
        ..Container::default()
    }
}

pub fn make_test_pod_with_containers<'a>(
    namespace: &'a str,
    name: &'a str,
    labels: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
    annotations: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
    affinity: Option<Affinity>,
    containers: Vec<Container>,
) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: collect_btree(labels),
            annotations: collect_btree(annotations),
            ..ObjectMeta::default()
        },
        spec: Some(PodSpec {
            containers,
            restart_policy: Some("Never".to_owned()),
            affinity,
            ..PodSpec::default()
        }),
        ..Pod::default()
    }
}

/// Since the tests only scan the logs from an agent on a single node, we want to make sure that all the test pods are on
/// the same node so the agent picks them all.
pub fn make_test_pod_with_affinity<'a>(
    namespace: &'a str,
    name: &'a str,
    command: &'a str,
    labels: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
    annotations: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
    affinity_label: Option<(&'a str, &'a str)>,
    affinity_namespace: Option<&'a str>,
) -> Pod {
    let affinity = affinity_label.map(|(label, value)| {
        let selector = LabelSelector {
            match_expressions: None,
            match_labels: Some({
                let mut map = BTreeMap::new();
                map.insert(label.to_string(), value.to_string());
                map
            }),
        };

        Affinity {
            node_affinity: None,
            pod_affinity: Some(PodAffinity {
                preferred_during_scheduling_ignored_during_execution: None,
                required_during_scheduling_ignored_during_execution: Some(vec![PodAffinityTerm {
                    label_selector: Some(selector),
                    namespaces: Some(vec![affinity_namespace.unwrap_or(namespace).to_string()]),
                    topology_key: "kubernetes.io/hostname".to_string(),
                }]),
            }),
            pod_anti_affinity: None,
        }
    });

    make_test_pod_with_containers(
        namespace,
        name,
        labels,
        annotations,
        affinity,
        vec![make_test_container(name, command)],
    )
}

pub fn make_test_pod<'a>(
    namespace: &'a str,
    name: &'a str,
    command: &'a str,
    labels: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
    annotations: impl IntoIterator<Item = (&'a str, &'a str)> + 'a,
) -> Pod {
    make_test_pod_with_affinity(namespace, name, command, labels, annotations, None, None)
}

pub fn parse_json(s: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(s)
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
    let expected_pat = "INFO vector::app:";
    assert!(
        first_line.contains(expected_pat),
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
        debug!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up <= 0 {
            info!("Giving up");
            log_reader.kill().await?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = match parse_json(&line) {
            Ok(val) => val,
            Err(err) if err.is_eof() => {
                // We got an EOF error, this is most likely some very long line,
                // we don't produce lines this bing is our test cases, so we'll
                // just skip the error - as if it wasn't a JSON string.
                error!("The JSON line we just got was incomplete, most likely it was too long, so we're skipping it");
                continue;
            }
            Err(err) => return Err(err.into()),
        };

        match predicate(val) {
            FlowControlCommand::GoOn => {
                // Not what we were looking for, go on.
            }
            FlowControlCommand::Terminate => {
                // We are told we should stop, request that log reader is
                // killed.
                // This doesn't immediately stop the reading because we want to
                // process the pending buffers first.
                log_reader.kill().await?;
            }
        }
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    Ok(())
}

/// Create a pod for our other pods to have an affinity to to ensure they are all deployed on
/// the same node.
pub async fn create_affinity_pod(
    framework: &Framework,
    namespace: &str,
    affinity_label: &str,
) -> Result<Manager<CommandBuilder>, Box<dyn std::error::Error>> {
    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            namespace,
            "affinity-pod",
            "tail -f /dev/null",
            vec![(affinity_label, "yes")],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            namespace,
            vec!["pods/affinity-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    Ok(test_pod)
}
