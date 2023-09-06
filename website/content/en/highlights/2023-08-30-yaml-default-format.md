---
date: "2023-08-30"
title: "YAML default configuration format"
description: "YAML is now the default configuration format"
authors: ["pront"]
pr_numbers: [18325, 18345, 18378, 18388, 18420]
release: "0.33.0"
hide_on_release_notes: false
badges:
  type: announcement
---

## What is changing?

We are happy to announce that the default configuration language for Vector has been updated from [TOML](https://toml.io/en/) to [YAML](https://yaml.org/).

We hope that this will improve the overall clarity of Vector configurations as we've found that TOML configurations quickly become difficult to read when they contain more than a small number of components. Updating the default in the documentation and CLI defaults will encourage users to start out with YAML rather than switching only once they outgrow TOML. This also aligns the default configuration language with the required language, YAML, when deploying via Helm, thus reducing some friction.

Existing TOML and JSON configurations are not affected by this decision and will work as usual.

## Action Needed

If you are relying on Vector auto-loading `/etc/vector/vector.toml`, this will still work in `0.33.0` but it is now deprecated. The aforementioned location will no longer be considered in `0.34.0`. The `/etc/vector/vector.yaml` will be used as the secondary default location in `0.33.0` and will become the  default path in `0.34.0`. You can keep using your existing config by providing the following option `--config /etc/vector/vector.toml` explicitly. Alternatively, you can convert your existing configuration to YAML and write it to the new default path.

## New Tools

For those users interested in switching to YAML, the next Vector release will provide the following new tools:

* We implemented a new command, `vector convert-config`, which can be used as a starting point to convert one or more configurations from TOML/JSON to YAML. Note that this command is best-effort and comes with the following caveats:
  * It will not preserve comments.
  * It might skip explicitly writing values if they are equal to the default config values.
  * Please review the converted config and edit accordingly.
* The existing `vector generate` command now can generate YAML configurations.
