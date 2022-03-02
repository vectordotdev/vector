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
customers relying on Splunk HEC to more easily transition to Vector. Some
third-party Splunk integrations (e.g. AWS Kinesis Firehose) require the indexer
acknowledgement feature.

For the `splunk_hec` sinks, supporting indexer acknowledgements improves the
accuracy of Vector’s end-to-end acknowledgement system when used with
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

Users can configure the `splunk_hec` source with additional indexer
acknowledgement related settings.

```toml
[sources.splunk_hec]
type = "splunk_hec"
# ...
acknowledgements.enabled = true
acknowledgements.max_pending_acks = 10_000_000
acknowledgements.max_number_of_ack_channel = 1_000_000
acknowledgements.max_pending_acks_per_channel = 1_000_000
acknowledgements.ack_idle_cleanup = true
acknowledgements.max_idle_time = 300
```

* `acknowledgements.enabled` This controls indexer acknowledgement enablement.
  Defaults to `false` matching Splunk HEC's opt-in behavior.
* `acknowledgements.max_pending_acks` With acknowledgements enabled, this
  controls the maximum number of pending query ackId's overall (across all channels)
  Defaults to `10_000_000` (Splunk default).
* `acknowledgements.max_number_of_ack_channel` This controls the max number of
  channels a client can use with the `splunk_hec` source. Defaults to
  `1_000_000` (Splunk default).
* `acknowledgements.max_pending_acks_per_channel` This controls the max number
  of pending query ackId's per channel. Defaults to `1_000_000` (Splunk
  default).
* `acknowledgements.ack_idle_cleanup` This controls whether the `splunk_hec`
  source will drop channel information (ackId's, statuses) after `max_idle_time`
  seconds. Defaults to `false` (Splunk default).
* `acknowledgements.max_idle_time` This controls the max channel idle time
  before removal. Defaults to `600` seconds (Splunk default).

Since Vector does not share Splunk’s internal constraints, we can relax certain
protocol requirements to avoid unnecessary complexity. For the most part, we
will take inspiration from Splunk. Specifically,

* Authentication tokens
  * Enabling indexer acknowledgements will be an overall `splunk_hec` source
    configuration rather than a per-token configuration. Users can configure a
    secondary Splunk source without acknowledgements if necessary, and/or users
    can ignore the `ackId`s for requests that do not participate in
    acknowledgement.

* Splunk Channels
  * Like Splunk, the `splunk_hec` source will require channel IDs for
    acknowledgement. Currently, we store channel IDs as an additional `LogEvent`
    field. If users intend to move data from a `splunk_hec` source to a
    `splunk_hec` sink, passing through the channel IDs can be helpful.
  * Like Splunk, the `splunk_hec` source will assign `ackId`’s per channel.

* Pending acks
  * We will support the same configuration settings described in Splunk
    documentation concerning pending `ackId`'s and channel configuration.

### Implementation

First, we describe implementation details for creating, storing, and updating
`ackId`’s and statuses. The following suggested implementation prioritizes
memory efficiency.

Second, we describe implementation details for channel behavior.

#### AckId Data Structures

* A
  [RoaringTreemap](https://docs.rs/roaring/0.8.0/roaring/treemap/struct.RoaringTreemap.html)
  (memory efficient bitset representing a set of integers) `ack_ids_in_use`. An
  `ackId` will be a `u64` value whose membership in the bitmap will indicate
  in-use. The maximum number of elements allowed in the bitmap at any given time
  is determined by `max_pending_acks_per_channel`.
* A `RoaringTreemap` `ack_ids_ack_status`. Membership of an `ackId` in
  `ack_ids_ack_status` indicates that the request data associated with said
  `ackId` has been delivered. Otherwise the request data is pending
  acknowledgement or dropped. Max size is again determined by
  `max_pending_acks_per_channel`. Note, a single bitmap is not necessarily
  sufficient as we care about 3 states: `ackId` is available, `ackId` is
  pending/dropped, and `ackId` is delivered.
* A `u64` value `currently_available_ack_id` initialized with `0`.
* The above will be wrapped in a `HecAckInfo` struct with utility methods.

#### AckId Process

* Assigning and updating `ackId`’s and statuses
  * To assign a new `ackId` to a new request, we use the
    `currently_available_ack_id` current value and insert the value into
    `ack_ids_in_use`. We increment `currently_available_ack_id` by 1. We respond
    to the request with a `200 OK` with the `ackId` included in the JSON body.
    Given the benefits of a roaring bitmap, supporting a potentially sparse and
    wide range of `ackId` values is still manageable.
    * If `max_pending_acks_per_channel == ack_ids_in_use.len()`, we drop
      `ackId`'s starting from `ack_ids_in_use.min()` which will generally be the
      oldest pending acks.
    * Incrementing `currently_available_ack_id` to exceed the `u64::MAX_SIZE`
      should be extremely rare, especially given that ack info is handled per
      channel and channels can be treated ephemerally (expiring after, for
      example, 10 minutes of idling). To simplify this edge case, we can
      consider resetting `ack_ids_in_use` and `ack_ids_ack_status` and setting
      `currently_available_ack_id = 0` at this point. If we did not reset and
      simply wrapped `currently_available_ack_id` back to `0`, we may begin
      having issues with the pending ack drop strategy
      (`ack_ids_in_use.min()` may no longer be oldest).
  * To associate request data to the assigned `ackId` and receive the status of
    the data, we will use the existing `BatchNotifier`/`BatchReceiver` system
    from Vector’s overall end-to-end-acknowledgement infrastructure.

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
            BatchStatus::Rejected => // leave ackId -> false,
        }
    }
    ```

  * When the data in the request is successfully delivered, we add the
    respective `ackId` to `ack_ids_ack_status`.
* Querying ackId status
  * We add a new `/services/collector/ack` route
  * `ackId`’s from incoming requests are used to query `ack_ids_ack_status`. We
    return true/false depending on whether the value is a member of the bitmap.
  * If we return true for an `ackId`, we remove said `ackId` from `ack_ids_in_use` and
    `ack_ids_ack_status`. The `ackId` can then be reused.

#### Channel Behavior

The above `ackId` process will occur per-channel. As part of the `splunk_hec`
source struct, we will store a `Arc<RwLock<Map<channel_id, channel>>>` where
`channel_id` is a `String` value and `channel` is a struct wrapping a
`last_used_timestamp` (used to expire channels) and the `HecAckInfo` structure
described above.

Incoming requests will be required to specify a `channelId`. On receiving a new
`channelId`, we create a new instance of `channel` and insert it into the `Map`.
We handle all `ackId` processing after mapping to the appropriate `HecAckInfo`
based on the client provided `channelId`. Every time a `channelId` and
corresponding `channel` information is used/accessed, we update the respective
`last_used_timestamp` to current.

We will also monitor the total number of pending acks across channels
`total_pending_acks` (can be updated as we use and remove `ackId`s) to respect
the `max_pending_acks` configuration. If a new request arrives and
`total_pending_acks == max_pending_acks`, we can drop a number of acks from the
least recently used channel (based on `last_used_timestamp`).

To expire idle channels, we use a background task that shares the channel `Map`
and compares each channel's `last_used_timestamp` to the current timestamp. If
the difference exceeds the configured `max_idle_time`, the channel will be
removed. This background task will loop at an interval based on `max_idle_time`.

## Proposal: `splunk_hec` Sinks Indexer Acknowledgement

### User Experience

The `splunk_hec` sinks will automatically integrate with indexer
acknowledgements if the user has enabled it in their Splunk instance (if
`ackId`’s are present in HEC responses). If indexer acknowledgement is disabled,
`splunk_hec` sinks will continue to finalize events based on the HEC response
status code.

Users can configure the `splunk_hec` sinks with the following indexer
acknowledgement settings

* `acknowledgements.query_interval` The amount of time to wait between requests
  to `services/collector/ack`. Defaults to 10 seconds as recommended by
  Splunk.
* `acknowledgements.retry_limit` The number of retry requests to
  `services/collector/ack`. Defaults to 30 which, along with the default
  `query_interval`, is 5 minutes of retrying as recommend by Splunk.

### Implementation

* [Splunk recommendations for client integration with indexer acknowledgement](https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck#Indexer_acknowledgment_client_behavior)

We generate a single `guid` to use as a channel ID and include this in HEC
requests. After submitting events to HEC, we will parse the HTTP response for an
`ackId`. If no `ackId` is found, we rely on the current behavior of setting
`EventStatus` based solely on the HTTP status code.

If an `ackId` is found, we store it in a `Arc<Mutex<Map<u64, (u8, Sender)>>>`
shared with a background tokio task which, for all pending `ackId`’s, will query
`/services/collector/ack`. The `(u8, Sender)` map value represents the number of
retries remaining and the send end of a one-shot notification channel.

This background task will query at an interval (configured or default) and with
a retry limit (configured or default). If we receive `true` for an `ackId`, we
remove the `ackId` from the map and notify an awaiting receiver with
`EventStatus::Delivered`. If we receive `false` for an `ackId`, we decrement its
remaining retry count. When remaining retries is `0`, we remove the `ackId` from
the map and notify with `EventStatus::Dropped`.

Back in the response handler, we’ll await the receiver. Below is an example of
response handler behavior.

```rust
fn call(&mut self, req: HecRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            ...

            // handle response
            let response = http_service.call(req).await?;
            // if ack_id is found
            let (tx, rx) = oneshot::channel::<EventStatus>();
            self.ack_id_to_status_map.insert(ack_id, (30, tx));
            let event_status = match rx.await {
                Ok(EventStatus::Delivered) => EventStatus::Delivered,
                Ok(_) => EventStatus::Dropped,
                Err(_) => EventStatus::Rejected,
            }
            ...

            // if ack_id is not found, fall back on current behavior
            let event_status = if response.status().is_success() {
                EventStatus::Delivered
            } else if response.status().is_server_error() {
                EventStatus::Errored
            } else {
                EventStatus::Rejected
            };

            ...
        })
    }
```

## Alternatives

* For `splunk_hec` source, [a user
  mentioned](https://github.com/vectordotdev/vector/issues/2374#issuecomment-795367929)
  the possibility of simply adding a mock implementation of indexer
  acknowledgements wherein queries to `/services/collector/ack` return true for
  any ackId. This was suggested prior to the existence of Vector’s internal
  end-to-end acknowledgements system.

## Outstanding Questions

* ~~For `splunk_hec` source, there are several choices to be made in regards to
  how closely we mimic the real Splunk HEC indexer acknowledgements behavior.
  While we’d like to avoid inheriting Splunk’s issues wherever possible, we’d
  also like to make this as fully functional as users need. Does the above make
  sense in terms of what our users need from Vector?~~ We will support most of
  the same Splunk HEC indexer acknowledgements behavior for better user
  experience and robustness.
* ~~For `splunk_hec` sink, should we instead allow users to configure channel
  behavior (e.g. list of channel IDs, # of channels to use, etc.)?~~ We will
  leave this for future work.

## Plan of Attack

### `splunk_hec` Source Indexer Acknowledgement

* Update the `splunk_hec` source configuration with new settings
* Add the `/services/collector/ack` route and necessary logic (parsing, validating)
* Implement and unit test the `ackId` create and update system
* Integrate the ackId system with the current end-to-end acknowledgement system
* Integration and performance testing

### `splunk_hec` Sink Indexer Acknowledgement

* Refactor `build_request` to use channel ID
* Add the shared structure and implement background task logic (querying
  `/services/collector/ack`)
* Refactor current `HecService` code to handle responses according to indexer
  acknowledgement integration
* Integration and performance testing

## Future Improvements

* Allow advanced configuration of channel ID generation for `splunk_hec` sinks
