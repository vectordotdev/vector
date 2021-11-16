---
date: "2021-11-16"
title: "Pipelines is now available on Vector"
description: "A guide for using the new pipelines functionality"
authors: ["barieom"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

# Pipelines is now available on Vector

We're excited to announce a new release that introduces the concept of Pipelines to Vector.

Vector's topology is based on a [directed acylclic graph], enabling users to create a complex pipelines and powerful models that power advanced use cases, illustrated by the example diagram below:
![DAG example](/img/vector-DAG-example.png)

However, this is in contrast to many leading observability pipeline offerings — such as [Datadog's LWL Pipelines][] — by defining a linear set of transforms, in which the first transform feeds into the second, then the third, in sequential manner. Diagram below highlights the different flow compared to the DAG model:
![New pipeline example](/img/new-pipeline-example.png)

To align with industry comparable workflows, Vector now offers a linear transforms, reducing complexity. 

To illustrate this better, let's assume we have a pipeline with three stages - source, transforms, then sink. The `transforms` directory has the following structure:

```
/transforms/
└───processing/
│   └───logs/   
│       │   sudo.toml
│       │   aws_s3_access_logs.toml
│   
│   processing.toml
│   
```

The configuration files for `sudo.toml` and `aws_s3_access_logs.toml` respectively are below:

```toml
name = "Sudo"
filter.type = "datadog_search"
filter.condition = "source:s3"

[[transforms]]
type = "remap"
source = '''
. |= parse_grok(
    value: .message,
    patterns: ["sudo.default", "%{_sudo_user} : %{_sudo_tty}( %{_sudo_pwd})? %{_sudo_run_as_user}( %{_sudo_group})?( %{_sudo_tsid})?( %{_sudo_env})? (%{_sudo_command})?; (%{_sudo_lang} )?(%{_sudo_lc_type} )?.*"],
    aliases: {
        _sudo_user: "%{notSpace:system.user}",
        _sudo_tty: "TTY=%{notSpace:system.tty} ;",
        _sudo_pwd: "PWD=%{notSpace:system.pwd} ;",
        _sudo_run_as_user: "USER=%{notSpace:system.run_as_user} ;",
        _sudo_group: "GROUP=%{notSpace:system.run_as_group} ;",
        _sudo_tsid: "TSID=%{notSpace:system.tsid} ;",
        _sudo_env: "ENV=%{data:system.env}",
        _sudo_command: "COMMAND=%{data:system.cmd}",
        _sudo_lang: "LANG=%{notSpace:system.lang}",
        _sudo_lc_type: "LC_CTYPE=%{notSpace:system.lc_type}"
    }
)
'''
```

``` toml
name = "AWS S3 access logs"
filter.type = "datadog_search"
filter.condition = "source:s3"

[[transforms]]
type = "remap"
source = '''
# Processor 1: Grok Parser: Parsing S3 Access Logs
. |= parse_grok(
    .message,
    patterns: [
        ...
    ]
    aliases: {
        ...
    }
)

# Processor 2: User-Agent Parser
.http.useragent_details |= parse_user_agent(.http.useragent)

# Processor 3: Url Parser
.http.url_details |= parse_user_agent(.http.url)

# Processor 4: Date Remapper: Define date_access as the official timestamp of the log
schema_set_timestamp(.date_access)

# Processor 5: Category Processor: Categorize status code
.http.status_category = switch http.status_code {
when 200...299:
    "OK"
when 300...399:
    "notice"
when 400...499:
    "warning"
}

# Processor 6: Status Remapper: Set the log status based on the status code value
schema_set_status(.http.status_category)
'''
```

And in the processing.toml file, the following configuration is required to set the order of the processors:

``` toml
type = "pipelines"
inputs = ["datadog_agent", "syslog"]
mode = "linear"

logs.order = [
    "aws_s3_access_logs",
    "sudo"
]
```


For our next steps, we'll be looking to add `filtering` functionality, which will enable events to be passed to the next processor if a condition is not. In the meantime, if you any feedback for us, let us know on our [Discord chat][] or on [Twitter][]!


[Datadog's LWL Pipelines]: https://www.datadoghq.com/blog/logging-without-limits/
[directed acyclic graph]: /docs/about/under-the-hood/architecture/pipeline-model/
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev