# RFC 3829 - 2022-03-07 - Better Support for Large AWS S3 Batches

This RFC discusses improving vector's support for sending large batches to AWS S3.

## Context

- [Better support for large S3 batches, Issue
  #3829](https://github.com/vectordotdev/vector/issues/3829)

## Terminology

- Batch: A sequence of events collected within vector in preparation
  for uploading to a storage sink.
- Object: A completed upload stored on the destination sink's server(s).
- Part: A completed partial upload that will be composed into an object.

## Scope

### In scope

- Improving Vector's support for sending large batches to the AWS S3 sink.

### Out of scope

- Support for large batches in the Azure Blob, Datadog Archive, and
  GCP Cloud Storage sinks.
- Changing the criteria controlling when an S3 object is created.

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
   need to track these identifiers with the partition that initiated
   the upload.
2. The upload parts (except for the final part) have a _minimum_ size
   limit of 5MB. Since the sink may need to be restarted before a
   batch reaches this minimum size, vector will need to complete the
   multipart object when shutting down.

In the case of an interruption in the shutdown process or a crash, the
destination bucket will be left with incomplete multipart uploads and
parts. To recover from this, Vector will optionally scan for such
parts at startup and assemble them to their original object
names. That process works as follows:

1. The existing incomplete uploads are listed with the
   `ListMultipartUploads` action.
2. Included in the listing are the object names the uploads would be
   written to. The `key_prefix` in the configuration is translated
   into a pattern, and these object names are filtered to match only
   those that could have come from the current configuration.
3. For each resulting multipart upload, the parts that compose it are
   listed with one or more `ListParts` actions.
4. The upload assembled to the final object with the
   `CompleteMultipartUpload` action as above.

In the case of multiple Vector instances writing into the same bucket
with the same `key_prefix` where one restarts, the above process could
trigger the reassembly of a multipart upload that is still being
written to by another instance. When the other instance(s) try to
upload a subsequent part, S3 will report that the multipart upload can
no longer be found. When encountering this error, the writing instance
will discard its previous multipart data and retry after creating a
new upload.

### User Experience

From a user point of view, there is no visible change in the behavior
under normal conditions. Objects are created as specified by the
`batch` configuration, either when the maximum size or a timeout is
reached. Internally, the sink will upload the objects in multiple
parts when possible, but the resulting object will not be visible in
the bucket until all the parts of it are complete. When possible, the
sink will fall back to the `PutObject` path to reduce the number of
AWS actions.

This change will introduce the following new configuration items for
this sink, which will all default to operating the same as the
existing sink.

- `batch.min_part_bytes` (integer): This specifies the target size a
  locally-stored part must reach before it is uploaded. Due to the
  limitations of the S3 API, this has an minimum value of 5MB, which
  is also the default value when `upload_interval` is set. If this is
  larger or equal to `batch.max_bytes`, the batches will always be
  uploaded as single objects.
- `recover_partials` (bool): When set, vector will scan for unfinished
  objects at startup and reassemble them. These are multipart uploads
  that are interrupted by a crash.

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
    pub min_part_bytes: Option<usize>,
}
```

#### Modify the S3 sink to use multi-part upload by default

In addition to above, the S3 sink will require one additional
configuration item to support the new upload mode:

```rust
struct S3SinkConfig {
    …
    pub recover_partials: bool,
}
```

The upload system will be modified to upload batches as parts and then
assembled once the maximum size or timeout is reached.  This will
require a upload state tracker to contain the data required to
assemble the parts when the object is completed. There will be one
saved state for each output partition, and the identifiers must be
deleted when the final object is created.

```rust
struct MultipartUploads {
    uploads: DashMap<String, MultipartUpload>,
}

/// Multipart upload state data
struct MultipartUpload {
    /// The identifier for the upload process.
    upload_id: String,
    /// The time after which this batch will be completed.
    completion_time: DateTime<UTC>,
    /// A list of uploaded parts.
    parts: Vec<MultipartUploadPart>,
}

struct UploadPart {
    number: i64,
    e_tag: String,
}
```

## Drawbacks

- While this proposal does not change the timing of objects being
  created in the bucket, it does increase the number of AWS actions
  required to create an object. This may slightly increase costs for
  users.
- This multipart scheme raises the possibility of leaving unusable
  data in the S3 bucket due to incomplete multipart uploads. This data
  is recoverable, but only with an extra processing step.

## Prior Art

The [S3 output plugin](https://docs.fluentd.org/output/s3) for fluentd
uses a disk buffer to store the object before uploading, but uses the
standard `PutObject` action to upload the object.

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
- [ ] Initial support for multi-part upload in the S3 sink.
- [ ] Add support for optimizing single-part objects.
- [ ] Add support for partial upload recovery.

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

This proposal makes no changes to timing of object creation. With the
multipart upload mechanism, there is the opportunity to change the
object creation time to an interval much larger than the current batch
timeouts, such as on the hour or similar. This kind of enhancement
would also be useful in the `file` sink.

There is also the opportunity to allow for object creation to span a
single run of vector. That is, the final object would not necessarily
be created at shutdown but would be tracked and completed during the
next run. This would require the use of a disk-based buffer, either a
specialized batch buffer in the sink or disk buffer before the sink.
