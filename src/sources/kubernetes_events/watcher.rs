//! Resumable list/watch support for Kubernetes Events.
//!
//! Unlike `kube::runtime::watcher`, this stream exposes the resource versions that are safe to
//! checkpoint and accepts a resource version from which to resume after a leader transition.

use std::time::Duration;

use async_stream::stream;
use futures::{Stream, StreamExt};
use k8s_openapi::api::events::v1::Event as KubeEvent;
use kube::{
    Api, Error as KubeError,
    api::{ListParams, WatchEvent, WatchParams},
    core::response::Status,
    runtime::watcher,
};
use tokio::time::sleep;

const WATCH_EOF_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ApplyKind {
    Added,
    Modified,
}

#[derive(Debug)]
pub(super) enum Event {
    Apply { event: KubeEvent, kind: ApplyKind },
    Delete(KubeEvent),
    Init,
    InitApply(KubeEvent),
    InitDone { resource_version: String },
    Bookmark { resource_version: String },
}

impl Event {
    /// Returns the resource version that becomes safe to persist after this event is handled.
    /// Initial-list objects do not advance progress individually because a list is an unordered
    /// snapshot; its collection resource version is committed only at `InitDone`.
    pub(super) fn checkpoint(&self) -> Option<&str> {
        match self {
            Self::Apply { event, .. } | Self::Delete(event) => {
                event.metadata.resource_version.as_deref()
            }
            Self::InitDone { resource_version } | Self::Bookmark { resource_version } => {
                Some(resource_version)
            }
            Self::Init | Self::InitApply(_) => None,
        }
    }
}

#[derive(Debug)]
pub(super) enum Error {
    InitialListFailed(KubeError),
    WatchStartFailed(KubeError),
    WatchStatus(Box<Status>),
    WatchFailed(KubeError),
    NoResourceVersion,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitialListFailed(error) => {
                write!(formatter, "failed to perform initial object list: {error}")
            }
            Self::WatchStartFailed(error) => {
                write!(formatter, "failed to start watching object: {error}")
            }
            Self::WatchStatus(error) => {
                write!(
                    formatter,
                    "error returned by apiserver during watch: {error}"
                )
            }
            Self::WatchFailed(error) => write!(formatter, "watch stream failed: {error}"),
            Self::NoResourceVersion => {
                formatter.write_str("no metadata.resourceVersion in Kubernetes API response")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InitialListFailed(error)
            | Self::WatchStartFailed(error)
            | Self::WatchFailed(error) => Some(error),
            Self::WatchStatus(error) => Some(error.as_ref()),
            Self::NoResourceVersion => None,
        }
    }
}

pub(super) fn resumable_watcher(
    api: Api<KubeEvent>,
    config: watcher::Config,
    initial_resource_version: Option<String>,
) -> impl Stream<Item = Result<Event, Error>> + Send {
    stream! {
        let mut resource_version = initial_resource_version;

        'restart: loop {
            if resource_version.is_none() {
                yield Ok(Event::Init);

                let mut continue_token = None;
                let mut collection_resource_version = None;

                loop {
                    let params = list_params(&config, continue_token.as_deref());
                    let list = match api.list(&params).await {
                        Ok(list) => list,
                        Err(error) => {
                            yield Err(Error::InitialListFailed(error));
                            continue 'restart;
                        }
                    };

                    if let Some(version) = list.metadata.resource_version.filter(|v| !v.is_empty()) {
                        collection_resource_version = Some(version);
                    }
                    continue_token = list.metadata.continue_.filter(|v| !v.is_empty());

                    for event in list.items {
                        yield Ok(Event::InitApply(event));
                    }

                    if continue_token.is_none() {
                        break;
                    }
                }

                let Some(version) = collection_resource_version else {
                    yield Err(Error::NoResourceVersion);
                    continue;
                };

                resource_version = Some(version.clone());
                yield Ok(Event::InitDone {
                    resource_version: version,
                });
            }

            let version = resource_version
                .as_deref()
                .expect("resource version is set after initial list");
            let mut watch = match api.watch(&watch_params(&config), version).await {
                Ok(watch) => Box::pin(watch),
                Err(error) => {
                    if is_gone_error(&error) {
                        resource_version = None;
                    }
                    yield Err(Error::WatchStartFailed(error));
                    continue;
                }
            };

            while let Some(result) = watch.next().await {
                match result {
                    Ok(WatchEvent::Added(event)) => {
                        let Some(version) = event.metadata.resource_version.clone().filter(|v| !v.is_empty()) else {
                            resource_version = None;
                            yield Err(Error::NoResourceVersion);
                            continue 'restart;
                        };
                        resource_version = Some(version);
                        yield Ok(Event::Apply {
                            event,
                            kind: ApplyKind::Added,
                        });
                    }
                    Ok(WatchEvent::Modified(event)) => {
                        let Some(version) = event.metadata.resource_version.clone().filter(|v| !v.is_empty()) else {
                            resource_version = None;
                            yield Err(Error::NoResourceVersion);
                            continue 'restart;
                        };
                        resource_version = Some(version);
                        yield Ok(Event::Apply {
                            event,
                            kind: ApplyKind::Modified,
                        });
                    }
                    Ok(WatchEvent::Deleted(event)) => {
                        let Some(version) = event.metadata.resource_version.clone().filter(|v| !v.is_empty()) else {
                            resource_version = None;
                            yield Err(Error::NoResourceVersion);
                            continue 'restart;
                        };
                        resource_version = Some(version);
                        yield Ok(Event::Delete(event));
                    }
                    Ok(WatchEvent::Bookmark(bookmark)) => {
                        let version = bookmark.metadata.resource_version;
                        if version.is_empty() {
                            resource_version = None;
                            yield Err(Error::NoResourceVersion);
                            continue 'restart;
                        }
                        resource_version = Some(version.clone());
                        yield Ok(Event::Bookmark {
                            resource_version: version,
                        });
                    }
                    Ok(WatchEvent::Error(status)) => {
                        if status.code == 410 {
                            resource_version = None;
                        }
                        yield Err(Error::WatchStatus(status));
                        continue 'restart;
                    }
                    Err(error) => {
                        yield Err(Error::WatchFailed(error));
                        continue 'restart;
                    }
                }
            }

            // A normal watch timeout and a proxy repeatedly returning an empty response both end
            // as a clean EOF. Delay reconnecting so the latter cannot create a tight request loop.
            sleep(WATCH_EOF_RETRY_DELAY).await;
        }
    }
}

fn list_params(config: &watcher::Config, continue_token: Option<&str>) -> ListParams {
    ListParams {
        label_selector: config.label_selector.clone(),
        field_selector: config.field_selector.clone(),
        timeout: config.timeout,
        limit: config.page_size,
        continue_token: continue_token.map(ToString::to_string),
        version_match: None,
        resource_version: None,
    }
}

fn watch_params(config: &watcher::Config) -> WatchParams {
    WatchParams {
        label_selector: config.label_selector.clone(),
        field_selector: config.field_selector.clone(),
        timeout: config.timeout,
        bookmarks: true,
        send_initial_events: false,
    }
}

fn is_gone_error(error: &KubeError) -> bool {
    matches!(error, KubeError::Api(status) if status.code == 410)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::kubernetes_events::test_util::make_event;
    use chrono::Utc;
    use http_1::{Request, Response, header::CONTENT_TYPE};
    use kube::{Client, client::Body};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::service_fn;

    fn json_response(value: impl Into<Vec<u8>>) -> Response<Body> {
        Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(value.into()))
            .unwrap()
    }

    #[test]
    fn only_complete_progress_markers_are_checkpointed() {
        let init_apply = Event::InitApply(make_event("uid", "10", Utc::now()));
        assert_eq!(init_apply.checkpoint(), None);

        let init_done = Event::InitDone {
            resource_version: "20".to_string(),
        };
        assert_eq!(init_done.checkpoint(), Some("20"));

        let apply = Event::Apply {
            event: make_event("uid", "21", Utc::now()),
            kind: ApplyKind::Modified,
        };
        assert_eq!(apply.checkpoint(), Some("21"));

        let bookmark = Event::Bookmark {
            resource_version: "22".to_string(),
        };
        assert_eq!(bookmark.checkpoint(), Some("22"));
    }

    #[tokio::test]
    async fn resumes_watch_from_persisted_resource_version() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = {
            let requests = Arc::clone(&requests);
            service_fn(move |request: Request<Body>| {
                requests.lock().unwrap().push(request.uri().to_string());
                let event = make_event("uid", "41", Utc::now());
                let body = format!(
                    "{}\n",
                    serde_json::json!({ "type": "MODIFIED", "object": event })
                );
                async move { Ok::<_, std::io::Error>(json_response(body.into_bytes())) }
            })
        };
        let api = Api::all(Client::new(service, "default"));
        let mut events = Box::pin(resumable_watcher(
            api,
            watcher::Config::default(),
            Some("40".to_string()),
        ));

        let event = events.next().await.unwrap().unwrap();
        assert!(matches!(
            event,
            Event::Apply {
                kind: ApplyKind::Modified,
                ..
            }
        ));
        assert_eq!(event.checkpoint(), Some("41"));

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("watch=true"));
        assert!(requests[0].contains("resourceVersion=40"));
    }

    #[tokio::test(start_paused = true)]
    async fn clean_watch_eof_delays_before_reconnecting() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let service = {
            let request_count = Arc::clone(&request_count);
            service_fn(move |_request: Request<Body>| {
                let request_number = request_count.fetch_add(1, Ordering::SeqCst);
                let body = if request_number == 0 {
                    Vec::new()
                } else {
                    format!(
                        "{}\n",
                        serde_json::json!({
                            "type": "MODIFIED",
                            "object": make_event("uid", "41", Utc::now()),
                        })
                    )
                    .into_bytes()
                };
                async move { Ok::<_, std::io::Error>(json_response(body)) }
            })
        };
        let api = Api::all(Client::new(service, "default"));
        let mut events = Box::pin(resumable_watcher(
            api,
            watcher::Config::default(),
            Some("40".to_string()),
        ));
        let started_at = tokio::time::Instant::now();

        let event = events.next().await.unwrap().unwrap();

        assert!(matches!(event, Event::Apply { .. }));
        assert!(started_at.elapsed() >= WATCH_EOF_RETRY_DELAY);
        assert_eq!(request_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cold_list_commits_collection_version_before_watching() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = {
            let request_count = Arc::clone(&request_count);
            let requests = Arc::clone(&requests);
            service_fn(move |request: Request<Body>| {
                let request_number = request_count.fetch_add(1, Ordering::SeqCst);
                requests.lock().unwrap().push(request.uri().to_string());
                let body = if request_number == 0 {
                    serde_json::json!({
                        "apiVersion": "events.k8s.io/v1",
                        "kind": "EventList",
                        "metadata": { "resourceVersion": "50" },
                        "items": [make_event("listed", "45", Utc::now())],
                    })
                    .to_string()
                } else {
                    format!(
                        "{}\n",
                        serde_json::json!({
                            "type": "ADDED",
                            "object": make_event("watched", "51", Utc::now()),
                        })
                    )
                };
                async move { Ok::<_, std::io::Error>(json_response(body.into_bytes())) }
            })
        };
        let api = Api::all(Client::new(service, "default"));
        let mut events = Box::pin(resumable_watcher(api, watcher::Config::default(), None));

        assert!(matches!(events.next().await.unwrap().unwrap(), Event::Init));

        let listed = events.next().await.unwrap().unwrap();
        assert!(matches!(listed, Event::InitApply(_)));
        assert_eq!(listed.checkpoint(), None);

        let init_done = events.next().await.unwrap().unwrap();
        assert_eq!(init_done.checkpoint(), Some("50"));

        let watched = events.next().await.unwrap().unwrap();
        assert!(matches!(
            watched,
            Event::Apply {
                kind: ApplyKind::Added,
                ..
            }
        ));
        assert_eq!(watched.checkpoint(), Some("51"));

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(!requests[0].contains("watch=true"));
        assert!(requests[1].contains("watch=true"));
        assert!(requests[1].contains("resourceVersion=50"));
    }

    #[tokio::test]
    async fn expired_resource_version_falls_back_to_a_fresh_list() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = {
            let request_count = Arc::clone(&request_count);
            let requests = Arc::clone(&requests);
            service_fn(move |request: Request<Body>| {
                let request_number = request_count.fetch_add(1, Ordering::SeqCst);
                requests.lock().unwrap().push(request.uri().to_string());
                let body = if request_number == 0 {
                    serde_json::json!({
                        "type": "ERROR",
                        "object": {
                            "apiVersion": "v1",
                            "kind": "Status",
                            "status": "Failure",
                            "message": "too old resource version: 40",
                            "reason": "Gone",
                            "code": 410,
                        },
                    })
                    .to_string()
                        + "\n"
                } else {
                    serde_json::json!({
                        "apiVersion": "events.k8s.io/v1",
                        "kind": "EventList",
                        "metadata": { "resourceVersion": "50" },
                        "items": [],
                    })
                    .to_string()
                };
                async move { Ok::<_, std::io::Error>(json_response(body.into_bytes())) }
            })
        };
        let api = Api::all(Client::new(service, "default"));
        let mut events = Box::pin(resumable_watcher(
            api,
            watcher::Config::default(),
            Some("40".to_string()),
        ));

        let error = events.next().await.unwrap().unwrap_err();
        assert!(matches!(error, Error::WatchStatus(status) if status.code == 410));
        assert!(matches!(events.next().await.unwrap().unwrap(), Event::Init));
        assert_eq!(
            events.next().await.unwrap().unwrap().checkpoint(),
            Some("50")
        );

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("watch=true"));
        assert!(requests[0].contains("resourceVersion=40"));
        assert!(!requests[1].contains("watch=true"));
        assert!(!requests[1].contains("resourceVersion="));
    }
}
