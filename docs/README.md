# What Is Vector?

![](assets/components.svg)

Vector is a [high-performance](performance.md), [open-source](https://github.com/timberio/vector) router for observability data. It makes [collecting](usage/configuration/sources/), [transforming](usage/configuration/transforms/), and [sending](usage/configuration/sinks/) logs, metrics, & events easy. It decouples data collection & routing from your services, giving you data ownership, and enabling you to freely adopt services over time, among [many other benefits](use-cases/).

Built in [Rust](https://www.rust-lang.org/), Vector places high-value on [performance](performance.md), [correctness](correctness.md), and [operator friendliness](usage/administration/). It compiles to a single static binary and is designed to be [deployed](setup/deployment/) across your entire infrastructure, serving both as a light-weight [agent](setup/deployment/roles/agent.md) and a highly efficient [service](setup/deployment/roles/service.md), making it the single tool you need to get data from A to B.

