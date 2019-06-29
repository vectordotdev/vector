---
description: Using Vector on Docker
---

# Docker

Vector maintains the [`timberio/vector` Docker image][url.docker_hub_vector]
available on [Docker Hub][url.docker_hub_vector].

## Installation

{% tabs %}
{% tab title="Default Config" %}
Vector ships with a [default `vector.toml` file][url.default_configuration]
as a proof of concept. This is used to test Vector and ensure it is installed
and working:

```bash
docker run -ti timberio/vector:latest /vector/bin/vector --config=/vector/config/vector.toml
```

See the "Custom Config" tab for how to use your own Vector confirmation file.
{% endtab %}
{% tab title="Custom Config" %}
Start by generating a `vector.toml` file with your custom
[configuration]


[url.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[url.docker_hub_vector]: https://hub.docker.com/r/timberio/vector
