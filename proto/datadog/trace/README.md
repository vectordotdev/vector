# Datadog Agent trace protobufs

Vendored from [`DataDog/datadog-agent`](https://github.com/DataDog/datadog-agent)
`pkg/proto/datadog/trace`.

Pinned to commit `14f65ca3802739e69ea2951a60c87814c21f7161` (2026-08-13), the latest
change in this tree as of the Vector import (`span.proto` msgpack limit for
`meta_struct`). Refresh by copying the files Vector compiles from that path at a
newer commit and updating this pin.
