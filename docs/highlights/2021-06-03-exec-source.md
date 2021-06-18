---
last_modified_on: "2021-06-02"
$schema: ".schema.json"
title: "Generate events from an external program using the new `exec` source"
description: "Introducing the new `exec` source for generating events from the output of other programs"
author_github: "https://github.com/jszwedko"
pr_numbers: [6876]
release: "0.14.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: sources"]
---

This release includes a new `exec` source that can be used to run programs outside of Vector to generate log events by
consuming the stdout and stderr output streams. This can be especially useful to consume input from sources that Vector
does not yet natively support such as querying data from a Postgres database via `psql`.

It is capable of either running a command on an interval or starting up a long-running command.

See [the `exec` source documentation](\(urls.vector_exec_source)) for more details and examples.

Thanks to [@moogstuart](https://github.com/moogstuart) for this contribution.
