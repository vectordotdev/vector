---
description: Install Vector on the Docker platform
---

# Docker

Vector maintains Docker images [available on Docker Hub][url.docker_hub_vector].
The [`Dockerfile`][url.dockerfile] is available in the root of the Vector repo.

## Installation

Start by pulling the Vector image:

```bash
docker pull timberio/vector:latest
```

Run the image:

```bash
docker run -ti timberio/vector:latest /vector/bin/vector --config=/vector/config/vector.toml
```

This will boot Vector with the [default configuration][url.default_configuration],
which is likely not what you want. See the [Custom Configuration](#custom-configuration)
section for more info.

## How It Works

### Base Image

The Vector Docker images use `debian:9-slim`.

### Custom Configuration

The Vector Docker images ship with [default configuration][url.default_configuration].
This is meant to serve as an example. To customize the configuration we recommend
generating a new image that includes your own configuration:

{% code-tabs %}
{% code-tabs-item title="Dockerfile" %}
```
FROM timberio/vector:latest
COPY vector.toml /vector/config/vector.toml
```
{% endcode-tabs-item %}
{% endcode-tabs %}

`vector.toml` should be in the same directory as your `Dockerfile`. Information
on Vector's configuration can be found in the [Configuration][docs.configuration]
section.

## Log Parsing

Docker, by default, wraps it's log messages in JSON documents. For example:

```
{"log": "{\"message\": \"Sent 200 in 54.2ms\", \"status\": 200}", "stream": "stdout", "time": "2018-10-02T21:14:48.2233245241Z"}
```

When configuring Vector, you'll want to unwrap the JSON, parsing and merging
the JSON document contained in the `"log"` key. This can be achieved by
chaining [`json_parser` transforms][docs.json_parser_transform]. You can see an
example of this in the [I/O section of the `json_parser` documentation][docs.json_parser_transform.io]

## Resources

* [Vector on Docker Hub][url.docker_hub_vector]
* [Dockerfile][url.dockerfile]


[docs.configuration]: ../../..docs/usage/configuration
[docs.json_parser_transform.io]: ../../../usage/configuration/transforms/json_parser.md#io
[docs.json_parser_transform]: ../../../usage/configuration/transforms/json_parser.md
[url.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[url.docker_hub_vector]: https://hub.docker.com/r/timberio/vector
[url.dockerfile]: https://github.com/timberio/vector/blob/master/Dockerfile
