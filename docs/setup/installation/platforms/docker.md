---
description: Install Vector on the Docker platform
---

# Docker

Vector maintains Docker images [available on Docker Hub][docker_hub_vector].
The [`Dockerfile`][docker_file] is available in the root of the Vector repo.

## Installation

Start by pulling the Vector image:

```bash
docker pull timberio/vector:latest
```

Run the image:

```bash
docker run -ti timberio/vector:latest /vector/bin/vector --config=/vector/config/vector.toml
```

This will boot Vector with the [default configuration][default_configuration],
which is likely not what you want. See the [Custom Configuration](#custom-configuration)
section for more info.

## How It Works

### Base Image

The Vector Docker images use [`debian:9-slim`][debian_base_image]

### Custom Configuration

The Vector Docker images ship with [default configuration][default_configuration].
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
on Vector's configuration can be found in the [Configuration][configuration]
section.

## Log Parsing

Docker, by default, wraps it's log messages in JSON documents. For example:

```
{"log": "{\"message\": \"Sent 200 in 54.2ms\", \"status\": 200}", "stream": "stdout", "time": "2018-10-02T21:14:48.2233245241Z"}
```

When configuring Vector, you'll want to unwrap the JSON, parsing and merging
the JSON document contained in the `"log"` key. This can be achieved by
chaining [`json_parser` transforms][json_parser_transform]. You can see an
example of this in the [I/O section of the `json_parser` documentation][json_parser_transform_io]

## Resources

* [Vector on Docker Hub][docker_hub_vector]
* [Dockerfile][dockerfile]


[default_configuration]: default_configuration
[debian_base_image]: https://hub.docker.com/_/debian/?tab=description
[docker_hub_vector]: https://hub.docker.com/r/timberio/vector
[dockerfile]: https://github.com/timberio/vector/blob/master/Dockerfile
[json_parser_transform]: ../../configuration/transforms/json_parser.md
[json_parser_transform_io]: ../../configuration/transforms/json_parser.md#i-o
