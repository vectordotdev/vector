---
title: Config Autocompletion Guide
short: Config Autocompletion
description: Learn how to generate the Vector schema and use it for IDE autocompletion.
author_github: https://github.com/pront
domain: dev
tags: [ "dev", "ide", "autocompletion", "config", "guides", "guide" ]
aliases: [ "/docs/guides/developer/config-autocompletion.md" ]
weight: 1
---

{{< requirement >}}
This guide assumes you understand [basic Vector concepts][concepts]

[concepts]: /docs/about/concepts
{{< /requirement >}}

Vector provides CLI [subcommands][subcommands] to perform various tasks apart from running pipelines. This short guide focuses on how to
use the `generate-schema` subcommand to generate the Vector schema with your Vector binary and how to provide its output to an IDE to
enable autocompletion.

## How to use `generate-schema`

Run the following:

```sh
# Optional step: get the Vector version and include it in the file name.
# vector --version
vector generate-schema -o vector-v0.45.0-schema.json
```

## Integrate with IDEs

### JetBrains (e.g. RustRover)

1. `Settings | Languages & Frameworks | Schemas and DTDs | JSON Schema Mappings`
2. Import `vector-v0.45.0-schema.json`

You can find more details [here][jetbrains].

### Visual Studio Code

Follow the guide [here][vscode].

## Example

<img src="/gifs/guides/config-autocomplete.gif" alt="Config Autocomplete GIF"/>

With this setup, the IDE will provide real-time suggestions and reduce visits to the [Vector docs][docs].

[subcommands]: https://github.com/vectordotdev/vector/blob/master/src/cli.rs#L268-L321

[jetbrains]: https://www.jetbrains.com/help/idea/yaml.html#json_schema

[vscode]: https://www.ibm.com/docs/en/dbb/3.0?topic=ide-configuring-schema-validation-vscode#3-open-the-yamlschemas-property-in-settingsjson

[docs]: https://vector.dev/docs/
