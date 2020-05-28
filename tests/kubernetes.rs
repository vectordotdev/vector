use k8s_openapi::{
    api::core::v1::{Container, Pod, PodSpec},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use kubernetes_test_framework::{test_pod, wait_for_resource::WaitFor, Framework, Interface};

const VECTOR_CONFIG: &str = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: vector-config
data:
  vector.toml: |
    [sinks.stdout]
        type = "console"
        inputs = ["kubernetes_logs"]
        target = "stdout"
        encoding = "json"
"#;

const BUSYBOX_IMAGE: &str = "busybox:1.28";

fn repeating_echo_cmd(marker: &str) -> String {
    format!(
        r#"echo before; i=0; while [ $i -le 600 ]; do sleep 0.1; echo "{}"; i=$((i+1)); done"#,
        marker
    )
}

fn make_framework() -> Framework {
    let interface = Interface::from_env().expect("interface is not ready");
    Framework::new(interface)
}

fn make_test_pod(namespace: &str, name: &str, command: &str) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
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

fn parse_json(s: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(s)?)
}

#[test]
fn test() -> Result<(), Box<dyn std::error::Error>> {
    let framework = make_framework();

    let vector = framework.vector("test-vector", VECTOR_CONFIG)?;
    framework.wait_for_rollout("test-vector", "daemonset/vector", vec!["--timeout=10s"])?;

    let test_namespace = framework.namespace("test-vector-test-pod")?;

    let test_pod = framework.test_pod(test_pod::Config::from_pod(&make_test_pod(
        "test-vector-test-pod",
        "test-pod",
        "echo MARKER",
    ))?)?;
    framework.wait(
        "test-vector-test-pod",
        vec!["pods/test-pod"],
        WaitFor::Condition("initialized"),
        vec!["--timeout=30s"],
    )?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector")?;

    // Wait for first line as a smoke check.
    let first_line = log_reader.next().expect("unable to read first line");
    let expected_pat = "INFO vector: Log level \"info\" is enabled.\n";
    assert!(
        first_line.ends_with(expected_pat),
        "Expected a line ending with {:?} but got {:?}; vector might be malfunctioning",
        expected_pat,
        first_line
    );

    // Read the rest of the log lines.
    let mut lines_till_we_give_up = 10000;
    let mut got_marker = false;
    while let Some(line) = log_reader.next() {
        println!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up <= 0 {
            println!("Giving up");
            log_reader.kill()?;
            break;
        }

        if !line.starts_with("{") {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, predend we don't
            // see it.
            continue;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // We got a marker, so we're pretty much done.
        log_reader.kill()?;
    }

    // Ensure log reader exited.
    log_reader.wait().expect("log reader wait failed");

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}
