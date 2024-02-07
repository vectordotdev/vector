Let's write a basic sink for Vector. Currently, there are two styles of sink in
Vector - 'event' and 'event streams'. The 'event' style sinks are deprecated,
but currently a significant portion of Vector's sinks are still developed in
this style. A tracking issue that covers which sinks have been converted to
'event streams' can be found [here][event_streams_tracking].

This tutorial covers writing an 'event stream' Sink.

Create a new rust module in `src/sinks/` called `basic.rs`.

# Doc comments

Provide some module level comments to explain what the sink does.

```rust
//! `Basic` sink.
//! A sink that will send it's output to standard out for pedagogical purposes.
```

# Imports

Let's setup all the imports we will need for the tutorial:

```rust
use crate::sinks::prelude::*;
use vector_lib::internal_event::{
    ByteSize, BytesSent, EventsSent, InternalEventHandle, Output, Protocol,
};
```

# Configuration

The first step when developing a Sink is to create a struct that represents the
configuration for that sink. The configuration file passed to Vector on startup
is deserialized to the fields in this struct so the user can customise the
sink's behaviour.

```rust
#[configurable_component(sink("basic"))]
#[derive(Clone, Debug)]
/// A basic sink that dumps its output to stdout.
pub struct BasicConfig {
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}
```

Note the [`configurable_component`][configurable_component] attribute. This
is used by Vector to generate documentation from the struct. To do this, doc
comments must be included above the struct - Vector won't compile if they
aren't.

We also include a single member in our struct - `acknowledgements`.  This
struct configures end-to-end acknowledgements for the sink, which is the ability
for the sink to inform the upstream sources if the event has been successfully
delivered. See Vector's [documentation][acknowledgements] for more details. We
will make this a configurable option.

Next we want to implement the [`GenerateConfig`][generate_config] trait for
our struct:

```rust
impl GenerateConfig for BasicConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}
```

This is used by the `vector generate` command to generate a default
configuration for the sink.

# SinkConfig

We need to implement the [`SinkConfig`][sink_config] trait. This is used by
Vector to generate the main Sink from the configuration. Note that type name
given to `typetag` below must match the name of the configurable component above.

```rust
#[async_trait::async_trait]
#[typetag::serde(name = "basic")]
impl SinkConfig for BasicConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let healthcheck = Box::pin(async move { Ok(()) });
        let sink = VectorSink::from_event_streamsink(BasicSink);

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
```

## The `build` function

Of particular importance is the [`build`][sink_config_build] function. This is
an async function that builds two components of the sink.

First, the healthcheck is an async block that can be used to check the health
of the service we are connecting to. In this very simple case we are just
outputting to the console which we assume will work, so the healthcheck returns
`Ok(())` indicating our target is healthy.

The actual work for this sink is done in `BasicSink` (to be implemented
shortly). The `build` function converts this into a [`VectorSink`][vector_sink]
via [`VectorSink::from_event_streamsink`][from_eventstreamsink] and returns it.


## BasicSink

Lets implement `BasicSink`.

```rust
struct BasicSink;
```

Our sink is so basic it has no properties to determine it's behaviour.

For it to work with Vector it must implement the [`StreamSink`][stream_sink]
trait:

```rust
#[async_trait::async_trait]
impl StreamSink<Event> for BasicSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
```

`StreamSink` is an async trait with a single async function: `run`. The main
parameter to this function, `input` is a stream of the events that are being
sent to this sink. We pull from this stream to send the events on to our
destination.

In order to handle lifetime issues that arise from using  [`async_trait`]
(https://docs.rs/async-trait/latest/async_trait/), this function simply calls
another method `run_inner` that is implemented directly on `BasicSink`.

Let's look an `run_inner`:

```rust
impl BasicSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            println!("{:?}", event);
        }

        Ok(())
    }
}
```

Our sink simply pulls each event from the input stream and prints the debug
representation of the object.

# Importing to Vector

## Feature flag

Each sink is kept behind a feature flag which allows copies of Vector to be
build with just the components required. We need to add this feature to the
`Cargo.toml`.

```diff
  sinks-azure_blob = ["dep:azure_core", "dep:azure_identity", "dep:azure_storage", "dep:azure_storage_blobs"]
  sinks-azure_monitor_logs = []
+ sinks-basic = []
  sinks-blackhole = []
  sinks-chronicle = []
```

Add it to our list of log sinks:

```diff
sinks-logs = [
  "sinks-amqp",
  "sinks-apex",
  "sinks-aws_cloudwatch_logs",
  "sinks-aws_kinesis_firehose",
  "sinks-aws_kinesis_streams",
  "sinks-aws_s3",
  "sinks-aws_sqs",
  "sinks-axiom",
  "sinks-azure_blob",
  "sinks-azure_monitor_logs",
+ "sinks-basic",
  "sinks-blackhole",
  "sinks-chronicle",
```

# Acknowledgements

When our sink finishes processing the event, it needs to acknowledge this so
that this can be passed back to the source.

We need to make a couple of changes to our `run_inner` function:

```diff
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
-       while let Some(event) = input.next().await {
+       while let Some(mut event) = input.next().await {
            println!("{:#?}", event);

+           let finalizers = event.take_finalizers();
+           finalizers.update_status(EventStatus::Delivered);
        }

        Ok(())
    }
```

First we need to make `event` mutable so that we can update the events status when
it is delivered.

Next we access the events finalizers with the `take_finalizers` function. We
then update the status with [`EventStatus::Delivered`][event_status_delivered]
to indicate the event has been delivered successfully.

If there had been an error whilst delivering the event, but the error was not a
permanent error, we would update the status with [`EventStatus::Errored`][event_status_errored]. Vector
will attempt to redeliver this event again.

If the error was a permanent one that would never work no matter how many times
we retry delivery, we update the status with [`EventStatus::Rejected`][event_status_rejected].

# Emitting internal events

Vector should be observable. It emit events about how it is running so users
can introspect its state to allow users to determine how healthy it is running.
Our sink must emit some metric when an event has been delivered to update the
count of how many events have been delivered.

There are two events that need to be emitted by the component.

## BytesSent

[`BytesSent`][bytes_sent] instruments how many bytes the sink is sending downstream.

First we need to get the number of bytes that we are sending. Then we need to
emit the event. Change the body of `run_inner` to look like the following:

```diff
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
+       let bytes_sent = register!(BytesSent::from(Protocol("console".into(),)));

        while let Some(mut event) = input.next().await {
+           let bytes = format!("{:#?}", event);
+           println!("{}", bytes);
-           println!("{:#?}", event);
+           bytes_sent.emit(ByteSize(bytes.len()));

            let finalizers = event.take_finalizers();
            finalizers.update_status(EventStatus::Delivered);
        }

        Ok(())
    }
```

## EventSent

[`EventSent`][events_sent] is emitted by each component in Vector to
instrument how many bytes have been sent to the next downstream component.

Change the body of `run_inner` to look like the following:

```diff
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let bytes_sent = register!(BytesSent::from(Protocol("console".into(),)));
+       let events_sent = register!(EventsSent::from(Output(None)));

        while let Some(mut event) = input.next().await {
            let bytes = format!("{:#?}", event);
            println!("{}", bytes);
            bytes_sent.emit(ByteSize(bytes.len()));

+           let event_byte_size = event.estimated_json_encoded_size_of();
+           events_sent.emit(CountByteSize(1, event_byte_size));

            let finalizers = event.take_finalizers();
            finalizers.update_status(EventStatus::Delivered);
        }

        Ok(())
    }
```

More details about instrumenting Vector can be found
[here](https://github.com/vectordotdev/vector/blob/master/docs/specs/instrumentation.md).

# Running our sink

Let's run our sink. Create the following Vector configuration in `./basic.yml`:

```yml
sources:
  stdin:
    type: stdin

sinks:
  basic:
    type: basic
    inputs:
      - stdin
```

This simply connects a `stdin` source to our `basic` sink.

## vdev

Vector provides a build tool `vdev` that simplifies the task of building Vector. Install
`vdev` using the instructions [here][vdev_install].

With `vdev` installed we can run Vector using:

```sh
cargo vdev run ./basic.yml
```

This uses the config file to detect and set the relevant features to build Vector with.

Without `vdev`, we can run using:

```sh
cargo run --no-default-features --features "sources-stdin, sinks-basic" -- -c ./basic.yml
```

Type some text into the terminal and Vector should output the Debug information
for the log event.

Our sink works!


[event_streams_tracking]: https://github.com/vectordotdev/vector/issues/9261
[vdev_install]: https://github.com/vectordotdev/vector/tree/master/vdev#installation
[acknowledgements]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
[configurable_component]: https://rust-doc.vector.dev/vector_config/attr.configurable_component.html
[generate_config]: https://rust-doc.vector.dev/vector/config/trait.generateconfig
[sink_config]: https://rust-doc.vector.dev/vector/config/trait.sinkconfig
[sink_config_build]: https://rust-doc.vector.dev/vector/config/trait.sinkconfig#tymethod.build
[from_eventstreamsink]: https://rust-doc.vector.dev/vector/sinks/enum.vectorsink#method.from_event_streamsink
[vector_sink]: https://rust-doc.vector.dev/vector/sinks/enum.vectorsink
[stream_sink]: https://rust-doc.vector.dev/vector/sinks/util/trait.streamsink
[sinks_enum]: https://rust-doc.vector.dev/vector/sinks/enum.sinks
[event_status_delivered]: https://rust-doc.vector.dev/vector/event/enum.eventstatus#variant.Delivered
[event_status_errored]: https://rust-doc.vector.dev/vector/event/enum.eventstatus#variant.Errored
[event_status_rejected]: https://rust-doc.vector.dev/vector/event/enum.eventstatus#variant.Rejected
[bytes_sent]: https://rust-doc.vector.dev/vector_common/internal_event/struct.bytessent
[events_sent]: https://rust-doc.vector.dev/vector_common/internal_event/struct.eventssent
