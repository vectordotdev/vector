# RFC 13691 - 2022-07-28 - Registered Internal Events

Emission of named metrics, complete with all the required tags, is a relatively expensive process
both in terms of generating the metric data on the Vector side, and then determining which global
metric to update the new data to on the `metrics` crate side. Recent upgrades to the `metrics` crate
provide the opportunity to register a metric handle once and then update the data for that handle at
a later point. That update can frequently then be reduced to an indirect jump and atomic memory
operation. The `InternalEvent` trait, however, does not provide any way to split registration of an
internal event from the emission of the of that event.  This document proposes an approach to
solving that problem that can then be applied incrementally to the most expensive internal metrics.

## Context

- Custom registered metric in the `SourceSender` that should be an `InternalEvent`:
  https://github.com/vectordotdev/vector/pull/13611
- Performance boost from emitting events once per batch:
  https://github.com/vectordotdev/vector/pull/13739

## Scope

### In scope

- Creation of a base interface to standardize registering handles to metrics.
- Creation of registered events to replace key "hot" metrics.
- Incremental conversion of hot events to registered events.

### Out of scope

- Conversion of all existing internal events into registered events.

## Pain

- Emission of internal metrics is computationally expensive for hot events.

## Proposal

One of the largest, if not _the_ largest, performance cost of emitting internal metrics is creating
the metric identifier from scratch each time, and then looking it up in the global registry of
metrics in order to then adjust its value. This identifier, or key, consists of both the metric
name, which is a simple constant string, along with a set of labels, which will vary for each
emitting task. The two steps of this process thus involves numerous memory allocations, memory
copies, hashing, and at least one mutex locked operation.

This proposal provides a mechanism for registering a metric handle such that this computation can be
done once at task setup time, leaving only the relatively simple task of adjusting the metric to
happen in the hot path.

### User Experience

As an internal event mechanism, this should have no user impact besides making Vector more efficient
at recording the same internal observability data.

### Implementation

The current setup of internal events using a trait method on structures has the desirable property
that all parameters are named:

```rust
BytesSent {
    byte_size: 12345,
    protocol: "https",
}
.emit();
```

We include a common wrapper to make it look more functional. The function is also wrapped in a macro
which is conveniently imported everywhere in the main vector library (and coincidentally allows us
to do a little magic with event names in tests).

```rust
emit!(BytesSent {
    byte_size: 12345,
    protocol: "https",
});
```

To continue the same pattern, we will set up a new trait to register an event. As above, it will
consume the event data (and so take ownership of all the fields) and return the handle. This new
return type for the event handle will necessarily be named by the trait.

```rust
trait RegisterInternalEvent: Sized {
    type Handle: InternalEventHandle;
    fn register(self) -> Self::Handle;
}
```

Since we want to continue the functional conveniences of `emit`, we will also set up a function and
macro wrapper to provide the convenience:

```rust
fn register<T: RegisterInternalEvent>(event: E) -> E::Handle {
    event.register()
}

macro_rules! register {
    ($event:expr) => { vector_core::internal_event::register($event) };
}
```

We want the same test handling magic, so that will come included with its own wrapper:

```rust
struct DefaultHandleName<E> {
    pub name: &'static str,
    pub event: E,
}

#[cfg(test)]
macro_rules! register {
    ($event:expr) => {
        vector_core::internal_event::register(
            vector_core::internal_event::DefaultHandleName {
                event: $event,
                name: stringify!($event),
            }
        )
    };
}
```

The registered event handle will have its own trait for emitting the event, which enforces all such
events follow exactly the same pattern. The input data is consumed, just as it is for the existing
`emit` function.

```rust
trait InternalEventHandle {
    type Data;
    fn emit(&self, data: Data);
}
```

To assist with providing the necessary types for emitting the registered events, a set of common
unit structures will be defined for the required data:

```rust
struct ByteSize(usize);

struct ByteSizeCount(usize, usize);
```

Finally, here is a sample implementation of the above traits, taken from the existing
`EndpointBytesReceived` internal event:

```rust
use metrics::Counter;

struct RegisteredEndpointBytesReceived {
    bytes_total: Counter,
    protocol: &'static str,
    endpoint: String,
}

struct EndpointBytesReceivedHandle {
    bytes_total: Counter,
    protocol: &'static str,
    endpoint: String,
}

impl RegisterInternalEvent for RegisteredEndpointBytesReceived {
    type Handle = EndpointBytesReceivedHandle;

    fn register(self) -> Self::Handle {
        let bytes_total = counter!(
            "component_received_bytes_total",
            "protocol" => self.protocol,
            "endpoint" => self.endpoint.clone(),
        );
        Self {
            bytes_total,
            protocol: self.protocol,
            endpoint: self.endpoint,
        }
    }
}

impl InternalEventHandle for EndpointBytesReceivedHandle {
    type Data = ByteSize;
    fn emit(&self, data: ByteSize) {
        trace!(
            message = "Bytes received.",
            byte_size = %data.0,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
        self.bytes_total.add(data.0);
    }
}

// In component code:

use crate::internal_events::{InternalEventHandle, RegisteredEndpointBytesReceived};

let handle = register!(RegisteredEndpointBytesReceived {
    protocol = "https",
    endpoint = self.config.endpoint.clone(),
);

handle.emit(ByteSize(received.len()));
```

Storing this handle in a structure requires using either the internal handle name or using the
`Handle` data type:

```rust
struct RunningSource {
    bytes_sent_alt1: <RegisteredEndpointBytesSent as RegisterInternalEvent>::Handle,
    bytes_sent_alt2: EndpointBytesReceivedHandle,
}
```

## Rationale

- This interface for registered internal event handles is notionally equivalent to the existing
  `InternalEvent` interface, using field names and named types for all data.

## Drawbacks

- This increases the complexity of the internal event interface, presenting two different interfaces
  for internal events.

- Internal events that can be registered are necessarily distinct from existing events that can be
  simply emitted. That is, you cannot `register!(BytesSent { … })`, nor can you
  `emit!(RegisteredBytesSent { … })` (although the latter could be modified to allow
  `emit!(RegisteredBytesSent { … }, ByteSize(…))`).

- The syntax for naming the handle for storing it in structures is awkward. Writing a macro to
  handle this is probably overkill. This is particularly awkward if the registration struct uses a
  lifetime, since this then requires naming that lifetime to access the handle type, which in turn
  requires a named lifetime bound on the containing structure _even if the handle itself doesn't
  require one_. This can be avoided by using the handle name itself, but that requires additional
  knowledge of the internal event details.

## Alternatives

The simplest method of providing this is to just write new internal event structures that are
created through a struct method returning `Self`. This would allow us to name the handle more
simply, but have the downside of not allowing for naming the creation parameters and not enforcing
the register pattern at check/compile time on all such events.

```rust
pub struct BytesSentHandle { … }

impl BytesSentHandle {
    fn new(protocol: &str, etc: &str) -> Self { … }
}
```

Alternately, we could audit our metric usage and ensure that all hot metrics are emitted in batches
instead of per-event or equivalent. This is not easily determined just by examining the code,
requiring run-time analysis. Also, batching emission may not be straightforward neither.

## Plan Of Attack

- [ ] Implement the above interfaces along with a single common registered event (ie
      `RegisteredBytesSent`). Use this event in at least one place to ensure `check-events` and
      component tests pass.
- [ ] Convert all remaining users of `BytesSent` and drop the non-registered version.
- [ ] Convert `EventsReceived` to a registered event.
- [ ] Convert `EventsSent` to a registered event.
- [ ] Convert `BytesReceived` to a registered event.
- [ ] ...

## Future Improvements
