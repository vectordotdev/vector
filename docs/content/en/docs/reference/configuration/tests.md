---
title: Unit testing Vector configurations
short: Unit tests
weight: 5
aliases: ["/docs/reference/tests"]
---

Vector enables you to unit test [transforms] in your processing topology

You can define unit tests in your Vector configuration file to cover a network of transforms within the topology. The intention of these tests is to improve the maintainability of configurations containing larger and more complex combinations of transforms.

You can execute tests within a configuration file using the `test` subcommand:

```bash
vector test /etc/vector/vector.toml
```

[transforms]: /docs/reference/glossary/#transform
