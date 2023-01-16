# RFC 5843 - 2021-01-08 - Encoding/Decoding for Vector to Vector Communication

There has been an ongoing discussion of what format Vector-to-Vector communications should take. For some examples of issues related to the subject check out #5809, #5124, or #5341. This RFC is intended to propose a format for standardization of our vector-to-vector communications.

## Scope

- This RFC should cover the encoding format we intend to leverage
- It should also cover any alterations that might need to be made to the expected data format

## Motivation

The question posed by @binarylogic in #5843 is a good definition for the motivation of this change: "How do we transmit data from one Vector instance to another while being mindful of the public contract we create?" Clearly sending metrics from one Vector instance to another should preserve all metric data upon ingestion downstream but there are still concerns around exposing an undocumented format to users in the context of these operations. This discussion continues to come up and we really need a canonical and definitive decision around what shape this takes so that when we expand our components our contract is explicitly codified.

## Internal Proposal

From a high level my proposal is pretty simple. I'll dive into the rationale and context lower in the doc but lets just tee that up, shall we?

I propose that if we're not explicitly searching for alternate tooling because we've decided that we wholly can't deal with the tooling overhead of protobufs, we should instead continue to use them with prost. As an added dimension to the issue I'd suggest that we support an initial Transport of gRPC (or even optionally HTTP/2 - we'll discuss why I'd suggest gRPC over HTTP/2 later in the doc).

Performance of the format itself is not a huge concern as everything we're considering is fast enough not to be a bottleneck for us. However, our current implementation of Protobufs in vector and the integration point to our data model is unoptimized. Should we continue using Protobuf, we should take an optimization pass at them.

## Doc-level Proposal

I'm fairly certain that this should not require a docs entry as it's largely internal to the tools. There is a possibility that we might want to document the data format we use if either to dissuade users from following it/using it (in the case that we don't adopt a standardized data format) or to point out that we've standardized on an open data spec that can be followed and used in other areas.

## Rationale

Unfortunately in most of my research I've pretty well come to the conclusion that encoding formats as a domain are rife with technical and political failures. That is to say, in some regard or another, every format from JSON to Bincode is going to force a tradeoff between correctness, performance, features or tooling and maintenance overhead. Inlined below here is a reasonable starting point on performance. It's taken from the [NoProto project](https://github.com/only-cliches/NoProto) since they're regularly updating their benchmarks. They've also done an excellent job of enumerating some of the aspects to consider when making a decision of this nature and their read on the space very much mirrors my own thinking in many ways.

For the below data, Encodes and Decodes are ops/sec so higher is better while size before and after compression lower is better. This list is far from exhaustive and specifically lacks a serde_json comparison which is unfortunate but other benches out there do include them (though obviously a different benchmark makes cross comparison difficult).

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

It might not be totally clear from the serialization benchmark examples but while protobufs are inherently slow in some respects, they're _still_ more performant than raw JSON implementations and even MessagePack or CBOR. And their maintenance cost has already been paid since we're using them today. Any schema based data format is likely going to have better perf than something schemaless. The schema itself is obviously part of the tooling overhead here but I'd argue that if we cared _more_ about tooling overhead than we did about performance and sustainability we'd probably be writing this project in something other than Rust.

With regards to the transport and the suggested path of implementing gRPC we have to keep both perf and kubernetes issues in mind. [TCP causes us some problems in our K8s integration today](https://github.com/vectordotdev/vector/issues/2070) and unfortunately this problem has quite a bit more context to it than can sanely be shared in this RFC. Suffice to say that any choice we make here has repercussions on our deployment architecture and ultimately gRPC with Protobufs provides us what we think are the right tradeoffs for our implementation.

This brings us to the question of _why_ gRPC instead of HTTP/2 or even HTTP/3 for that matter. There are benefits in HTTP/3 but not _really_ around throughput performance as much as reliability and behavior. HTTP/3 being based on UDP means that in the case of fetching multiple objects simultaneously in the case of a dropped packet only the single interrupted stream is blocked as opposed to all streams being blocked head of line. While this might be useful behavior, the cost of writing and maintaining something in HTTP/3 will (likely) initially be much higher. Available libraries in Rust are fairly low-level and don't provide much in the way of quality abstraction for consumers, which doesn't even cover the major glaring issue that Http/3 as a protocol hasn't fully proliferated or become ubiquitous and novelty at this stage of the project is probably not what we want. That alone makes me feel like it should be avoided initially.

So there are some tangible benefits to using gRPC instead of just HTTP/2 specifically relating to ergonomics and maintenance which ends up being the principle motivator for this decision for me. Most everything that we could want to do with gRPC out of box can be achieved with HTTP/2 in hyper. However using gRPC also gives us access to [tonic](https://github.com/hyperium/tonic) which provides some truly excellent abstractions out-of-box that could pay dividends on our tooling and maintenance overhead the further we go with it, including the maintenance and overhead of protobuf generation. As a specific and shining example, should we want or need to adopt streaming requests, bi-directional stream or mutual TLS authentication. `Tonic` makes this really straightforward and ergonomic. Lets look at an example. First let's start with the protofbuf file specifically:

```protobuf
    syntax = "proto3";

    package our_rpc;

    service Dummy {
      rpc Send (DummyRequest) returns (DummyResponse);
      rpc SendStream(DummyRequest) returns (stream DummyResponse);
      rpc ReceiveStream(stream DummyRequest) returns (DummyResponse);
      rpc Bidirectional(stream DummyRequest) returns (stream DummyResponse);
    }

    message DummyRequest {
      string name = 1;
    }

    // return value
    message DummyResponse {
      string message = 1;
    }
```

This example assumes a server that has trait implementations for the services we've defined in our protos. So let's whip those up real quick here.

```rust
use tokio::sync::mpsc;
use tonic::{transport::Server, Request, Response, Status};

use our_rpc_mod::say_server::{Dummy}, DummyServer};
use our_rpc_mod::{DummyRequest, DummyResponse};

mod our_rpc_mod;

#[derive(Default)]
pub struct MyHandler {}

#[tonic::async_trait]
//Implementation of the traits we need for our various "services" defined in our protos.
impl Dummy for MyHandler {
    type SendStreamStream = mpsc::Receiver<Result<DummyResponse, Status>>;
    async fn send_stream(
        &self,
        request: Request<DummyRequest>,
    ) -> Result<Response<Self::SendStreamStream>, Status> {
        let (mut tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            for _ in 0..4 {
                tx.send(Ok(DummyResponse {
                    message: format!("hello"),
                }))
                .await;
            }
        });

        Ok(Response::new(rx))
    }

    type BidirectionalStream = mpsc::Receiver<Result<DummyResponse, Status>>;
    async fn bidirectional(
        &self,
        request: Request<tonic::Streaming<DummyRequest>>,
    ) -> Result<Response<Self::BidirectionalStream>, Status> {
        let mut streamer = request.into_inner();
        let (mut tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            while let Some(req) = streamer.message().await.unwrap(){
                tx.send(Ok(DummyResponse {
                    message: format!("hello {}", req.name),
                }))
                .await;
            }
        });

        Ok(Response::new(rx))
    }

    async fn receive_stream(
        &self,
        request: Request<tonic::Streaming<DummyRequest>>,
    ) -> Result<Response<DummyResponse>, Status> {
        let mut stream = request.into_inner();
        let mut message = String::from("");

        while let Some(req) = stream.message().await? {
            message.push_str(&format!("Hello {}\n", req.name))
        }

        Ok(Response::new(DummyResponse { message }))
    }

    async fn send(&self, request: Request<DummyRequest>) -> Result<Response<DummyResponse>, Status> {
        Ok(Response::new(DummyResponse {
            message: format!("hello {}", request.get_ref().name),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:9999".parse().unwrap();
    let handler = MyHandler::default();
    println!("Server listening on {}", addr);
    Server::builder()
        .add_service(DummyServer::new(handler))
        .serve(addr)
        .await?;
    Ok(())
}
```

Hopefully those protos looks familiar enough. In order for all of this to fit together correctly we then create a module to "include" the generated proto handling code in our project with a lovely macro. This refers to the package name of our proto file. Let's say we name this `our_rpc_mod.rs`.

```rust
// this allows us to easily include code generated for package our_rpc from the .proto file
tonic::include_proto!("our_rpc");
```

With that done we have access to `our_rpc` generated code in our theoretical `client.rs` module as well as our previously written `server.rs`:

```rust
    use our_rpc_mod::dummy_client::DummyClient;
    use our_rpc_mod::DummyRequest;

    mod our_rpc_mod;

    #[tokio::main]
    async fn main() -> Result<(), Box<dyn std::error::Error>> {
      // Start a connection channel to the server
      let channel = tonic::transport::Channel::from_static("http://[::1]:9999")
        .connect()
        .await?;

    // Create a gRPC client from the channel
        let mut client = DummyClient::new(channel);

    // Build ourselves a request
        let request = tonic::Request::new(
            DummyRequest {
               name:String::from("eeyun")
            },
        );

    // Send it and wait for response
        let response = client.send(request).await?.into_inner();
        println!("RESPONSE={:?}", response);
        Ok(())
    }
```

It's not much code and it should be very easy to follow but It should also give you an idea of how easy tonic makes it. Now to swap our client between these modalities is _super_ trivial. For streaming we can go from what we have here to:

```rust
    // sending stream
        let response = client.receive_stream(request).await?.into_inner();
        println!("RESPONSE=\n{}", response.message);
```

Or if we want to go to bidirectional?

```rust
    // calling rpc
        let mut response = client.bidirectional(request).await?.into_inner();
    // listening on the response stream
        while let Some(res) = response.message().await? {
            println!("NOTE = {:?}", res);
        }
        Ok(())
```

This is obviously overly simplistic and a bit hand-wavey but, hopefully it expresses the value tonic provides in the form of these easy to consume abstractions. That seems to remain true if we want to do mutual TLS authentication or a few other more niche gRPC specific features which is fantastic. This library alone has me won over.

The truth is that of the formats that have wide adoption, all of them have their pitfalls and while Protobufs bring with it some maintenance burden its still some of the _best_ tools with better integrations and libraries than most of the other things out there.

As with protofbufs, HTTP/2 is pretty dang performant, it can provide users with some header compression, and it allows us to leverage client-side load-balancing in kubernetes. It feels like the best choice from that perspective.

As an intentionally buried lede - I wrote this in the hopes that folks would share their opinions on the subject so you might be able to find some gaps in theorem here.

## Prior Art

Per the data format itself - I haven't found any single conglomerate conclusion about _any_ encoding format. However, existing projects using HTTP/2 and Protobufs (or gRPC over Tonic and Protobufs) are about a dime a dozen. These protocols and formats are used together in just about anything anymore.

## Drawbacks

The only drawback to adopting protobuf as the encoding format is that Protobufs can be slightly slower than other schema'd data formats and we don't shed the protobuf tooling overhead.

Drawbacks to adopting HTTP/2 are that (like literally any other transport we could pick) it alters our kubernetes deployment handling and if we don't decide to continue to maintain other transports it means needing to appropriately handle the deprecation and removal of our TCP and HTTP/1.1 implementations fairly soon afterwards.

## Alternatives

One alternative to consider is that we just commit to providing multiple transports and stick with protobufs for encoding decoding. I think this approach should be considered thoroughly. Assuming we're using protobufs as the only encoding type, maintaining a few different transports might not be a huge maintenance overhead and it could give users some runtime flexibility that might add value.

Another alternative as was pointed out by @MOZGIII the schema that we have currently is simple compared to the use cases that protobuf was designed for - so we could consider writing the whole of the serialization logic with some optimal, specially tailored process. This is likely not worth doing but, it's probably worth discussing. The downside of a hard-written solution is that we'll then maintain our own serialization library - which has a maintenance cost, potentially higher than any tooling overhead.

## Outstanding Questions

Are there features in GRPC that we'd actually want to use?

Are there any other side benefits of using HTTP/2 or gRPC for transport?

## Plan Of Attack

The following steps are generally the incremental steps to execute this change. To summarize first we need to support a v2 config with both the `Vector` source _and_ the `Vector` sink. The v2 config will be backed by a GRPC implementation over Tonic that we will clearly document as an `Internal Only` API. Once fully implemented and tested we will deprecate the `Vector` v1 config in the off-chance a user is leveraging these sources and sinks. At this point it might be good to dive into optimization of our protobufs. Once GRPC has settled in we may wish to remove support for TCP. This last step is optional in the case that we need the source and sink to leverage a different transport or protocol for integrating into other systems.

- [ ] Implement GRPC via Tonic and v2 config (similar to the Lua transform)
- [ ] Deprecate TCP
- [ ] Optimize Protobufs implementation
- [ ] Remove TCP support (optional)
