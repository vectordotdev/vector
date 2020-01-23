// NOTE: Tests assume that Kubernetes is accessable and localy available image of vector
//       that is to be tested is present.
#![cfg(feature = "kubernetes-integration-tests")]

use crate::test_util::trace_init;
use k8s_openapi::api::apps::v1::{DaemonSetSpec, DaemonSetStatus};
use k8s_openapi::api::core::v1::{PodSpec, PodStatus};
use kube::{
    api::{
        Api, DeleteParams, KubeObject, ListParams, Log, LogParams, Object, PostParams,
        PropagationPolicy,
    },
    client::APIClient,
    config,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::borrow::Borrow;
use std::thread;
use std::time::Duration;

static NAMESPACE_MARKER: &'static str = "$(TEST_NAMESPACE)";
static USER_NAMESPACE_MARKER: &'static str = "$(USER_TEST_NAMESPACE)";
static ARGS_MARKER: &'static str = "$(ARGS_MARKER)";
static ECHO_NAME: &'static str = "$(ECHO_NAME)";
static WAIT_LIMIT: usize = 60; //s

// ******************************* CONFIG ***********************************//
// Replacing configurations need to have :
// - value of NAMESPACE_MARKER set as namespace
// - value of USER_NAMESPACE_MARKER set as only namespace to listen
// - image: vector:latest
// - imagePullPolicy: Never
// - split documents into separate things.

static NAMESPACE_YAML: &'static str = r#"
# Everything related to vector should be in this namespace
apiVersion: v1
kind: Namespace
metadata:
   name: $(TEST_NAMESPACE)
"#;

static CONFIG_MAP_YAML: &'static str = r#"
# ConfigMap which contains vector.toml configuration for pods.
apiVersion: v1
kind: ConfigMap
metadata:
  name: vector-config
  namespace: $(TEST_NAMESPACE)
data:
  vector-agent-config: |
    # VECTOR.TOML
    # Configuration for vector-agent

    # Set global options
    data_dir = "/tmp/vector/"

    # Ingest logs from Kubernetes
    [sources.kubernetes_logs]
      type = "kubernetes"

    [sinks.out]
      type = "console"
      inputs = ["kubernetes_logs"]
      target = "stdout"

      encoding = "json"
      healthcheck = true

  # This line is not in VECTOR.TOML
"#;

// TODO: use localy builded image of vector
static VECTOR_YAML: &'static str = r#"
# Vector agent runned on each Node where it collects logs from pods.
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: vector-agent
  namespace: $(TEST_NAMESPACE)
spec:
  minReadySeconds: 1
  selector:
    matchLabels:
      name: vector-agent
  template:
    metadata:
      labels:
        name: vector-agent
    spec:
      volumes:
      # Directory with logs
      - name: var-log
        hostPath:
          path: /var/log/
      # Docker log files in Kubernetes are symlinks to this folder.
      - name: var-lib
        hostPath:
          path: /var/lib/
      # Mount vector configuration from config map as a file vector.toml
      - name: config-dir
        configMap:
         name: vector-config
         items:
           - key: vector-agent-config
             path: vector.toml
      - name: tmp
        emptyDir: {}
      containers:
      - name: vector
        image: ktff/vector-improve:latest
        imagePullPolicy: Always
        volumeMounts:
        - name: var-log
          mountPath: /var/log/
          readOnly: true
        - name: var-lib
          mountPath: /var/lib
        - name: config-dir
          mountPath: /etc/vector
          readOnly: true
        - name: tmp
          mountPath: /tmp/vector/
"#;

static ECHO_YAML: &'static str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: $(ECHO_NAME)
  namespace: $(TEST_NAMESPACE)
spec:
  containers:
  - name: busybox
    image: busybox:1.28
    command: ["echo"]
    args: $(ARGS_MARKER)
  restartPolicy: Never
"#;

type KubePod = Object<PodSpec, PodStatus>;
type KubeDaemon = Object<DaemonSetSpec, DaemonSetStatus>;

struct Kube {
    client: APIClient,
    namespace: String,
}

impl Kube {
    // Also immedietely creates namespace
    fn new<S: Borrow<str>>(namespace: S) -> Self {
        trace_init();
        let config = config::load_kube_config().expect("failed to load kubeconfig");
        let client = APIClient::new(config);
        let kube = Kube {
            client,
            namespace: namespace.borrow().to_owned(),
        };
        kube.create_with(&Api::v1Namespace(kube.client.clone()), NAMESPACE_YAML);
        kube
    }

    fn api<K, F: FnOnce(APIClient) -> Api<K>>(&self, f: F) -> Api<K> {
        f(self.client.clone()).within(self.namespace.as_str())
    }

    /// Will substitute NAMESPACE_MARKER
    fn create<K, S: Borrow<str>, F: FnOnce(APIClient) -> Api<K>>(&self, f: F, yaml: S) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        self.create_with(&self.api(f), yaml)
    }

    /// Will substitute NAMESPACE_MARKER
    fn create_with<K, S: Borrow<str>>(&self, api: &Api<K>, yaml: S) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        let yaml = yaml
            .borrow()
            .replace(NAMESPACE_MARKER, self.namespace.as_str());
        let map: serde_yaml::Value = serde_yaml::from_slice(yaml.as_bytes()).unwrap();
        let json = serde_json::to_vec(&map).unwrap();
        retry(|| {
            api.create(&PostParams::default(), json.clone())
                .map_err(|error| {
                    error!(message = "Failed creating Kubernetes object", ?error);
                })
                .ok()
        })
    }

    fn list(&self, object: &KubeDaemon) -> Vec<KubePod> {
        retry(|| {
            self.api(Api::v1Pod)
                .list(&ListParams {
                    field_selector: Some(format!("metadata.namespace=={}", self.namespace)),
                    ..ListParams::default()
                })
                .map_err(|error| {
                    error!(message = "Failed listing Pods", ?error);
                })
                .ok()
        })
        .items
        .into_iter()
        .filter(|item| {
            item.metadata
                .name
                .as_str()
                .starts_with(object.metadata.name.as_str())
        })
        .collect()
    }

    fn logs(&self, pod_name: &str) -> Vec<String> {
        retry(|| {
            self.api(Api::v1Pod)
                .log(pod_name, &LogParams::default())
                .map_err(|error| {
                    error!(message = "Failed getting Pod logs", ?error);
                })
                .ok()
        })
        .lines()
        .map(|s| s.to_owned())
        .collect()
    }

    fn wait_for_running(&self, mut object: KubeDaemon) -> KubeDaemon {
        let api = self.api(Api::v1DaemonSet);
        retry(move || {
            object = api
                .get_status(object.meta().name.as_str())
                .map_err(|error| {
                    error!(message = "Failed getting object status", ?error);
                })
                .ok()?;
            match object.status.clone()? {
                DaemonSetStatus {
                    desired_number_scheduled,
                    number_available: Some(number_available),
                    ..
                } if number_available == desired_number_scheduled => Some(object.clone()),
                status => {
                    debug!(message = "DaemonSet not yet ready", ?status);
                    None
                }
            }
        })
    }

    fn wait_for_success(&self, mut object: KubePod) -> KubePod {
        let api = self.api(Api::v1Pod);
        let legal = ["Pending", "Running", "Succeeded"];
        let goal = "Succeeded";
        retry(move || {
            object = api
                .get_status(object.meta().name.as_str())
                .map_err(|error| {
                    error!(message = "Failed getting object status", ?error);
                })
                .ok()?;
            match object.status.clone()? {
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if phase.as_str() == goal => Some(object.clone()),
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if legal.contains(&phase.as_str()) => None,
                PodStatus { phase, .. } => {
                    error!(message = "Illegal pod phase", ?phase);
                    None
                }
            }
        })
    }

    fn cleanup(&self) {
        let _ = Api::v1Namespace(self.client.clone()).delete(
            self.namespace.as_str(),
            &DeleteParams {
                propagation_policy: Some(PropagationPolicy::Background),
                ..DeleteParams::default()
            },
        );
    }
}

impl Drop for Kube {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// If F returns None, retries it after some time, for some count.
/// Panics if all trys fail.
fn retry<F: FnMut() -> Option<R>, R>(mut f: F) -> R {
    for _ in 0..WAIT_LIMIT {
        if let Some(data) = f() {
            return data;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        debug!("Retrying");
    }
    panic!("timed out while waiting");
}

fn user_namespace(namespace: &str) -> String {
    "user-".to_owned() + namespace
}

#[must_use]
fn echo(kube: &Kube, name: &str, message: &str) -> KubePod {
    // Start echo
    let echo = kube.create(
        Api::v1Pod,
        ECHO_YAML
            .replace(ECHO_NAME, name)
            .replace(ARGS_MARKER, format!("[{:?}]", message).as_str()),
    );

    // Wait for success state
    kube.wait_for_success(echo.clone());

    echo
}

fn start_vector(kube: &Kube, user_namespace: &str) -> KubeDaemon {
    // Start vector
    kube.create(
        Api::v1ConfigMap,
        CONFIG_MAP_YAML.replace(USER_NAMESPACE_MARKER, user_namespace),
    );
    let vector = kube.create(Api::v1DaemonSet, VECTOR_YAML);

    // Wait for running state
    kube.wait_for_running(vector.clone());

    vector
}

fn logs(kube: &Kube, vector: &KubeDaemon) -> Vec<Value> {
    // Wait for logs to propagate
    thread::sleep(Duration::from_secs(4));
    let mut logs = Vec::new();
    for daemon_instance in kube.list(&vector) {
        debug!(message="daemon_instance",name=%daemon_instance.metadata.name);
        logs.extend(
            kube.logs(daemon_instance.metadata.name.as_str())
                .into_iter()
                .filter_map(|s| serde_json::from_slice::<Value>(s.as_ref()).ok()),
        );
    }
    logs
}

#[test]
fn kube_one_log() {
    let namespace = "vector-test-one-log";
    let message = "12";
    let user_namespace = user_namespace(namespace);

    let kube = Kube::new(namespace);
    let user = Kube::new(user_namespace.clone());

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str());

    // Start echo
    let _echo = echo(&user, "echo", message);

    // Verify logs
    // If any daemon logged message, done.
    for line in logs(&kube, &vector) {
        if line["message"].as_str().unwrap() == message {
            // DONE
            return;
        } else {
            debug!(namespace,log=%line);
        }
    }
    panic!("Vector didn't log message: {:?}", message);
}

#[test]
fn kube_old_log() {
    let namespace = "vector-test-old-log";
    let message_old = "13";
    let message_new = "14";
    let user_namespace = user_namespace(namespace);

    let user = Kube::new(user_namespace.clone());
    let kube = Kube::new(namespace);

    // echo old
    let _echo_old = echo(&user, "echo-old", message_old);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str());

    // echo new
    let _echo_new = echo(&user, "echo-new", message_new);

    // Verify logs
    // If any daemon logged message, done.
    let mut logged = false;
    for line in logs(&kube, &vector) {
        if line["message"].as_str().unwrap() == message_old {
            panic!("Old message logged");
        } else if line["message"].as_str().unwrap() == message_new {
            // OK
            logged = true;
        } else {
            debug!(namespace,log=%line);
        }
    }
    if logged {
        // Done
    } else {
        panic!("Vector didn't log message: {:?}", message_new);
    }
}

#[test]
fn kube_multi_log() {
    let namespace = "vector-test-multi-log";
    let mut messages = vec!["15", "16", "17", "18"];
    let user_namespace = user_namespace(namespace);

    let kube = Kube::new(namespace);
    let user = Kube::new(user_namespace.clone());

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str());

    // Start echo
    let _echo = echo(&user, "echo", messages.join("\n").as_str());

    // Verify logs
    // If any daemon logged message, done.
    for line in logs(&kube, &vector) {
        if Some(&line["message"].as_str().unwrap()) == messages.first() {
            messages.remove(0);
        } else {
            debug!(namespace,log=%line);
        }
    }
    if messages.is_empty() {
        //Done
    } else {
        panic!("Vector didn't log messages: {:?}", messages);
    }
}

#[test]
fn kube_object_uid() {
    let namespace = "vector-test-object-uid";
    let message = "19";
    let user_namespace = user_namespace(namespace);

    let kube = Kube::new(namespace);
    let user = Kube::new(user_namespace.clone());

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str());

    // Start echo
    let _echo = echo(&user, "echo", message);

    // Verify logs
    // If any daemon has object uid, done.
    for line in logs(&kube, &vector) {
        if line.get("object_uid").is_some() {
            // DONE
            return;
        } else {
            debug!(namespace,log=%line);
        }
    }

    panic!("Vector didn't log message: {:?}", message);
}
