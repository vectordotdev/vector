# RFC 7351 - 2021-05-05 - Framing and Codecs

This RFC discusses a standardized way of specifying framing and codecs across
sources and sinks. _Framing_ is concerned with turning sequences of bytes into
byte frames (which indicate the boundaries of a complete message). Codecs
consist of _decoders_, which deserialize a byte frame into structured data, and
_encoders_, which serialize structured data into a byte frame.

Conceptually, we want reusable pieces of logic which would allow us to collapse
a source + (decoder) transform into a source with decoder config, and to
collapse an (encoder) transform + sink into a sink with encoder config, in a way
that is transparent to the user. These transforms merely convert between
encoding formats, e.g. from `bytes` to `json` and vice versa.

## Scope

The scope of this RFC concern at which level framing and decoding operates, how
these framers and codecs can be configured, and how they can be shared in a
uniform way. It does not cover any specific implementation for framing or a
codec. Also see [future work](#future-work) for extended goals that are not
covered by this RFC.

## Motivation

Currently, we have no explicit abstraction that is responsible for handling
framing and codecs, such that each source and sink may implement them in their
own way. This can introduce inconsistencies and additional maintenance burden.
Components have their own defaults, making behavior unpredictable and surprising
users, e.g. as documented in
[#3453](https://github.com/timberio/vector/issues/3453).

## Internal Proposal

Since decoding and encoding should apply universally to all sources and sinks,
it should be configurable by a shared field. This is accomplished by the
`SourceOuter` and `SinkOuter` wrappers:

```rust
// Newly introduced wrapper, analogously to `SinkOuter`.
struct SourceOuter {
    framing: Framer,
    decoding: Decoder,
}
```

```rust
struct SinkOuter {
    // … existing fields
    encoding: Encoder,
}
```

Conceptually, decoding is applied _after_ a source has processed an event and
encoding is applied _before_ a sink has processed an event. These can be
implemented as `FunctionTransform`s in our pipeline, in the same way transforms
are implemented.

Framing takes place before an event is created since the message boundary
required to form an event is not known yet. Therefore the source needs to be
aware of framing and call the framer to determine message boundaries before
creating an event.

To make the source aware of framing, the `Framer` needs to be passed to the
`SourceContext`, so that the source implementation can call it:

```rust
struct SourceContext {
    // … existing fields
    framing: Framer,
}
```

In the context of Vector, decoders and encoders are implemented as `Transform`.
That is, they either implement the `TaskTransform` or `FunctionTransform` trait.

Implementing framers as `Transform` would be possible - however, we want to
restrict them to `FunctionTransform` at the moment. The reason here is that
`TaskTransform`s can take any amount of input bytes before they output frames.
If we would allow that, it wouldn't be clear how we merge metadata from multiple
packages that compose a frame, e.g. the sender in UDP datagrams or partition key
and offset in kafka messages. Using `FunctionTransform`, we know that a 1:n
relationship between byte chunks and frames exist, and we can simply duplicate
the metadata for each frame.

## Doc-level Proposal

// TODO.

<!--
- Optional. Only do this if your change is public facing.
- Demonstrate how your change will look in the form of Vector's public docs.
-->

## Rationale

One prime example where a source's implementation may be reused with a different
codec is the `syslog` source (see
[#7046](https://github.com/timberio/vector/pull/7046)), or the upcoming `syslog`
sink in [#7106](https://github.com/timberio/vector/issues/7106). Instead of
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
to frame byte streams. However, these codecs can only operate on byte input and
would therefore be unsuitable to implement codecs that can be chained.

[Logstash Codec Plugins](https://www.elastic.co/guide/en/logstash/current/codec-plugins.html)
are interesting since they operate on a higher level than what has been proposed
in this RFC. They don't distinguish between a framing and codec stage, but
rather have codecs that support framing (e.g. `line` codec), compression (e.g.
`gzip_lines` codec) and encoding (e.g. `protobuf` codec). Supporting these kind
of codecs could be an interesting future thought but would require an
architectural change, especially to the internal representation of an `Event`.

## Drawbacks

// TODO.

- Is this the right abstraction, is it possible to cleanly separate at these
  boundaries for all our current and future use cases?
- For the components that we expect benefit most from reusability enabled by
  separating, will they always only differ by encoding in the future?
- Does it hurt discoverability (e.g. predefined `syslog` source vs `socket` +
  `octet-framing`)?

<!--
- Why should we not do this?
- What kind on ongoing burden does this place on the team?
-->

## Alternatives

// TODO.

<!--
- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?
-->

## Outstanding Questions

Is there a realistic chance that multiple framing / codec options might need be
applied / composed?

Now that we establish a pattern for encoding, do we want to make the distinction
internally when we have bytes or (e.g. UTF-8) strings at hand? Currently we are
just relying on the next component to handle possibly invalid encodings.

## Future Work

In this first release, we plan to implement a variety of framing and codec
options that cover the most common use cases. However, users might want to use a
custom codec that only applies to their specific use case or is not supported by
Vector yet. Adding a system that would allow custom-defined codecs could be
considered in the future, for now this is can be accomplished by the `wasm` or
`lua` transform.

Codecs are very close to transform steps. When looking at e.g. the existing
`json` transform, it is apparent that it can be further configured. In this
initial design, we will not allow any configuration and choose common defaults.

## Plan Of Attack

// TODO.

<!--
Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.
-->
