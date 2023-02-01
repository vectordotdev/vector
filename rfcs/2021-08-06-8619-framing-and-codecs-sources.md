# RFC 8619 - 2021-08-06 - Framing and Codecs - Sources

This RFC is part of a series to discuss a standardized way of specifying framing
and codecs across sources and sinks. _Framing_ is concerned with turning a
stream of bytes into byte frames (chunks of data that have finite size and
contain complete messages), while a _Codec_ converts between a byte frame and
structured data.

Conceptually, we want reusable pieces of logic which would allow us to collapse
a component + transform into a component with codec config in places where the
transform was merely used to convert between encoding formats, e.g. from `bytes`
to `json` or vice versa.

## Scope

For this part, we focus on the source/decoding side and leave the sink/encoding
side to a separate RFC.

The scope of this RFC concerns at which level framing and decoding operates, how
these framers and decoders can be configured, and how they can be shared in a
uniform way. It does not cover any specific implementation for a framer or a
decoder. Also see [future work](#future-work) for extended goals that are not
covered by this RFC.

## Motivation

Currently, we have no explicit abstraction that is responsible for handling
framing and decoding, such that each source may implement these in their own
way. This has led to inconsistencies in available options and poses additional
maintenance burden. Components have their own defaults, making
behavior unpredictable and surprising users, e.g. as documented in
[#3453](https://github.com/vectordotdev/vector/issues/3453).

## Internal Proposal

To expose framing and decoding to sources in a uniform way, we specify a common
configuration struct:

```rust
pub struct DecodingConfig {
    framing: Option<FramingConfig>,
    decoding: Option<ParserConfig>,
}
```

while the framing method and parser used to deserialize into a structured event
are selected via `typetag`ed traits:

```rust
pub type BoxedFramer = Box<dyn Framer + Send + Sync>;

pub type BoxedParser = Box<dyn Parser + Send + Sync + 'static>;

#[typetag::serde(tag = "method")]
pub trait FramingConfig: Debug + DynClone + Send + Sync {
    fn build(&self) -> BoxedFramer;
}

#[typetag::serde(tag = "codec")]
pub trait ParserConfig: Debug + DynClone + Send + Sync {
    fn build(&self) -> BoxedParser;
}
```

The `DecodingConfig` exposes a `build` method to create a Vector `Decoder` which
implements the `tokio_util::codec::Decoder` trait with
`Item = (SmallVec<[Event; 1]>, usize)`. That way we can produce `Event`s from
`ByteMut` by repeated calls to `decode`/`decode_eof` either on a byte stream or
a byte message. The additional `usize` item conveys how many bytes were read to
produce the particular `SmallVec<[Event; 1]>`, so that this information can be
passed along to e.g. the internal event log.

Internally, the `Decoder` holds a framer and a parser:

```rust
#[derive(Clone)]
pub struct Decoder {
    framer: BoxedFramer,
    parser: BoxedParser,
}
```

with the `Framer` trait being defined as:

```rust
pub trait Framer: tokio_util::codec::Decoder<Item = Bytes, Error = BoxedFramingError> + DynClone + Send + Sync {}
```

and the `Parser` trait being defined as:

```rust
pub trait Parser: DynClone + Send + Sync {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>>;
}
```

Ideally, implementations for `Parser` can be shared/derived from VRL's `parse_*`
functions.

The `Decoder` calls its framer repeatedly to produce byte frames, then calls the
parser to create an `SmallVec<[Event; 1]>` and returns. It returns a `SmallVec`
rather than an `Event` directly, since one byte frame can potentially hold
multiple events, e.g. when parsing a JSON array. However, we optimize the most
common case of emitting one event by not requiring heap allocations for it.

Sources which want to expose framing/decoding functionality to the user can
embed `DecodingConfig` in their config, build the `Decoder` and apply it to the
sequences of bytes they produce to create events.

## Doc-level Proposal

The `framing` and `decoding` behavior can be configured for a source in the
vector config, e.g.:

```yaml
[framing]
method = "character_delimited"
character_delimited.delimiter = "\t"

[decoding]
codec = "json"
```

which would transform the input of

```text
"{ \"foo\": 1 }\t{ \"bar\": 2 }\t{ \"baz\": 3 }"
```

to


```text
{ "foo": 1 }
{ "bar": 2 }
{ "baz": 3 }
```

## Rationale

One often request feature is reading JSON-encoded messages from Kafka.
Currently, this can only be accomplished by configuring and wiring together two
separate components, where with codecs it could be a convenient one-line change
to the config.

Introducing framing/decoding, a source's implementation may also be reused
internally. One example would be the `syslog` source (see
[#7046](https://github.com/vectordotdev/vector/pull/7046)), or the upcoming `syslog`
sink in [#7106](https://github.com/vectordotdev/vector/issues/7106). Instead of
re-implementing socket-based connection handling, the `syslog` components could
be replaced by the `socket` counterparts combined with `octet-framing`. This
reduces a possible source of bugs and inconsistencies and therefore leads to
less maintenance burden.

Introducing codecs may also shrink unnecessary noise in config files by removing
transform steps / input indirections, when basic transforms were used that are
only concerned with encoding formats.

## Prior Art

[Tokio Codecs](https://docs.rs/tokio-util/0.6.7/tokio_util/codec/index.html)
provide traits to conveniently convert from `AsyncRead`/`AsyncWrite` to
`Stream`/`Sink`. These are currently used in custom implementations of sources
to frame byte streams. We want to extend this existing facility to produce
events from bytes directly.

[Logstash Codec Plugins](https://www.elastic.co/guide/en/logstash/current/codec-plugins.html)
are interesting since they operate on a higher level than what has been proposed
in this RFC. They don't distinguish between a framing and codec stage, but
rather have codecs that support framing (e.g. `line` codec), compression (e.g.
`gzip_lines` codec) and encoding (e.g. `protobuf` codec). Supporting these kind
of codecs could be an interesting future thought but would require an
architectural change, especially to the internal representation of an `Event`.

## Drawbacks

It is possible that the proposed abstraction is too rigid, in a sense that it is
not possible to cleanly separate these stages into "framing" and "codec"
responsibilities. This limits what codecs can do, e.g. applying framing after
decoding/decompression would not be possible as there are no means to "go back"
to the framing stage.

However, the proposed solution is still strictly better than the status quo, as
it provides a consistent interface. The proposed changes don't conflict with
building more general codecs on the topology level in the future. The decoders
could be wrapped in a transform-like structure if we recognize a demand for this
feature.

Alternatively, introducing a `remap` codec could give users enough flexibility
to express their data transformation needs in a source:

```toml
[decoding]
codec = "remap"
src = """
. = parse_json!(.)
.nested = parse_json!(.nested)
.encoded = parse_base64!(.encoded)
"""
```

## Alternatives Considered

In a previous iteration of this RFC, codecs were implemented on the topology
level and it was possible to chain multiple codecs to cover a wide range of
scenarios, e.g. going from TCP stream -> decompress using `gzip` -> split by
newline -> byte frame -> `json`.

The advantage of that approach would be an easy mental model for the user since
codecs would work for every source, and the flexibility of composing codecs.
However, each codec implementation would need to accept any value (bytes,
string, object, ...) and the internal event would need to represent a state for
raw bytes / an unfinished event, meaning that the complexity to represent these
combinations would propagate throughout the system.

Since that approach would also allow users to configure senseless combinations
of codecs, we decided to restrict this feature to going from bytes to `Event`
through one framing and parsing method, which covers most use cases for which
the framing/codec feature was requested.

## Future Work

In this first release, we plan to implement a variety of framing and codec
options that cover the most common use cases. However, users might want to use a
custom codec that only applies to their specific use case or is not supported by
Vector yet. Adding a system that would allow custom-defined codecs could be
considered in the future, for now this is can be accomplished by the `wasm` or
`lua` transform, or alternatively by the proposed `remap` codec.

## Plan Of Attack

- Implement common configuration object
- Implement common `Decoder` and build methods from configs
- Integrate `DecodingConfig` into sources that expose framing/decoding to users:
  - `file` source
  - `kafka` source
  - `socket` (TCP, UDP, Unix) source
  - `stdin` source
- Reuse `Decoder` for sources that internally build an `Event` from bytes
  - `fluent` source
  - `logstash` source
  - `statsd` source
  - `syslog` source
  - `vector` source
