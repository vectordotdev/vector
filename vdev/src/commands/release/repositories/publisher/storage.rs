//! Conditional S3 publication. Ambiguous writes deliberately retain the lock.
use super::model::{Config, Entry, Snapshot, canonical, is_digest, mutable, root_order};
use anyhow::{Context, Result, ensure};
use aws_sdk_s3::{
    Client,
    config::{Credentials, Region, retry::RetryConfig, timeout::TimeoutConfig},
    primitives::ByteStream,
    types::{ChecksumMode, ServerSideEncryption},
};
use aws_smithy_runtime_api::client::{
    interceptors::{Intercept, context::BeforeDeserializationInterceptorContextRef},
    runtime_components::RuntimeComponents,
};
use aws_smithy_types::config_bag::ConfigBag;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone)]
struct ServerClock(Arc<Mutex<Option<DateTime<Utc>>>>);
impl Intercept for ServerClock {
    fn name(&self) -> &'static str {
        "PublicationServerClock"
    }
    fn read_before_deserialization(
        &self,
        context: &BeforeDeserializationInterceptorContextRef<'_>,
        _: &RuntimeComponents,
        _: &mut ConfigBag,
    ) -> Result<(), aws_smithy_runtime_api::box_error::BoxError> {
        let date = context
            .response()
            .headers()
            .get("date")
            .ok_or("AWS response has no server date")?;
        *self.0.lock().map_err(|_| "Server clock poisoned")? =
            Some(DateTime::parse_from_rfc2822(date)?.with_timezone(&Utc));
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(super) struct Head {
    pub entry: Entry,
    pub etag: String,
    pub version: String,
}
pub(super) enum Body<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}
pub(super) trait Store {
    fn head(&self, key: &str) -> Result<Option<Head>>;
    fn read(&self, key: &str) -> Result<(Vec<u8>, Head)>;
    fn heads(&self, keys: &[String]) -> Result<Vec<Option<Head>>> {
        keys.iter().map(|key| self.head(key)).collect()
    }
    fn put(
        &self,
        key: &str,
        body: Body<'_>,
        entry: &Entry,
        previous: Option<&Head>,
        immutable: bool,
    ) -> Result<Head>;
    fn download(&self, key: &str, path: &Path, limit: u64) -> Result<Entry>;
    fn server_time(&self) -> Result<DateTime<Utc>>;
    fn owner(&self) -> &str;
}
pub(super) struct S3Store {
    runtime: tokio::runtime::Runtime,
    client: Client,
    bucket: String,
    live: String,
    pub owner: String,
    clock: ServerClock,
}
fn checksum(value: Option<&str>, size: Option<i64>) -> Result<Entry> {
    let hash = STANDARD.decode(value.context("Missing stored SHA-256 checksum")?)?;
    ensure!(hash.len() == 32, "Invalid stored SHA-256 checksum");
    Ok(Entry {
        sha256: hex::encode(hash),
        size: u64::try_from(size.context("Missing object size")?)?,
    })
}
impl S3Store {
    pub fn new(config: &Config) -> Result<Self> {
        let role_arn = std::env::var("PACKAGE_REPOSITORIES_ROLE_ARN")
            .context("Set the publishing role ARN")?;
        ensure!(
            regex::Regex::new(&format!(
                r"^arn:aws:iam::{}:role/[A-Za-z0-9_+=,.@-]+$",
                config.account
            ))?
            .is_match(&role_arn),
            "Wrong publishing role ARN"
        );
        // Snapshot one temporary session, with no credential-provider renewal.
        let required = |name| -> Result<String> {
            let value = std::env::var(name).context("A fixed temporary AWS session is required")?;
            ensure!(!value.is_empty(), "Empty AWS credential");
            Ok(value)
        };
        let credentials = Credentials::new(
            required("AWS_ACCESS_KEY_ID")?,
            required("AWS_SECRET_ACCESS_KEY")?,
            Some(required("AWS_SESSION_TOKEN")?),
            None,
            "fixed-publisher-session",
        );
        let runtime = tokio::runtime::Runtime::new()?;
        let sdk = runtime.block_on(
            aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(Region::new(config.region.clone()))
                .credentials_provider(credentials)
                .retry_config(RetryConfig::disabled())
                .timeout_config(
                    TimeoutConfig::builder()
                        .connect_timeout(Duration::from_secs(10))
                        .read_timeout(Duration::from_secs(90))
                        .operation_timeout(Duration::from_mins(5))
                        .build(),
                )
                .load(),
        );
        let identity =
            runtime.block_on(aws_sdk_sts::Client::new(&sdk).get_caller_identity().send())?;
        let role = role_arn.rsplit('/').next().context("Invalid role")?;
        let owner = identity.arn().context("Missing caller ARN")?.to_owned();
        ensure!(
            identity.account() == Some(config.account.as_str())
                && owner.starts_with(&format!(
                    "arn:aws:sts::{}:assumed-role/{role}/",
                    config.account
                )),
            "Wrong AWS account or role"
        );
        let policy = runtime.block_on(
            aws_sdk_iam::Client::new(&sdk)
                .get_role()
                .role_name(role)
                .send(),
        )?;
        ensure!(
            policy
                .role()
                .context("Missing role")?
                .max_session_duration()
                .is_some_and(|seconds| seconds <= 3600),
            "Role exceeds the one-hour recovery bound"
        );
        let clock = ServerClock(Arc::new(Mutex::new(None)));
        let client = Client::from_conf(
            aws_sdk_s3::config::Builder::from(&sdk)
                .interceptor(clock.clone())
                .build(),
        );
        Ok(Self {
            runtime,
            client,
            bucket: config.bucket.clone(),
            live: config.live_prefix.clone(),
            owner,
            clock,
        })
    }
}
impl Store for S3Store {
    fn heads(&self, keys: &[String]) -> Result<Vec<Option<Head>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        // Join every request before returning a conflict and releasing the lock.
        std::thread::scope(|scope| {
            let workers: Vec<_> = keys
                .chunks(keys.len().div_ceil(16))
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|key| self.head(key))
                            .collect::<Result<Vec<_>>>()
                    })
                })
                .collect();
            let completed: Vec<_> = workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| anyhow::anyhow!("S3 inspection worker panicked"))?
                })
                .collect();
            Ok(completed
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect())
        })
    }
    fn owner(&self) -> &str {
        &self.owner
    }
    fn server_time(&self) -> Result<DateTime<Utc>> {
        self.clock
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Server clock poisoned"))?
            .context("No AWS server time")
    }
    fn head(&self, key: &str) -> Result<Option<Head>> {
        let result = self.runtime.block_on(
            self.client
                .head_object()
                .bucket(&self.bucket)
                .key(key)
                .checksum_mode(ChecksumMode::Enabled)
                .send(),
        );
        let output = match result {
            Ok(value) => value,
            Err(error) => {
                let status = error.raw_response().map(|r| r.status().as_u16());
                if status == Some(404) {
                    return Ok(None);
                }
                if status == Some(403) {
                    let listed = self.runtime.block_on(
                        self.client
                            .list_objects_v2()
                            .bucket(&self.bucket)
                            .prefix(key)
                            .max_keys(1)
                            .send(),
                    )?;
                    if listed.contents().iter().all(|o| o.key() != Some(key)) {
                        return Ok(None);
                    }
                }
                return Err(error.into());
            }
        };
        Ok(Some(Head {
            entry: checksum(output.checksum_sha256(), output.content_length())?,
            etag: output.e_tag().context("Missing ETag")?.to_owned(),
            version: output
                .version_id()
                .context("Bucket versioning is required")?
                .to_owned(),
        }))
    }
    fn read(&self, key: &str) -> Result<(Vec<u8>, Head)> {
        let output = self.runtime.block_on(
            self.client
                .get_object()
                .bucket(&self.bucket)
                .key(key)
                .checksum_mode(ChecksumMode::Enabled)
                .send(),
        )?;
        let head = Head {
            entry: checksum(output.checksum_sha256(), output.content_length())?,
            etag: output.e_tag().context("Missing ETag")?.to_owned(),
            version: output
                .version_id()
                .context("Bucket versioning is required")?
                .to_owned(),
        };
        ensure!(head.entry.size < super::model::LIMIT, "Oversized S3 object");
        let data = self
            .runtime
            .block_on(output.body.collect())?
            .into_bytes()
            .to_vec();
        ensure!(
            Entry::bytes(&data) == head.entry,
            "S3 read failed checksum verification"
        );
        Ok((data, head))
    }
    fn download(&self, key: &str, destination: &Path, limit: u64) -> Result<Entry> {
        ensure!(!destination.is_symlink(), "Invalid download destination");
        let output = self.runtime.block_on(
            self.client
                .get_object()
                .bucket(&self.bucket)
                .key(key)
                .checksum_mode(ChecksumMode::Enabled)
                .send(),
        )?;
        let expected = checksum(output.checksum_sha256(), output.content_length())?;
        ensure!(expected.size <= limit, "S3 object grew before download");
        let mut target = fs::File::create(destination)?;
        let mut digest = Sha256::new();
        let mut size = 0;
        self.runtime.block_on(async {
            let mut body = output.body;
            while let Some(chunk) = body.try_next().await? {
                size += chunk.len() as u64;
                ensure!(size <= limit, "Oversized download");
                digest.update(&chunk);
                target.write_all(&chunk)?;
            }
            Ok::<_, anyhow::Error>(())
        })?;
        let actual = Entry {
            sha256: hex::encode(digest.finalize()),
            size,
        };
        ensure!(actual == expected, "S3 body failed checksum verification");
        Ok(actual)
    }
    fn put(
        &self,
        key: &str,
        body: Body<'_>,
        entry: &Entry,
        previous: Option<&Head>,
        immutable: bool,
    ) -> Result<Head> {
        let body = match body {
            Body::Bytes(bytes) => ByteStream::from(bytes.to_owned()),
            Body::File(path) => self.runtime.block_on(ByteStream::from_path(path))?,
        };
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_length(i64::try_from(entry.size)?)
            .checksum_sha256(STANDARD.encode(hex::decode(&entry.sha256)?))
            .server_side_encryption(ServerSideEncryption::Aes256)
            .content_type(content_type(key))
            .cache_control(format!(
                "{},{}",
                if key.starts_with(&self.live) {
                    "public"
                } else {
                    "private"
                },
                if immutable {
                    "max-age=31536000,immutable"
                } else {
                    "max-age=0,no-cache,must-revalidate"
                }
            ));
        request = if let Some(head) = previous {
            request.if_match(&head.etag)
        } else {
            request.if_none_match("*")
        };
        let output = self.runtime.block_on(request.send())?;
        Ok(Head {
            entry: entry.clone(),
            etag: output.e_tag().context("Missing ETag")?.to_owned(),
            version: output
                .version_id()
                .context("Bucket versioning is required")?
                .to_owned(),
        })
    }
}
fn content_type(key: &str) -> &'static str {
    if key.ends_with(".deb") {
        "application/vnd.debian.binary-package"
    } else if key.ends_with(".rpm") {
        "application/x-rpm"
    } else if key.ends_with(".json") {
        "application/json"
    } else if key.ends_with(".gz") {
        "application/gzip"
    } else if key.ends_with(".xml") {
        "application/xml"
    } else {
        "application/octet-stream"
    }
}
#[derive(Debug)]
struct Conflict(String);
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Conflict {}
fn conflict(message: impl Into<String>) -> anyhow::Error {
    Conflict(message.into()).into()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    schema: u32,
    pub held: bool,
    pub active: Option<String>,
    pub pending: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    recover_after: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    nonce: Option<String>,
}
pub(super) struct Publisher<'a> {
    pub store: &'a dyn Store,
    pub config: &'a Config,
    state: Option<State>,
    head: Option<Head>,
}
impl<'a> Publisher<'a> {
    pub fn new(store: &'a dyn Store, config: &'a Config) -> Self {
        Self {
            store,
            config,
            state: None,
            head: None,
        }
    }
    pub fn load_state(&self) -> Result<(State, Option<Head>)> {
        if self.store.head(&self.config.control_key)?.is_none() {
            return Ok((
                State {
                    schema: 1,
                    held: false,
                    active: None,
                    pending: None,
                    owner: None,
                    recover_after: None,
                    nonce: None,
                },
                None,
            ));
        }
        let (data, head) = self.store.read(&self.config.control_key)?;
        let state: State = serde_json::from_slice(&data)?;
        ensure!(
            state.schema == 1
                && [&state.active, &state.pending]
                    .into_iter()
                    .flatten()
                    .all(|s| is_digest(s)),
            "Invalid publication state"
        );
        ensure!(
            !state.held || (state.recover_after.is_some() && state.owner.is_some()),
            "Incomplete held lock"
        );
        Ok((state, Some(head)))
    }
    fn write_state(&mut self) -> Result<()> {
        let state = self.state.as_mut().context("Lock not initialized")?;
        state.nonce = Some(uuid::Uuid::new_v4().simple().to_string());
        let data = canonical(state)?;
        self.head = Some(self.store.put(
            &self.config.control_key,
            Body::Bytes(&data),
            &Entry::bytes(&data),
            self.head.as_ref(),
            false,
        )?);
        Ok(())
    }
    fn acquire(&mut self) -> Result<()> {
        let (mut state, head) = self.load_state()?;
        ensure!(
            !state.held,
            "Publisher busy; locks are never stolen on timeout"
        );
        state.held = true;
        state.owner = Some(self.store.owner().to_owned());
        state.recover_after = Some(self.store.server_time()? + chrono::Duration::minutes(65));
        self.state = Some(state);
        self.head = head;
        self.write_state()
    }
    fn release(&mut self) -> Result<()> {
        self.state.as_mut().context("No lock")?.held = false;
        self.write_state()
    }
    pub fn recover_lock(&mut self, version: &str, owner_stopped: bool) -> Result<()> {
        let (state, head) = self.load_state()?;
        ensure!(
            owner_stopped,
            "Confirm that the previous workflow/process has stopped"
        );
        ensure!(
            state.held && head.as_ref().is_some_and(|h| h.version == version),
            "Lock version changed or is released"
        );
        ensure!(
            self.store.server_time()? >= state.recover_after.context("Missing recovery time")?,
            "Recovery waits 65 minutes for the previous fixed STS session to expire"
        );
        self.state = Some(state);
        self.head = head;
        self.release()
    }
    pub fn ensure_bytes(&self, key: &str, data: &[u8]) -> Result<()> {
        self.ensure_immutable(key, Body::Bytes(data), &Entry::bytes(data))
    }
    pub fn ensure_file(&self, key: &str, path: &Path, entry: &Entry) -> Result<()> {
        self.ensure_immutable(key, Body::File(path), entry)
    }
    fn ensure_immutable(&self, key: &str, body: Body<'_>, entry: &Entry) -> Result<()> {
        if let Some(head) = self.store.head(key)? {
            if head.entry != *entry {
                return Err(conflict(format!("Immutable object conflict: {key}")));
            }
            return Ok(());
        }
        self.store.put(key, body, entry, None, true)?;
        Ok(())
    }
    pub fn archived(&self, id: &str) -> Result<Snapshot> {
        ensure!(is_digest(id), "Invalid snapshot ID");
        Snapshot::decode(
            &self
                .store
                .read(&format!("{}{id}/manifest.json", self.config.archive_prefix))?
                .0,
            Some(id),
        )
    }
    fn preflight(&self, snapshot: &Snapshot) -> Result<()> {
        let active = &self.state.as_ref().context("No lock")?.active;
        if active.as_deref() != Some(&snapshot.id) {
            if *active != snapshot.base {
                return Err(conflict(
                    "Repository advanced after preparation; regenerate against current base",
                ));
            }
            if let Some(id) = active {
                for (name, entry) in self.archived(id)?.entries {
                    if !mutable(&name) && snapshot.entries.get(&name) != Some(&entry) {
                        return Err(conflict(format!(
                            "Publication would remove historical data: {name}"
                        )));
                    }
                }
            }
        }
        let keys: Vec<_> = snapshot
            .entries
            .keys()
            .map(|name| format!("{}{name}", self.config.live_prefix))
            .collect();
        let heads = self.store.heads(&keys)?;
        for ((name, entry), old) in snapshot.entries.iter().zip(heads) {
            if !mutable(name) {
                if old.as_ref().is_some_and(|h| h.entry != *entry) {
                    return Err(conflict(format!("Immutable object conflict: {name}")));
                }
                if old.is_none() && !snapshot.source.join(name).is_file() {
                    return Err(conflict(format!("Inherited object is missing: {name}")));
                }
            }
        }
        Ok(())
    }
    fn publish_locked(
        &mut self,
        snapshot: &Snapshot,
        recovery: Option<(&str, Option<&str>)>,
    ) -> Result<()> {
        let state = self.state.as_ref().context("No lock")?;
        if let Some((active, pending)) = recovery {
            let pair = (state.active.as_deref(), state.pending.as_deref());
            if snapshot.base.as_deref() != Some(active)
                || ![
                    (Some(active), pending),
                    (Some(active), Some(snapshot.id.as_str())),
                    (Some(snapshot.id.as_str()), None),
                ]
                .contains(&pair)
            {
                return Err(conflict("Repository changed after rollback preparation"));
            }
        } else if state.pending.as_ref().is_some_and(|p| p != &snapshot.id) {
            return Err(conflict("Recover pending publication or roll back"));
        }
        self.preflight(snapshot)?;
        let archive = format!("{}{}/", self.config.archive_prefix, snapshot.id);
        for (name, entry) in &snapshot.entries {
            if mutable(name) {
                self.ensure_file(
                    &format!("{archive}files/{name}"),
                    &snapshot.source.join(name),
                    entry,
                )?;
            }
        }
        self.ensure_bytes(&format!("{archive}manifest.json"), &snapshot.data)?;
        let mut metadata = BTreeMap::new();
        for (name, entry) in &snapshot.entries {
            if mutable(name) {
                let data = self.store.read(&format!("{archive}files/{name}"))?.0;
                if Entry::bytes(&data) != *entry {
                    return Err(conflict(format!(
                        "Archived metadata checksum mismatch: {name}"
                    )));
                }
                metadata.insert(name, data);
            }
        }
        self.state.as_mut().context("No lock")?.pending = Some(snapshot.id.clone());
        self.write_state()?;
        for (name, entry) in &snapshot.entries {
            if !mutable(name) && snapshot.source.join(name).exists() {
                self.ensure_file(
                    &format!("{}{name}", self.config.live_prefix),
                    &snapshot.source.join(name),
                    entry,
                )?;
            }
        }
        let mut names: Vec<_> = metadata.keys().copied().collect();
        names.sort_by_key(|name| {
            (
                if name.ends_with("/Packages") || name.ends_with("/Packages.gz") {
                    0
                } else {
                    1 + root_order(name)
                },
                *name,
            )
        });
        for name in names {
            let key = format!("{}{name}", self.config.live_prefix);
            let entry = &snapshot.entries[name];
            let previous = self.store.head(&key)?;
            if previous.as_ref().is_some_and(|h| h.entry == *entry) {
                continue;
            }
            self.store.put(
                &key,
                Body::Bytes(&metadata[name]),
                entry,
                previous.as_ref(),
                false,
            )?;
        }
        let keys: Vec<_> = snapshot
            .entries
            .keys()
            .map(|name| format!("{}{name}", self.config.live_prefix))
            .collect();
        for ((name, entry), head) in snapshot.entries.iter().zip(self.store.heads(&keys)?) {
            ensure!(
                head.is_some_and(|h| h.entry == *entry),
                "Published object failed final verification: {name}"
            );
        }
        let state = self.state.as_mut().context("No lock")?;
        state.active = Some(snapshot.id.clone());
        state.pending = None;
        self.write_state()
    }
    pub fn publish(
        &mut self,
        snapshot: &Snapshot,
        recovery: Option<(&str, Option<&str>)>,
    ) -> Result<()> {
        self.acquire()?;
        match self.publish_locked(snapshot, recovery) {
            Ok(()) => self.release(),
            Err(error) if error.is::<Conflict>() => {
                self.release()?;
                Err(error)
            }
            Err(error) => {
                Err(error.context("Publication stopped; lock remains held for operator recovery"))
            }
        }
    }
}
