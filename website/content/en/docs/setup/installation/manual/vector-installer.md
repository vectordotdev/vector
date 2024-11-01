---
title: Install Vector using the Vector installer
short: Vector installer
---

The Vector installer enables you to install Vector using a platform-agnostic installation script:

```shell
curl --proto '=https' --tlsv1.2 -sSfL https://sh.vector.dev | bash
```

You may use `VECTOR_VERSION` to specify a custom version like below:

```shell
curl --proto '=https' --tlsv1.2 -sSfL https://sh.vector.dev | VECTOR_VERSION=0.34.1 bash
```

## Management

{{< jump "/docs/administration/management" "vector-executable" >}}
