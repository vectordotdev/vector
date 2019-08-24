# What Is Vector?

![](assets/components.svg)

Vector is a [high-performance][docs.performance], [open-source][url.vector_repo]
observability data router. It makes [collecting][docs.sources],
[transforming][docs.transforms], and [sending][docs.sinks] logs, metrics, &
events easy. It decouples data collection from your services, giving you data
ownership and control, among [many other benefits][docs.use_cases].

Built in [Rust][url.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.agent_role] and a highly efficient
[service][docs.service_role], making it the single tool you need to get data
from A to B.


[docs.administration]: ./usage/administration
[docs.agent_role]: ./setup/deployment/roles/agent.md
[docs.correctness]: ./correctness.md
[docs.deployment]: ./setup/deployment
[docs.performance]: ./performance.md
[docs.service_role]: ./setup/deployment/roles/service.md
[docs.sinks]: ./usage/configuration/sinks
[docs.sources]: ./usage/configuration/sources
[docs.transforms]: ./usage/configuration/transforms
[docs.use_cases]: ./use-cases
[url.rust]: https://www.rust-lang.org/
[url.vector_repo]: https://github.com/timberio/vector
