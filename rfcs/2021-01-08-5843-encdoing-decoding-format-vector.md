# RFC 5843 - 2021-01-08 - Encoding/Decoding for Vector to Vector Communication

There has been an ongoing discussion of what format Vector-to-Vector communications should take. For some examples of issues related to the subject check out #5809, #5124, or #5341. This RFC is intended to propose a format for standardization of our vector-to-vector communications.

## Scope

- This RFC should cover the encoding format we intend to leverage
- It should also cover any alterations that might need to be made to the expected data format

## Motivation

The question posed by @binarylogic in #5843 is a good definition for the motivation of this change: "How do we transmit data from one Vector instance to another while being mindful of the public contract we create?" Clearly sending metrics from one Vector instance to another should preserve all metric data upon ingestion downstream but there are still concerns around exposing an undocumented format to users in the context of these operations. This discussion continues to come up and we really need a canonincal and definitive decision around what shape this takes so that when we expand our components our contract is explicitly codified.

## Internal Proposal

From a high level my proposal is pretty simple. I'll dive into the rationale and context lower in the doc but lets just tee that up, shall we?

I propose that if we're not explicitly searching for alternate tooling because we've decided that we wholly can't deal with the tooling overhead of protobufs, we should instead continue to use them with prost. As an added dimension to the issue I'd suggest that we support an initial Transport of HTTP/1.1 and consider a fast-follow of effort to implement HTTP/2 (or even optionally GRPC - though it bares saying I am unconvinced of the necessity of GRPC over HTTP/2 for our usecase).

Performance of the format itself is not a huge concern as everything we're considering is fast enough not to be a bottleneck for us. However, our current implementation of Protobufs in vector and the integration point to our data model is unoptimized. Should we continue using Protobuf, we should take an optimization pass at them.

## Doc-level Proposal

I'm fairly certain that this should not require a docs entry as it's largely internal to the tools. There is a possibility that we might want to document the data format we use if either to dissuade users from following it/using it (in the case that we don't adopt a standardized data format) or to point out that we've standardized on an open data spec that can be followed and used in other areas.

## Rationale

Unfortunately in most of my research I've pretty well come to the conclusion that encoding formats as a domain are rife with technical and political failures. That is to say, in some regard or another, every format from JSON to Bincode is going to force a tradeoff between correctness, performance, features or tooling and maintenance overhead. Inlined below here is a reasonable starting point on performance. It's taken from the [NoProto project](https://github.com/only-cliches/NoProto) since they're regularly updating their benchmarks. They've also done an excellent job of enumerating some of the aspects to consider when making a decision of this nature and their read on the space very much mirrors my own thinking in many ways.

For the below data, Encodes and Decodes are ops/sec so higher is better while size before and after compression lower is better. This list is far from exhaustive and specifcially lacks a serde_json comparison which is unfortunate but other benches out there do include them (though obviously a different benchmark makes cross comparison difficult).

| Library           | Encode | Decode All | Decode 1 | Update 1 | Size (bytes) | Size (Zlib) |
| ----------------- | ------ | ---------- | -------- | -------- | ------------ | ----------- |
| **Runtime Libs**  |        |            |          |          |              |             |
| _NoProto_         | 1057   | 1437       | 47619    | 12195    | 208          | 166         |
| Apache Avro       | 138    | 51         | 52       | 37       | 702          | 336         |
| FlexBuffers       | 401    | 855        | 23256    | 264      | 490          | 309         |
| JSON              | 550    | 438        | 544      | 396      | 439          | 184         |
| BSON              | 115    | 103        | 109      | 80       | 414          | 216         |
| MessagePack       | 135    | 222        | 237      | 119      | 296          | 187         |
| **Compiled Libs** |        |            |          |          |              |             |
| Flatbuffers       | 1046   | 14706      | 250000   | 1065     | 264          | 181         |
| Bincode           | 5882   | 8772       | 9524     | 4016     | 163          | 129         |
| Protobuf          | 859    | 1140       | 1163     | 480      | 154          | 141         |
| Prost             | 1225   | 1866       | 1984     | 962      | 154          | 142         |

Performance is a single aspect of a nuanced problem for us; for more perf benchmarks I'd suggest checking out [rust-serialization-benchmarks](https://github.com/erickt/rust-serialization-benchmarks). Some of these formats (JSON for example) still leaves us with a marginal concern around dropped metadata.

While it is tempting to look at perf benchmarks and to just pick the best performing example, the practical requirements around the problem for us might give us some limitations. We have a few specific domains of concern around the chosen path:

- Performance
- Compatibility with Transports
- Maintaining backcompat
- Tooling and Maintenance costs

These four concerns don't fully enumerate the problem but I think they're a really excellent distillation of what we need to keep in mind. So, to reiterate - our ideal solution solves for all four but that might just not be possible.

It might not be totally clear from the serialization benchmark examples but while protobufs are inherently slow in some respects, they're _still_ more performant than raw JSON implementations and even MessagePack or CBOR. And their maintenance cost has already been paid since we're using them today. Any schema based data format is likely going to have better perf than something schemaless. The schema itself is obviously part of the tooling overhead here but I'd argue that if we cared _more_ about tooling overhead than we did about perfomance and sustainability we'd probably be writing this project in something other than Rust.

With regards to the tansport and the suggested path of implementing HTTP/1.1 _before_ HTTP/2 we have to keep both perf and kubernetes issues in mind. [TCP causes us some problems in our K8s integration today](https://github.com/timberio/vector/issues/2070) and unfortunately this problem has quite a bit more context to it than can sanely be shared in this RFC. Suffice to say that any choice we make here has repurcussions on our deployment architecture and ultimately HTTP/2 with Protobufs provides us what we think are the right tradeoffs for our implementation. However, just quickly getting out a low-effort HTTP/1.1 implementation with batching solves some immediate pain for ourselves and our users around dynamic IPs and kubernetes load-balancing. I'd classify it as low-hanging fruit while a more thorough HTTP/2 and optimized Protobufs effort gets underway.

The truth is that of the formats that have wide adoption, all of them have their pitfalls and while Protobufs bring with it some maintenance burden its still some of the _best_ tools with better integrations and libraries than most of the other things out there.

As with protofbufs, HTTP/2 is pretty dang performant, it can provide users with some header compression, and it allows us to leverage client-side load-balancing in kubernetes. It feels like the best choice from that perspective.

As an intentionally buried lede - I wrote this in the hopes that folks would share their opinions on the subject so you might be able to find some gaps in theorem here.

## Prior Art

Per the data format itself - I haven't found any single conglomerate conlusion about _any_ encoding format. However, existing projects using HTTP/2 and Protobufs (or gRPC over Tonic and Protobufs) are about a dime a dozen. These protocols and formats are used together in just about anything anymore.

## Drawbacks

The only drawback to adopting protobuf as the encoding format is that Protobufs can be slightly slower than other schema'd data formats and we don't shed the protobuf tooling overhead.

Drawbacks to adopting HTTP/2 are that (like literally any other transport we could pick) it alters our kubernetes deployment handling and if we don't decide to continue to maintain other transports it means needing to appropriately handle the deprecation and removal of our TCP and HTTP/1.1 implementations fairly soon afterwards.

## Alternatives

One alternative to consider is that we just commit to providing multiple transports and stick with protobufs for encoding decoding. I think this approach should be considered thoroughly. Assuming we're using protobufs as the only encoding type, maintaining a few different transports might not be a huge maintenance overhead and it could give users some runtime flexibility that might add value.

## Outstanding Questions

Are there features in GRPC that we'd actually want to use?

Are there any other side benefits of using HTTP/2 or gRPC for transport?

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ] Implement HTTP/1.1 with batching and v2 config (similar to the Lua transform)
- [ ] Implementent HTTP/2
- [ ] Optimize Protobufs implementation
- [ ] Deprecate HTTP/1.1 and TCP
- [ ] Remove HTTP/1.1 and TCP support?
