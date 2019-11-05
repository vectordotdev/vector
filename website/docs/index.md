---
id: "what_is_vector"
title: "What is Vector?"
---

![][assets.components]

Vector is a [high-performance][docs.performance], [open-source][urls.vector_repo]
observability data router. It makes [collecting][docs.sources],
[transforming][docs.transforms], and [sending][docs.sinks] logs, metrics, &
events easy. It decouples data collection from your services, giving you data
ownership and control, among [many other benefits][docs.use_cases].

Built in [Rust][urls.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making it the single tool you need to get data
from A to B.


[assets.components]: ./assets/components.svg
[docs.administration]: ./administration
[docs.correctness]: ./about/correctness
[docs.deployment]: ./setup/deployment
[docs.performance]: ./about/performance
[docs.roles.agent]: ./setup/deployment/roles/agent
[docs.roles.service]: ./setup/deployment/roles/service
[docs.sinks]: ./components/sinks
[docs.sources]: ./components/sources
[docs.transforms]: ./components/transforms
[docs.use_cases]: ./use_cases
[urls.rust]: https://www.rust-lang.org/
[urls.vector_repo]: https://github.com/timberio/vector
