---
title: Unit tests reference
short: Unit tests
weight: 5
---

You can define unit tests in your Vector configuration file to cover a network of transforms within the topology. The intention of these tests is to improve the maintainability of configurations containing larger and more complex combinations of transforms.

Executing tests within a configuration file can be done with the test subcommand:

```bash
vector test /etc/vector/*.toml
```

## Configuration

{{% config/unit-tests %}}
