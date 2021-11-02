# RFC 9465 - 2021-10-29 - Splunk HEC Indexer Acknowledgement

This RFC summarizes the Splunk HEC Indexer Acknowledgement protocol, outlines a
source-side implementation, and outlines a sink-side integration.

## Context

* [Support Splunk indexer acknowledgements](https://github.com/vectordotdev/vector/issues/9465)
  * [Support indexer acknowledgement with splunk_hec source](https://github.com/vectordotdev/vector/issues/6534)
  * [Support indexer acknowledgement with splunk_hec sink](https://github.com/vectordotdev/vector/issues/2374)
* [Splunk HEC Indexer Acknowledgement Docs (v7.3.2)](https://docs.splunk.com/Documentation/Splunk/7.3.2/Data/AboutHECIDXAck#About_HTTP_Event_Collector_Indexer_Acknowledgment)
* [Splunk HEC Indexer Acknowledgement Docs (latest)](https://docs.splunk.com/Documentation/Splunk/latest/Data/UsetheHTTPEventCollector)

## Scope

### Within scope

* Implementing indexer acknowledgement protocol in `splunk_hec` source
* Integrating end-to-end acknowledgements with indexer acknowledgements in
  `splunk_hec` sinks

### Out of scope

* Passing through Splunk channel IDs for `splunk_hec` source -> `splunk_hec`
  sink setups

## Pain

For the `splunk_hec` source, supporting indexer acknowledgements enables
customers relying on Splunk HEC to more easily transition to vector. Some
third-party Splunk integrations (e.g. AWS Kinesis Firehose) require the indexer
acknowledgement feature.

For the `splunk_hec` sinks, supporting indexer acknowledgements improves the
accuracy of vector’s end-to-end acknowledgement system when used with
`splunk_hec` sinks.

## Overview: Splunk HEC Indexer Acknowledgement

Note: Some details and examples in this section are based on observed behavior
running the [Splunk v7.3.2
image](https://hub.docker.com/r/timberio/splunk-hec-test) used in integration
testing. Splunk deprecated this version as of October 22, 2021, but I have not
seen a difference comparing documentation between `7.3.2` and the latest version
(`8.2.3`).

Splunk HTTP Event Collector (HEC) Indexer Acknowledgements is an opt-in Splunk
feature that allows users to verify that data submitted to HEC has been
successfully persisted. By default, when a client sends a request to HEC, HEC
responds with a `200 OK` as soon as the request is received. This `200 OK` does
not guarantee that the Splunk event data in the request is persisted or fully
processed.

With indexer acknowledgement enabled, the `200 OK` response contains an
additional JSON body field specifying an `ackId` where `ackId` is an integer
identifier corresponding to the request. Note that a single HEC request can
include multiple Splunk events, so the `ackId` covers the entire request.

```jsonc
// Example response from local Splunk 7.3.2
{
   "text": "Success",
   "code": 0,
   "ackId": 10
}
```

Using one or more `ackId`’s, the user can query Splunk’s
`/services/collector/ack` endpoint to check on the status of associated
requests. For each `ackId`, Splunk returns `true`/`false` depending on whether
the data in the request has been persisted (in Splunk’s words, “replicated at
the desired replication factor”). Upon returning `true`, Splunk drops the
`ackId` status and all subsequent requests with the same `ackId` will return
`false` (based on Splunk channel-related expiration settings, `ackId`'s can be
reset/reused).

```jsonc
// Example request body
{
    "acks": [0, 1, 2]
}

// Example response body
{
    "acks":
        {
            "0": true,
            "1": false,
            "2": true
        }
}
```

In addition to the overall protocol, there are a few details worth highlighting:

* Authentication tokens
  * To submit requests to HEC in general, an authentication token must be
    included in request headers. Indexer acknowledgement is enabled per
    authentication token in Splunk. The `splunk_hec` source currently supports a
    list of valid authentication tokens. The `splunk_hec` sinks currently
    support configuring a single authentication token.

* Splunk channels
  * Upon enabling indexer acknowledgement, Splunk requires HEC requests to
    include a channel ID (any valid `guid` value). Querying
    `/services/collector/ack` also requires a channel ID.
  * `ackId`’s are assigned per-channel.

* Pending acks
  * Clients are not required or guaranteed to query for `ackId` status. To avoid
    running out of memory from `ackId` build up, Splunk offers a few
    configuration options to limit the number of pending `ackId`’s both overall
    and per-channel.
  * Splunk does not explicitly indicate whether event data is dropped or still
    pending processing. Either of these states will result in `{ack_id}: false`
    status. Splunk only advises that after a certain amount of time (e.g. 5
    minutes), the data can be considered dropped.

## Proposal: `splunk_hec` Source Indexer Acknowledgement

### User Experience

Users can configure the `splunk_hec` source with additional indexer acknowledgement related settings.

```toml
[sources.splunk_hec]
type = "splunk_hec"
# ...
acknowledgements.enabled = true
acknowledgements.max_pending_acks = 1_000_000
```

* `acknowledgements.enabled`  This defaults to `false` matching Splunk HEC's
  opt-in behavior.
* `acknowledgements.max_pending_acks` With acknowledgements enabled, this
  controls the maximum number of pending query ackId's (to avoid memory issues).
  This defaults to `10_000_000` matching Splunk HEC's default.

Since Vector does not share Splunk’s internal constraints, we can relax certain
protocol requirements to avoid unnecessary complexity. Specifically,

* Authentication tokens
  * Enabling indexer acknowledgements will be an overall `splunk_hec` source
    configuration rather than a per-token configuration. Users can configure a
    secondary Splunk source without acknowledgements if necessary, and/or users
    can ignore the `ackId`s for requests that do not participate in
    acknowledgement.

* Splunk Channels
  * The `splunk_hec` source will not require channel IDs for acknowledgement.
    Currently, we store channel IDs as an additional `LogEvent` field. If users
    intend to move data from a `splunk_hec` source to a `splunk_hec` sink,
    passing through the channel IDs can be helpful.
  * Rather than assign `ackId`’s per channel, the `splunk_hec` source will
    assign `ackId`’s across all its requests. In other words, if the source
    receives two requests with differing channel IDs, it will reply with `ackId
    = 0` and `ackId = 1` rather than `ackId = 0` and `ackId = 0`.

* Pending acks
  * We will support the overall `max_number_of_acked_requests_pending_query`
    configuration described in Splunk documentation (`max_pending_acks`) to
    avoid memory issues. We will not support the channel-based settings.

### Implementation

Implementation details mostly concern creating, storing, and updating `ackId`’s
and statuses. The following suggested implementation prioritizes memory
efficiency.

#### Data Structures

* A [bitvec](https://docs.rs/bitvec/0.22.3/bitvec/) (or similar structure)
  `ack_ids_in_use` whose indices represent `ackId`’s, max size is determined by
  `max_pending_acks`, and is initialized with all `0`'s'. A value of `0` at an
  index indicates that the `ackId` is not in-use. A value of `1` indicates
  in-use.
* A bitvec `ack_ids_ack_status` whose indices also correspond to `ackId`’s, max
  size is determined by `max_pending_acks`, and is initialized with all `0`'s. A
  value of `0` at an index indicates that the associated request data has not
  yet been delivered. A value of `1` indicates that the data has been delivered.
  These two bitvecs will be wrapped in a struct with useful methods. A single
  bitvec is not necessarily sufficient as we care about 3 states: `ackId` is
  available, `ackId` is pending/dropped, and `ackId` is delivered.
* An index pointer `currently_available_ack_id` initialized with `0`.

#### Process

* Assigning and updating `ackId`’s and statuses
  * To assign a new `ackId` to a new request, we use the
    `currently_available_ack_id` current value and set
    `ack_ids_in_use[currently_available_ack_id] = 1`. We increment
    `currently_available_ack_id` to the next index. We respond to the request
    with a `200 OK` with the `ackId` included in the JSON body.
    * If incrementing causes `currently_available_ack_id` to exceed the size of
      the bitvec, we wrap around to index `0` and search for the nearest
      available `ackId` (i.e. where `ack_ids_in_use[index] = 0`).
    * If there are no available `ackId`’s, we can begin to drop pending, in-use
      `ackId`’s, setting `ack_ids_in_use[...] = 0`. For simplicity, we can drop
      starting from the lowest `ackId` values (though there may be edge cases
      here that can cause this to continuously drop the same `ackId`'s). We
      should drop a chunk of pending `ackId`’s rather than a single `ackId` at a
      time to avoid potentially re-scanning the `ack_ids_in_use` array on the
      next incoming request.
  * To associate request data to the assigned `ackId` and receive the status of
    the data, we will use the existing `BatchNotifier`/`BatchReceiver` system
    from vector’s overall end-to-end-acknowledgement infrastructure.

    ```rust
    async fn handle_request(acknowledgements: bool, events: Vec<Events>, out: Pipeline, ack_id: usize...) {
        let receiver = acknowledgements.then(|| {
                let (batch, receiver) = BatchNotifier::new_with_receiver();
                for event in &mut events {
                    event.add_batch_notifier(Arc::clone(&batch));
                }
                receiver
            });

            out.send_all(&mut futures::stream::iter(events).map(Ok))
                ...
                .and_then(|_| handle_batch_status(ack_id, receiver))
                .await
    }
    ...
    async fn handle_batch_status(ack_id, receiver) {
        match receiver.await {
            BatchStatus::Delivered => // update ackId -> true,
            BatchStatus::Errored => // leave ackId -> false,
            BatchStatus::Failed => // leave ackId -> false,
        }
    }
    ```

  * When the data in the request is successfully delivered, we mark
    `ack_ids_ack_status[currently_available_ack_id] = 1`.
* Querying ackId status
  * We add a new `/services/collector/ack` route
  * `ackId`’s from incoming requests are used to index into
    `ack_ids_ack_status`. We return true/false depending on the value in the
    bitvec.
  * If we return true for an ackId, we mark `ack_ids_in_use[ack_id] = 0` and
    `ack_ids_ack_status[ack_id] = 0`. The `ackId` can then be reused.

## Proposal: `splunk_hec` Sinks Indexer Acknowledgement

### User Experience

The `splunk_hec` sinks will automatically integrate with indexer
acknowledgements if the user has enabled it in their Splunk instance (if
`ackId`’s are present in HEC responses). If indexer acknowledgement is disabled,
`splunk_hec` sinks will continue to finalize events based on the HEC response
status code.

### Implementation

* [Splunk recommendations for client integration with indexer acknowledgement](https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck#Indexer_acknowledgment_client_behavior)

We generate a single `guid` to use as a channel ID and include this in HEC
requests. After submitting events to HEC, we will parse the HTTP response for an
`ackId`. If no `ackId` is found, we rely on the current behavior of setting
`EventStatus` based solely on the HTTP status code.

If an `ackId` is found, we store it in a `Map<u64, bool>` shared with a
background tokio task which, for all pending `ackId`’s, will query
`/services/collector/ack`. This background task will query at an interval of 10
seconds (as recommended by Splunk) and update the shared structure based on the
results. Meanwhile, back in the HEC response handler, we’ll check the shared
structure for `true` at 10 second intervals up to a maximum time limit (5
minutes as recommended by Splunk).

```rust
fn call(&mut self, req: HecRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            ...
            // handle response
            let response = http_service.call(req).await?;
            // if ack_id is found
            self.ack_id_to_status_map.insert(ack_id, false);
            let mut interval = time::interval(Duration::from_secs(10));
            let mut retries = 0;
            while retries < RETRY_LIMIT {
                interval.tick().await;
                if self.ack_id_to_status_map.get(ack_id) {
                    // set EventStatus::Delivered and update map
                    break;
                }
                retries += 1;
            }

            // if ack_id is not found, fall back on current behavior
            let event_status = if response.status().is_success() {
                EventStatus::Delivered
            } else if response.status().is_server_error() {
                EventStatus::Errored
            } else {
                EventStatus::Failed
            };

            ...
        })
    }
```

If we receive a `true` response, we remove the ackId from the shared structure
and set `EventStatus::Delivered`.

If we do not receive a `true` response and the time limit expires, we remove the
ackId from the shared structure and set `EventStatus::Dropped`.

## Alternatives

* For `splunk_hec` source, [a user
  mentioned](https://github.com/vectordotdev/vector/issues/2374#issuecomment-795367929)
  the possibility of simply adding a mock implementation of indexer
  acknowledgements wherein queries to `/services/collector/ack` return true for
  any ackId. This was suggested prior to the existence of vector’s internal
  end-to-end acknowledgements system.

## Outstanding Questions

* For `splunk_hec` source, there are several choices to be made in regards to
  how closely we mimic the real Splunk HEC indexer acknowledgements behavior.
  While we’d like to avoid inheriting Splunk’s issues wherever possible, we’d
  also like to make this as fully functional as users need. Does the above make
  sense in terms of what our users need from vector?
* For `splunk_hec` sink, should we instead allow users to configure channel
  behavior (e.g. list of channel IDs, # of channels to use, etc.)?

## Plan of Attack

### `splunk_hec` Source Indexer Acknowledgement

* Update the `splunk_hec` source configuration with new settings
* Add the `/services/collector/ack` route and necessary logic (parsing, validating)
* Implement and unit test the `ackId` create and update system
* Integrate the ackId system with the current end-to-end acknowledgement system
* Integration and performance testing

### `splunk_hec` Sink Indexer Acknowledgement

* Refactor `build_request` to use channel ID
* Implement the shared structure and background task logic (querying
  `/services/collector/ack`)
* Refactor current `HecService` code to handle responses according to indexer
  acknowledgement integration
* Integration and performance testing
