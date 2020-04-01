// NOTE: Tests assume that Kubernetes is accessable and localy available image of vector
//       that is to be tested is present.
#![cfg(feature = "kubernetes-integration-tests")]

use crate::test_util::{random_string, trace_init, wait_for};
use k8s_openapi::api::apps::v1::{DaemonSetSpec, DaemonSetStatus};
use k8s_openapi::api::core::v1::{PodSpec, PodStatus};
use kube::{
    api::{
        Api, DeleteParams, KubeObject, ListParams, Log, LogParams, Object, PostParams,
        PropagationPolicy, RawApi,
    },
    client::APIClient,
    config,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

static NAMESPACE_MARKER: &'static str = "$(TEST_NAMESPACE)";
static USER_NAMESPACE_MARKER: &'static str = "$(USER_TEST_NAMESPACE)";
static USER_CONTAINERS_MARKER: &'static str = "$(USER_CONTAINERS)";
static USER_POD_UID_MARKER: &'static str = "$(USER_POD_UIDS)";
static ARGS_MARKER: &'static str = "$(ARGS_MARKER)";
static ECHO_NAME: &'static str = "$(ECHO_NAME)";
static WAIT_LIMIT: usize = 120; //s
/// Environment variable which contains name of the image to be tested.
/// Image tag defines imagePullPolicy:
/// - tag is 'latest' => imagePullPolicy: Always
/// - else => imagePullPolicy: IfNotPresent
static KUBE_TEST_IMAGE_ENV: &'static str = "KUBE_TEST_IMAGE";
static IMAGE_MARKER: &'static str = "$(IMAGE)";

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
      include_namespaces = ["$(USER_TEST_NAMESPACE)"]
      include_container_names = [$(USER_CONTAINERS)]
      include_pod_uids = [$(USER_POD_UIDS)]

    [sinks.out]
      type = "console"
      inputs = ["kubernetes_logs"]
      target = "stdout"

      encoding = "json"
      healthcheck = true

  # This line is not in VECTOR.TOML
"#;

// TODO: use localy builded image of vector
pub static VECTOR_YAML: &'static str = r#"
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
        image: $(IMAGE)
        # By ommiting imagePullPolicy, https://kubernetes.io/docs/concepts/configuration/overview/#container-images comes into effect.
        # This allows the caller to define imagePullPolicy with image tag:
        # - tag is 'latest' => imagePullPolicy: Always
        # - else => imagePullPolicy: IfNotPresent
        args: ["-vv"]
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
        env:
        - name: VECTOR_NODE_NAME
          valueFrom:
            fieldRef:
              fieldPath: spec.nodeName
"#;

static ECHO_YAML: &'static str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: $(ECHO_NAME)
  namespace: $(TEST_NAMESPACE)
spec:
  containers:
  - name: $(ECHO_NAME)
    image: busybox:1.28
    command: ["echo"]
    args: ["$(ARGS_MARKER)"]
  restartPolicy: Never
"#;

static REPEATING_ECHO_YAML: &'static str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: $(ECHO_NAME)
  namespace: $(TEST_NAMESPACE)
spec:
  containers:
  - name: $(ECHO_NAME)
    image: busybox:1.28
    command: ["sh"]
    args: ["-c","echo before; i=0; while [ $i -le 600 ]; do sleep 0.1; echo $(ARGS_MARKER); i=$((i+1)); done"]
  restartPolicy: Never
"#;

pub type KubePod = Object<PodSpec, PodStatus>;
pub type KubeDaemon = Object<DaemonSetSpec, DaemonSetStatus>;

pub struct Kube {
    client: APIClient,
    namespace: String,
}

impl Kube {
    // Also immedietely creates namespace
    pub fn new(namespace: &str) -> Self {
        trace_init();
        let config = config::load_kube_config().expect("failed to load kubeconfig");
        let client = APIClient::new(config);
        let kube = Kube {
            client,
            namespace: namespace.to_string(),
        };
        kube.create_with(&Api::v1Namespace(kube.client.clone()), NAMESPACE_YAML);
        kube
    }

    fn api<K, F: FnOnce(APIClient) -> Api<K>>(&self, f: F) -> Api<K> {
        f(self.client.clone()).within(self.namespace.as_str())
    }

    /// Will substitute NAMESPACE_MARKER
    pub fn create<K, F: FnOnce(APIClient) -> Api<K>>(&self, f: F, yaml: &str) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        self.create_with(&self.api(f), yaml)
    }

    /// Will substitute NAMESPACE_MARKER
    fn create_with<K>(&self, api: &Api<K>, yaml: &str) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        let yaml = yaml.replace(NAMESPACE_MARKER, self.namespace.as_str());
        let map: serde_yaml::Value = serde_yaml::from_slice(yaml.as_bytes()).unwrap();
        let json = serde_json::to_vec(&map).unwrap();
        retry(|| {
            api.create(&PostParams::default(), json.clone())
                .map_err(|error| {
                    format!("Failed creating Kubernetes object with error: {:?}", error)
                })
        })
    }

    /// Will substitute NAMESPACE_MARKER
    pub fn create_raw_with<K>(&self, api: &RawApi, yaml: &str) -> K
    where
        K: DeserializeOwned,
    {
        let yaml = yaml.replace(NAMESPACE_MARKER, self.namespace.as_str());
        let map: serde_yaml::Value = serde_yaml::from_slice(yaml.as_bytes()).unwrap();
        let json = serde_json::to_vec(&map).unwrap();
        retry(|| {
            api.create(&PostParams::default(), json.clone())
                .and_then(|request| self.client.request(request))
                .map_err(|error| {
                    format!("Failed creating Kubernetes object with error: {:?}", error)
                })
        })
    }

    fn list(&self, object: &KubeDaemon) -> Vec<KubePod> {
        retry(|| {
            self.api(Api::v1Pod)
                .list(&ListParams {
                    field_selector: Some(format!("metadata.namespace=={}", self.namespace)),
                    ..ListParams::default()
                })
                .map_err(|error| format!("Failed listing Pods with error: {:?}", error))
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
                .map_err(|error| format!("Failed getting Pod logs with error: {:?}", error))
        })
        .lines()
        .map(|s| s.to_owned())
        .collect()
    }

    pub fn wait_for_running(&self, mut object: KubeDaemon) -> KubeDaemon {
        let api = self.api(Api::v1DaemonSet);
        retry(move || {
            object = api
                .get_status(object.meta().name.as_str())
                .map_err(|error| format!("Failed getting object status with error: {:?}", error))?;
            match object.status.clone().ok_or("Object status is missing")? {
                DaemonSetStatus {
                    desired_number_scheduled,
                    number_available: Some(number_available),
                    ..
                } if number_available == desired_number_scheduled => Ok(object.clone()),
                status => {
                    // Try fetching Vectors logs for diagnostic purpose
                    for daemon_instance in self.list(&object) {
                        if let Ok(logs) = self.api(Api::v1Pod).log(
                            daemon_instance.metadata.name.as_str(),
                            &LogParams::default(),
                        ) {
                            info!("Deamon Vector's logs:\n{}", logs);
                        }
                    }

                    Err(format!(
                        "DaemonSet not yet ready with status: {:?}. Pods status: {:?}",
                        status,
                        self.list(&object)
                            .into_iter()
                            .map(|pod| pod.status)
                            .collect::<Vec<_>>()
                    ))
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
                .map_err(|error| format!("Failed getting object status with error: {:?}", error))?;
            match object.status.clone().ok_or("Object status is missing")? {
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if phase.as_str() == goal => Ok(object.clone()),
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if legal.contains(&phase.as_str()) => {
                    Err(format!("Pod in intermediate phase: {:?}", phase))
                }
                PodStatus { phase, .. } => {
                    Err(format!("Illegal pod phase with phase: {:?}", phase))
                }
            }
        })
    }

    /// Deleter will delete given resource on drop.
    pub fn deleter(&self, api: RawApi, name: &str) -> Deleter {
        Deleter {
            client: self.client.clone(),
            api,
            name: name.to_owned(),
        }
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

pub struct Deleter {
    client: APIClient,
    name: String,
    api: RawApi,
}

impl Drop for Deleter {
    fn drop(&mut self) {
        let _ = self
            .api
            .delete(
                self.name.as_str(),
                &DeleteParams {
                    propagation_policy: Some(PropagationPolicy::Background),
                    ..DeleteParams::default()
                },
            )
            .and_then(|request| self.client.request_text(request))
            .map_err(|error| error!(message = "Failed deleting Kubernetes object.",%error));
    }
}

/// If F returns None, retries it after some time, for some count.
/// Panics if all trys fail.
fn retry<F: FnMut() -> Result<R, E>, R, E: std::fmt::Debug>(mut f: F) -> R {
    let mut last_error = None;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(WAIT_LIMIT as u64) {
        match f() {
            Ok(data) => return data,
            Err(error) => {
                error!(?error);
                last_error = Some(error);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        debug!("Retrying");
    }
    panic!("Timed out while waiting. Last error: {:?}", last_error);
}

pub fn user_namespace<S: AsRef<str>>(namespace: S) -> String {
    "user-".to_owned() + namespace.as_ref()
}

fn echo_create(template: &str, kube: &Kube, name: &str, message: &str) -> KubePod {
    kube.create(
        Api::v1Pod,
        template
            .replace(ECHO_NAME, name)
            .replace(ARGS_MARKER, format!("{}", message).as_str())
            .as_str(),
    )
}

#[must_use]
pub fn echo(kube: &Kube, name: &str, message: &str) -> KubePod {
    // Start echo
    let echo = echo_create(ECHO_YAML, kube, name, message);

    // Wait for success state
    kube.wait_for_success(echo.clone());

    echo
}

fn create_vector<'a>(
    kube: &Kube,
    user_namespace: &str,
    container_name: impl Into<Option<&'a str>>,
    pod_uid: impl Into<Option<&'a str>>,
    config: &str,
) -> KubeDaemon {
    let container_name = container_name
        .into()
        .map(|name| format!("\"{}\"", name))
        .unwrap_or("".to_string());

    let pod_uid = pod_uid
        .into()
        .map(|uid| format!("\"{}\"", uid))
        .unwrap_or("".to_string());

    let image_name = std::env::var(KUBE_TEST_IMAGE_ENV).expect(
        format!(
            "{} environment variable must be set with the image name to be tested.",
            KUBE_TEST_IMAGE_ENV
        )
        .as_str(),
    );

    // Start vector
    kube.create(
        Api::v1ConfigMap,
        config
            .replace(USER_NAMESPACE_MARKER, user_namespace)
            .replace(USER_CONTAINERS_MARKER, container_name.as_str())
            .replace(USER_POD_UID_MARKER, pod_uid.as_str())
            .as_str(),
    );

    kube.create(
        Api::v1DaemonSet,
        VECTOR_YAML
            .replace(IMAGE_MARKER, image_name.as_str())
            .as_str(),
    )
}

pub fn start_vector<'a>(
    kube: &Kube,
    user_namespace: &str,
    container_name: impl Into<Option<&'a str>>,
    config: &str,
) -> KubeDaemon {
    let vector = create_vector(kube, user_namespace, container_name, None, config);

    // Wait for running state
    kube.wait_for_running(vector.clone());

    vector
}

pub fn logs(kube: &Kube, vector: &KubeDaemon) -> Vec<Value> {
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
    let namespace = format!("one-log-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str(), None, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", &message);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube, &vector) {
            if line["message"].as_str().unwrap() == message {
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_old_log() {
    let namespace = format!("old-log-{}", Uuid::new_v4());
    let message_old = random_string(300);
    let message_new = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let user = Kube::new(&user_namespace);
    let kube = Kube::new(&namespace);

    // echo old
    let _echo_old = echo(&user, "echo-old", &message_old);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str(), None, CONFIG_MAP_YAML);

    // echo new
    let _echo_new = echo(&user, "echo-new", &message_new);

    // Verify logs
    wait_for(|| {
        let mut logged = false;
        for line in logs(&kube, &vector) {
            if line["message"].as_str().unwrap() == message_old {
                panic!("Old message logged");
            } else if line["message"].as_str().unwrap() == message_new {
                // OK
                logged = true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        logged
    });
}

#[test]
fn kube_multi_log() {
    let namespace = format!("multi-log-{}", Uuid::new_v4());
    let mut messages = vec![
        random_string(300),
        random_string(300),
        random_string(300),
        random_string(300),
    ];
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str(), None, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", messages.join("\\n").as_str());

    // Verify logs
    wait_for(|| {
        for line in logs(&kube, &vector) {
            if Some(line["message"].as_str().unwrap()) == messages.first().map(|s| s.as_str()) {
                messages.remove(0);
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        messages.is_empty()
    });
}

#[test]
fn kube_object_uid() {
    let namespace = "kube-object-uid".to_owned(); //format!("object-uid-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str(), None, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", &message);
    // Verify logs
    wait_for(|| {
        // If any daemon has object uid, done.
        for line in logs(&kube, &vector) {
            if line.get("object_uid").is_some() {
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_container() {
    let namespace = format!("diff-container-{}", Uuid::new_v4());
    let message0 = random_string(300);
    let message1 = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let vector = start_vector(&kube, user_namespace.as_str(), "echo1", CONFIG_MAP_YAML);

    // Start echo0
    let _echo0 = echo(&user, "echo0", &message0);
    let _echo1 = echo(&user, "echo1", &message1);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube, &vector) {
            if line["message"].as_str().unwrap() == message1 {
                // DONE
                return true;
            } else if line["message"].as_str().unwrap() == message0 {
                panic!("Received message from not included container");
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_namespace() {
    let namespace = format!("diff-namespace-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace0 = user_namespace(namespace.to_owned() + "0");
    let user_namespace1 = user_namespace(namespace.to_owned() + "1");

    let kube = Kube::new(&namespace);
    let user0 = Kube::new(&user_namespace0);
    let user1 = Kube::new(&user_namespace1);

    // Start vector
    let vector = start_vector(&kube, user_namespace1.as_str(), None, CONFIG_MAP_YAML);

    // Start echo0
    let _echo0 = echo(&user0, "echo", &message);
    let _echo1 = echo(&user1, "echo", &message);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube, &vector) {
            if line["message"].as_str().unwrap() == message {
                if let Some(namespace) = line.get("pod_namespace") {
                    assert_eq!(namespace.as_str().unwrap(), user_namespace1);
                }
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_pod_uid() {
    let namespace = format!("diff-pod-uid-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start echo
    let echo0 = echo_create(REPEATING_ECHO_YAML, &user, "echo0", &message);
    let echo1 = echo_create(REPEATING_ECHO_YAML, &user, "echo1", &message);

    let uid0 = echo0.metadata.uid.as_ref().expect("UID present");
    let uid1 = echo1.metadata.uid.as_ref().expect("UID present");

    let mut uid = String::new();

    while uid0.starts_with(&uid) {
        uid = uid1.chars().take(uid.chars().count() + 1).collect();
    }

    // Create vector
    let vector = create_vector(
        &kube,
        user_namespace.as_str(),
        None,
        uid.as_str(),
        CONFIG_MAP_YAML,
    );

    // Wait for running state
    kube.wait_for_running(vector.clone());

    // Verify logs
    wait_for(|| {
        // If any daemon logged message, done.
        for line in logs(&kube, &vector) {
            if line["message"].as_str().unwrap() == message {
                if let Some(uid) = line.get("object_uid") {
                    assert_eq!(uid.as_str().unwrap(), echo1.metadata.uid.as_ref().unwrap());
                } else if let Some(name) = line.get("container_name") {
                    assert_eq!(name.as_str().unwrap(), "echo1");
                }
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}
