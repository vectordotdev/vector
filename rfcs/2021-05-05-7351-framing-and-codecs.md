# RFC 7351 - 2021-05-05 - Framing and Codecs

This RFC discusses a standardized way of specifying framing and codecs across sources and sinks. _Framing_ is concerned with turning sequences of bytes into byte frames. Codecs consist of _decoders_, which deserialize a byte frame into structured data, and _encoders_, which serialize structured data into a byte frame.

Conceptually, we want reusable pieces of logic which would allow us to collapse a source + (decoder) transform into a source with decoder config, and to collapse an (encoder) transform + sink into a sink with enocoder config, in a way that is transparent to the user. These transforms merely convert between encoding formats, e.g. from `bytes` to `json` and vice versa.

## Scope

The scope of this RFC concern at which level framing and decoding operates, how these framers and codecs can be configured, and how they can be shared in a uniform way. It does not cover any specific implementation for framing or any codec.

## Motivation

Currently, we have no explicit abstraction that is responsible for handling framing and codecs, such that each source and sink may implement them in their own way. Components have their own defaults, making behavior unpredictable and surprising users, e.g. as documented in #3453.

## Internal Proposal

// TODO.

```rust
struct SourceOuter {
    // … existing fields …
    framing: Framing,
    decoding: Decoding,
}
```

```rust
struct SinkOuter {
    // … existing fields …
    encoding: Encoder,
}
```

- Framing and decoding is applied _after_ a source has processed an event.
- Encoding is applied _before_ a sink has processed an event.

<!--
- Describe your change as if you were presenting it to the Vector team.
- Use lists, examples, and code blocks for efficient reading.
- Be specific!
-->

## Doc-level Proposal

// TODO.

<!--
- Optional. Only do this if your change is public facing.
- Demonstrate how your change will look in the form of Vector's public docs.
-->

## Rationale

One prime example where a source's implementation may be reused with a different codec is the `syslog` source, or the upcoming `syslog` sink in #7106. Instead of reimplementing socket-based connection handling, the `syslog` components could be replaced by the `socket` counterparts combined with octet-framing.

// TODO.

<!--
- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?
-->

## Prior Art

// TODO.

- Tokio Codecs.

<!--
- List prior art, the good and bad.
- Why can't we simply use or copy them?
-->

## Drawbacks

// TODO.

- Is this the right abstraction, is it possible to cleanly separate at these boundaries for all our current and future use cases?
- For the components that we expect benefit most from reusability enabled by separating, will they always only differ by encoding in the future?
- Does it hurt discoverability (e.g. predefined `syslog` source vs `socket` + `octet-framing`)?

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

Is there a realistic chance that multiple framing / codec options might be applied?

Now that we establish a pattern for encoding, do we want to make the distinction internally when we have bytes or (e.g. UTF-8) strings at hand? Currently we are just relying on the next component to handle possibly invalid encodings.

<!--
- List any remaining questions that you have.
- These must be resolved before the RFC can be merged.
-->

## Future Work

// TODO.

User-provided custom codecs.

Codecs with options.

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
