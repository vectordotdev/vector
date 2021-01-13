# RFC 5843 - 2021-01-08 - Encoding/Decoding for Vector to Vector Communication

There has been an ongoing discussion of what format Vector-to-Vector communications should take. For some examples of issues related to the subject check out #5809, #5124, or #5341. This RFC is intended to propose a format for standardization of our vector-to-vector communications.

## Scope

- This RFC should cover the encoding format we intend to leverage
- It should also cover any alterations that might need to be made to the expected data format

## Motivation

The question posed by @binarylogic in #5843 is a good definition for the motivation of this change: "How do we transmit data from one Vector instance to another while being mindful of the public contract we create?" Clearly sending metrics from one Vector instance to another should preserve all metric data upon ingestion downstream but there are still concerns around exposing a proprietary format to users in the context of these operations. This discussion continues to come up and we really need a canonincal and definitive decision around what shape this takes so that when we expand our components our contract is explicitly codified.

## Internal Proposal

From a high level my proposal is pretty simple. I'll dive into the rationale and context lower in the doc but lets just tee that up, shall we?

I propose that if we're not searching explicitly for better perf around these messages and we're not explicitly searching for alternate tooling because we've decided that we wholly can't deal with the tooling overhead of protobufs we instead continue to use them with prost. I would also posit that if the concerns around exposing our private data format are bothersome enough, we should consider following an open spec for the data as a primary example: [opentelemetry](https://opentelemetry.io/).

Should performance be a heavier concern for this standardization than I have assumed for the writing of this doc, my propsal would be to consider a highly performant alternative to protobufs like Flatbuffers and still consider following an open spec for the data. The `Rationale` section below will cover my thoughts on this more thoroughly.

## Doc-level Proposal

I'm fairly certain that this should not require a docs entry as it's largely internal to the tools. There is a possibility that we might want to document the data format we use if either to dissuade users from following it/using it (in the case that we don't adopt a standardized data format) or to point out that we've standardized on an open data spec that can be followed and used in other areas.

## Rationale

Unfortunately in most of my research I've pretty well come to the conclusion that encoding formats as a domain are rife with technical and political failures. That is to say, in some regard or another, every format from JSON to Flatbuffers is going to force a tradeoff between correctness, performance, features or tooling and maintenance overhead. Inlined below here is a reasonable starting point on performance. It's taken from the [NoProto project](https://https://github.com/only-cliches/NoProto) since they're regularly updating their benchmarks. They've also done an excellent job of enumerating some of the aspects to consider when making a decision of this nature and their read on the space very much mirrors my own thinking in many ways.

For the below data, Encodes and Decodes are ops/sec so higher is better while size before and after compression lower is better. This list is far from exhaustive and specifcially lacks a serde_json comparison which is unfortunate but other benches out there do include them (though obviously a different benchmark makes cross comparison difficult). 

| Library            | Encode | Decode All | Decode 1 | Update 1 | Size (bytes) | Size (Zlib) |
|--------------------|--------|------------|----------|----------|--------------|-------------|
| **Runtime Libs**   |        |            |          |          |              |             |
| *NoProto*          |   1057 |       1437 |    47619 |    12195 |          208 |         166 |
| Apache Avro        |    138 |         51 |       52 |       37 |          702 |         336 |
| FlexBuffers        |    401 |        855 |    23256 |      264 |          490 |         309 |
| JSON               |    550 |        438 |      544 |      396 |          439 |         184 |
| BSON               |    115 |        103 |      109 |       80 |          414 |         216 |
| MessagePack        |    135 |        222 |      237 |      119 |          296 |         187 |
| **Compiled Libs**  |        |            |          |          |              |             |
| Flatbuffers        |   1046 |      14706 |   250000 |     1065 |          264 |         181 |
| Bincode            |   5882 |       8772 |     9524 |     4016 |          163 |         129 |
| Protobuf           |    859 |       1140 |     1163 |      480 |          154 |         141 |
| Prost              |   1225 |       1866 |     1984 |      962 |          154 |         142 |

Performance is a single aspect of a nuanced problem for us; for more perf benchmarks I'd suggest checking out [rust-serialization-benchmarks](https://github.com/erickt/rust-serialization-benchmarks).  Some of these formats (JSON for example) still leaves us with a marginal concern around both dropped metadata as well as incidental exposure of a private data format to our users. The goal here is to hopefully solve for all of these things with whatever path is chosen.

While it is tempting to look at perf benchmarks and to just pick the best performing example, the practical requirements around the problem for us might give us some limitations. As was [expressed by @lukesteensen](https://discord.com/channels/742820443487993987/746070604192415834/796425792875397120) in the #development channel in discord we have three specific concerns around the choice:

-  Our internal data model itself (inclusive of our desire to not incidentally create _another_ observability data format)
-  How we serialize that data for internal use
-  The api for accessing that data model (e.g. lookups)

These three concerns don't fully enumerate the problem but I think they're a really excellent distillation of what we need to be considering. So, to reiterate - our ideal solution solves for all three but that might just not be possible. While I initially investigated this problem with really only #2 in mind, discussions with folks on the team led me to feeling like a real firecracker of a solution might be possible for both #1 and #2 out box. With the current state of lookups today, I'm less inclined to try to solve #3 immediately and I'm unconvinced that something might provide us a solution to it directly out of box. (Though, with that said, [flatbuffers](https://google.github.io/flatbuffers/) might be an interesting piece of software to take a look at in the future since it provides users the ability to access serialized data without unpacking it despite it not ultimately being my suggested approach.)

It might not be totally clear from the serialization benchmark examples but while protobufs are inherently slow in some respects, they're _still_ more performant than raw JSON implementations and even MssagePack (which I would be willing to bet remains true for CBOR). Any schema based data format is likely going to have better perf than something schemaless. The schema itself is obviously part of the tooling overhead here but I'd argue that if we cared _more_ about tooling overhead than we did about perfomance and sustainability we'd probably be writing this project in something other than Rust.

The truth is that of the formats that have wide adoption, all of them have their pitfalls and while protobufs bring with it some real tangible maintenance burden its still some of the _best_ tools with better integrations and libraries than most of the other things out there.

As an intentionally buried lede - I wrote this in the hopes that folks would share their opinions on the subject so you might be able to find some gaps in theorem here.

## Prior Art

As mentioned above - my suggestion for the  data spec itself is that we _do_ in fact follow along with prior art. Should we follow this path, we should be giving `Open Telemetry` a thorough exploration. Per the format itself - I haven't found any single conglomerate conlusion about  _any_ encoding format. There are arguments for and against any that we could adopt.


## Drawbacks

The drawbacks to fully adopting protobufs on top of an OpenTelemetry spec are many, which makes the path no different than any other.
    - Protobufs are slower than other schema'd data formats
    - Protobufs have an unignorable maintenance and tooling overhead
    - Protobufs are a heavy-weight solution to what could be percieved as a lightweight problem
    - OpenTelemetry's data spec could be dramatically different than the way we structure our data today.

Without question the existing burden of protobufs continues to exist if we follow this path.

## Alternatives

One alternative as mentioned already is flatbuffers and I didn't suggest this as a primary course of action for a few reasons. Like a lot of alternative encoding formats out there I'm honestly just not all that aware of what level of adoption it has. Also, While it has Rust support it seems rather lightweight and the level of integration available might not meet our needs. That being said, flatbuffers is really one of many many alternatives. Raw JSON with serde_bytes is another way to attack the problem. The alternatives are practically bottomless and to be perfectly honest, pretty subjective so it felt like the right approach here was to acknowledge that and submit an RFC to have a more structured conversation about those alternatives.

## Outstanding Questions

Should we abandon this proposal and instead do something entirely different?

Should we continue on with protobufs and solve for our data structure leakage?

Should we continue on with protobufs and ignore or data structure leakage?

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Submit a PR leveraging the proposal in vector source/sink's http configuration
- [ ] ...
