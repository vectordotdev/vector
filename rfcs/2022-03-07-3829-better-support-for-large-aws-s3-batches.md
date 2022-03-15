# RFC 3829 - 2022-03-07 - Better Support for Large AWS S3 Batches

This RFC discusses improving vector's support for sending large batches to AWS S3.

## Context

- [Better support for large S3 batches, Issue
  #3829](https://github.com/vectordotdev/vector/issues/3829)

## Scope

### In scope

- Improving Vector's support for sending large batches to the AWS S3 sink.

### Out of scope

- Support for large batches in the Azure Blob, Datadog Archive, and
  GCP Cloud Storage sinks.

## Terminology

- Batch: A sequence of events collected within vector in preparation
  for uploading to a storage sink.
- Object: A completed upload stored on the destination sink's server(s).
- Part: A completed partial upload that will be composed into an object.

## Pain

The current iteration of our S3 sink is built for sending
small-to-medium size objects relatively frequently. This is apparent
in a few aspects of the config:

- The default maximum batch size is just 1,000 events.
- The default batch timeout is just 5 minutes.
- There is no way to set an unlimited batch size.
- There is no way to flush the batch at a fixed interval.

This is in contrast with some other tools like fluentd that will
create one file per hour by default. While the different approaches
have their pros and cons, it would be a benefit if Vector smoothly
supported the type of behavior users may be accustomed to from those
other tools.

While users could technically just increase Vector's batch sizes and
timeouts to approach the desired behavior, the implementation is not
designed for it and would cause significant problems:

- End-to-end acknowledgements are not issued until the batch is
  finished, which will cause backpressure to sources and possibly
  timeouts with retransmissions.
- The batches are stored in memory which could cause out of memory
  crashes.

## Proposal

Vector will use the AWS S3 multipart upload API as the primary upload
process. This allows the sink to persist data to S3 sooner, to
acknowledge data within Vector earlier, and to reduce the memory
required for buffering the upload batches.

The AWS S3 multipart upload API uses the following process:

1. The upload is created with the `CreateMultipartUpload` action. This
   action returns a unique identifier for the upload which is used for
   all following actions.
2. Individual parts are uploaded with the `UploadPart` action. These
   uploads contain a sequence number that identifies the order they
   will be assembled into the final object, and may be uploaded in any
   order.
3. The final object is assembled with the `CompleteMultipartUpload`
   action.

There are two major issues that this API presents that force some of
the implementation decisions below:

1. Vector may need to continue a multipart upload process across sink
   restarts (due to reloads, restarts, or crashes). Since the uploads
   require the presence of a unique identifier for both the multipart
   upload itself as well as each of the uploaded parts, Vector will
   need to store these identifiers with the partition that initiated
   the upload.
2. The upload parts (except for the final part) have a _minimum_ size
   limit of 5MB. Since the sink may need to be restarted before a
   batch reaches this minimum size, it will also need to buffer the
   batch to disk before uploading the part.

### User Experience

The enhanced sink will add support for a new time-based upload
mode. This mode will create a new object in the destination bucket
once every configured interval instead of creating an object when a
batch is filled or times out. This new object will be uploaded
incrementally, but will not be visible in the bucket until all the
parts of it are complete.

The following upload behaviors are possible through the configuration
described below. In all cases, where possible, the sink will upload
the batch in multiple parts to reduce the amount of data buffered
locally.

1. The sink operates as before, where objects are created based only
   on the existing batch settings. This will initially be the default
   configuration to avoid breaking existing setups but will be
   deprecated.
1. The sink buffers batches until it can generate a complete part, and
   assembles the parts into an object only at the end of an
   interval. None of the intermediate parts are visible until the
   final object is assembled.

This change will introduce the following new configuration items for
this sink, which will all default to operating the same as the
existing sink.

- `data_dir` (string): The path to the directory in which the state
  data is stored. If this is not set, it is derived from the global
  `data_dir` setting.
- `batch.min_bytes` (integer): This specifies the target size a
  locally-stored part must reach before it is uploaded. Due to the
  limitations of the S3 API, this has an minimum value of 5MB, which
  is also the default value when `upload_interval` is set.
- `upload_interval` (string): When should the sink
  complete the upload process and initiate a new one. If this is
  unset, the existing batch-based object creation mechanism is
  used. the following values are permitted:
  - `"hourly"`: Complete the upload at the end of the hour exactly.
  - `"daily"`: Complete the upload at midnight local time each day.
  - `"at TIME"`: Complete the upload at a specific time each day.

### Implementation

#### Convert the S3 sink to use a new partitioned `Batcher`

The `Batcher` interface, used by a select few new-style stream sinks,
is a more generic batch collection setup. It provides for the
semantics required for this implementation, where events must be
encoded before batching in order to calculate their byte
size. However, it does not support partitioning, which is only
provided by the `PartitionBatcher` which does not support encoded size
limiting. We will need to create a new partitioned batcher combining
these two capabilities.

```rust
pub struct PartitionBatcher<S, C, P> {
    state: C,
    stream: Fuse<S>,
    partitioner: P,
    timer: Maybe<Sleep>,
}

impl<S, C, P> Stream for PartitionBatcher<S, C, P>
where
    S: Stream<Item = P::Item>,
    C: BatchConfig<S::Item>,
    P: Partitioner,
{
    type Item = (Prt::Key, C::Batch);
}
```

The S3 sink will then be converted to use the new partitioned batcher,
with the additional behavior change of encoding events before batching
rather than after.

#### Add a minimum batch size limit to the batcher

Since the multipart object creation API requires each part to have a
minimum size, this batcher will also have to enforce a minimum size
before an upload is initiated. The `Batcher` interface will be
modified to respect this interface and not emit batches until one is
ready (except at shutdown).

```rust
trait BatchConfig<T> {
    …
    /// Returns true if it the batch is full enough to send.
    fn is_batch_ready(&self) -> bool;
}

struct BatcherSettings {
    …
    pub size_minimum: usize,
}

trait SinkBatchSettings {
    …
    const MIN_BYTES: Option<usize>;
}

struct BatchConfig<D: SinkBatchSettings, S = Unmerged> {
    …
    pub min_bytes: Option<usize>,
}
```

#### Modify the S3 sink to use multi-part upload by default

As described above, the S3 sink will require additional configuration
items to support the new upload mode:

```rust
enum UploadInterval {
    Hourly,
    Daily,
    DailyAt(LocalTime),
}

struct S3SinkConfig {
    …
    pub data_dir: Option<PathBuf>,
    #[serde(deserialize_with = "upload_interval_or_duration")]
    pub upload_interval: Option<UploadInterval>,
}
```

When `upload_interval` is `Some`, `S3SinkConfig::build_processor` will
produce a sink result using all the above features; when `None`, it
will use the existing code paths.

In addition to saving and reloading buffers, this will require a
upload state tracker to allow resuming multipart uploads across
restarts. There will be one saved state for each output partition, and
the identifiers must be deleted when the final object is created. The
interface is a simplified form of what is used in the `file-source`
library with the unused bits removed.

```rust
pub struct Checkpointer<K, V> {
    tmp_file_path: PathBuf,
    stable_file_path: PathBuf,
    checkpoints: Arc<CheckpointsView>,
}

pub struct CheckpointsView {
    checkpoints: DashMap<MultipartUploadKey>,
}

type MultipartUploadKey = String;

/// Multipart upload state data
struct MultipartUploadData {
    /// The identifier for the upload process.
    upload_id: String,
    /// The time after which this batch will be completed.
    completion_time: DateTime<UTC>,
    /// A list of uploaded parts e_tags. The part number is derived from the index.
    parts: Vec<String>,
}
```

## Drawbacks

- By increasing the time during which events are not visible on the
  destination store, this will increase the chances of data being
  inaccessable due to crashes and other interruptions. This is not
  strictly data loss, as the uploaded data is recoverable by other
  means.

## Prior Art

The [S3 output plugin](https://docs.fluentd.org/output/s3) for fluentd
creates one upload per hour by default. It uses a disk buffer to store
the object before uploading.

## Alternatives

### Buffer the full large object locally

Instead of using the multipart upload API, Vector could save the data
that would compose the final object into a local disk buffer, and then
upload it in the background. This has several disadvantages:

1. It requires local storage for the complete object.
2. It creates complications for handling upload failures, as
   acknowledgements for that upload are disconnected from any source
   data.

### Upload parts as regular objects

Vector could also upload the individual batches as regular objects and
then go back and assemble the result into a larger object after all
the uploads are complete. Much of the proposal described above would
be the same, but the ordering would be changed and an additional step
is required:

1. Upload batches as regular objects, making note of object names.
2. When the final object is ready, create it with
   `CreateMultipartUpload`.
3. Add the previous objects using `UploadPartCopy`.
4. Assemble the final object with `CompleteMultipartUpload`.
5. Delete the intermediate objects that were assembled into the final
   object.

The downside with this scheme is the additional complexity of tracking
the intermediate object names and the deletion step at the end of the
process, both of which are handled automatically when uploading parts
as above. Additionally, processes that monitor the destination bucket
would end up seeing the events twice.

### Use the multipart upload copy to effect append behavior

One of the actions possible when creating a multipart object is to
copy data from an existing object. This can be used to both copy data
from and replace an object at the same time, allowing for effective
append (or prepend) behavior without using extra bandwidth. It is
unclear, however, if this behavior will interact badly with object
tracking sources like SQS, or if AWS would penalize this action
pattern for high volume uploads. As above, the data would end up being
visible multiple times.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Set up the new partitioned batcher.
- [ ] Initial MVP implementation in S3 with only the simplest upload
      interval and no persistent buffers.
- [ ] Add support for all upload interval types.

## Future Improvements

Since the AWS S3 multipart upload API only allows for a maximum of
10,000 parts per object, we could have configurations that end up
overflowing the effective maximum multipart object size. The above
scheme handles the overflow by creating multiple objects for an
interval that overflows. We could improve this experience with the
`UploadPartCopy` action, which takes a reference to an existing object
to include in the new object. With this action, we could assemble
these overflowed objects into a larger object at the end of an
interval.

The Azure Blob Store and GCP Cloud Storage also provide a multipart
upload API that is at least broadly compatible with the AWS S3
multipart scheme. It is likely that adding support for multipart
upload to these sinks would be able to use some of the infrastructure
described here.

With this implementation, Vector will now have two independent
multi-state checkpointer systems (file server and S3 sink) along with
at least one simpler checkpointer (journald source). Some
consideration should be given to unifying these internal state
mechanisms.
