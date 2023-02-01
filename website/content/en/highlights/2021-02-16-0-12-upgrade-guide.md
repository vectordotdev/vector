---
date: "2021-02-16"
title: "0.12 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.12.0"
authors: ["binarylogic"]
pr_numbers: [5281, 5978]
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

0.12 includes minimal breaking changes but significant deprecations. This guide will upgrade you quickly and
painlessly. If you have questions, [hop in our chat][chat] and we'll help you upgrade.

1. [Breaking: The `encoding.codec` option is now required for all relevant sinks](#first)
1. [Breaking: Vector `check_fields` conditions now require the `type` option](#second)
1. [Breaking: The `generator` source requires a `format` option](#third)
1. [Deprecation: Many transforms have been deprecated in favor of the new `remap` transform](#fourth)
1. [Deprecation: The `file` source `start_at_beginning` has been deprecated](#fifth)

## Upgrade Guide

### Breaking: The `encoding.codec` option is now required for all relevant sinks<a name="first"></a>

[Pull request #5281][pr_5281] removed the default values for the sink-level `encoding.codec` option. Therefore, you are
now required to provide a value for this option, ensuring that you are not surprised by opinionated encoding defaults.
Affected sinks include:

* `aws_s3` (previously defaulted to `text`)
* `file` (previously defaulted to `text`)
* `humio` (previously defaulted to `json`)
* `kafka` (previously defaulted to `text`)
* `nats` (previously defaulted to `text`)
* `new_relic_logs` (previously defaulted to `json`)
* `pulsar` (previously defaulted to `text`)
* `splunk_hec` (previously defaulted to `text`)

Upgrading is easy, just add the `encoding.codec` to your sinks with your preferred format (`json` or `text`):

```diff
 [sinks.backup]
 type = "aws_s3"
 inputs = ["..."]
 bucket = "my-bucket"
 compression = "gzip"
 region = "us-east-1"
+encoding.codec = "json"
```

For clarity, the `text` option strips away all structured data and passes only the value of the `message` field. It is
intended for use cases where Vector acts as a proxy and should not alter data. For most use cases we recommend `json`
since it includes all structured data.

### Breaking: Vector `check_fields` conditions now require the `type` option<a name="second"></a>

With the [announcement][vrl_announcement] of the [Vector Remap Language][vrl_reference] (VRL), [pull request #5978][pr_5978]
_deprecated_ the `check_fields` conditions in favor of using [VRL boolean expressions][vrl_boolean_expression]. The old
`check_fields` conditions were limiting and suffered from many of the [pitfalls][config_syntax_pitfalls] outlined in
the VRL announcement. Configuration languages, like TOML, are bad at expressing boolean conditions and severely
limited how users could [route][route_transform], [filter][filter_transform], and [reduce][reduce_transform] data.

While `check_fields` is deprecated and still supported, you will need to explicitly opt-into the feature by adding the
`type` option:

```diff
 [transforms.route]
 type = "route"
+lanes.errors.type = "check_field"
 lanes.errors."level.eq" = "error"
```

Alteratively, we recommend migrating to the new VRL syntax:

```diff
 [transforms.route]
 type = "route"
-lanes.errors."level.eq" = "error"
+lanes.errors = '.level = "error"'
```

Refer to the [VRL reference][vrl_reference] for the many ways you can specify conditions.

### Breaking: The `generator` source requires a `format` option<a name="third"></a>

The [`generator` source], commonly used for testing, has been updated with a new `format` option that emits logs in
the specified format. You will not be required to provide this option. Upgrading is easy:

```diff
 [sources.generator]
 type = "generator"
+format = "apache_common"  # or "apache_error" or "syslog"
```

### Deprecation: Many transforms have been deprecated in favor of the new `remap` transform<a name="fourth"></a>

The following transforms have been deprecated in favor of the new [`remap` transform][remap_transform]:

* [`add_fields`][add_fields_transform]
* [`add_tags`][add_tags_transform]
* [`ansi_stripper`][ansi_stripper_transform]
* [`aws_cloudwatch_logs_subscription_parser`][aws_cloudwatch_logs_subscription_parser_transform]
* [`coercer`][coercer_transform]
* [`concat`][concat_transform]
* [`grok_parser`][grok_parser_transform]
* [`json_parser`][json_parser_transform]
* [`key_value_parser`][key_value_parser_transform]
* [`logfmt_parser`][logfmt_parser_transform]
* [`merge`][merge_transform]
* [`regex_parser`][regex_parser_transform]
* [`remove_fields`][remove_fields_transform]
* [`remove_tags`][remove_tags_transform]
* [`rename_fields`][rename_fields_transform]
* [`split`][split_transform]
* [`tokenizer`][tokenizer_transform]

Deprecation notices have been placed on each of these transforms with example VRL programs that demonstrate how to
migrate to the new `remap` transform. For example, migrating from the `json_parser` transform is as simple as:

```toml
[transforms.remap]
type = "remap"
source = '''
. = merge(., parse_json!(.message))
'''
```

**You do not need to upgrade immediately. These transforms will not be removed until Vector hits 1.0, a milestone that
we hope to achieve in late 2022.** But, if possible, we recommend using this opportunity to upgrade and significantly
simplify your Vector configuration.

As always, if you need assistance [hop in our chat][chat]. We're eager to help and receive feedback on the language.

### Deprecation: The `file` source `start_at_beginning` has been deprecated<a name="fifth"></a>

As noted in the [file source checkpointing highlight][file_source_highlight], we've removed the `start_at_beginning`
option and replaced it with new [`ignore_checkpoints`][ignore_checkpoints] and [`read_from`][read_from] options.
Migrating is easy:

```diff
 [sources.file]
 type = "file"
-start_at_beginning = true
+ignore_checkpoints = false # default
+read_from = "beginning" # default
```

Adjust as necessary. The above values are the defaults and are not required to be specified.

[add_fields_transform]: /docs/reference/configuration/transforms/remap
[add_tags_transform]: /docs/reference/configuration/transforms/remap
[ansi_stripper_transform]: /docs/reference/vrl/functions/#strip_ansi_escape_codes
[aws_cloudwatch_logs_subscription_parser_transform]: /docs/reference/vrl/functions/#parse_aws_cloudwatch_log_subscription_message
[chat]: https://chat.vector.dev
[coercer_transform]: /docs/reference/vrl/functions/#coerce-functions
[concat_transform]: /docs/reference/configuration/transforms/remap
[config_syntax_pitfalls]: /blog/vector-remap-language/#config-languages
[file_source_highlight]: /highlights/2021-01-31-file-source-checkpointing
[grok_parser_transform]: /docs/reference/vrl/functions/#parse_grok
[json_parser_transform]: /docs/reference/vrl/functions/#parse_json
[key_value_parser_transform]: /docs/reference/vrl/functions/#parse_key_value
[logfmt_parser_transform]: /docs/reference/vrl/functions/#parse_logfmt
[merge_transform]: /docs/reference/vrl/functions/#merge
[pr_5281]: https://github.com/vectordotdev/vector/pull/5281
[pr_5978]: https://github.com/vectordotdev/vector/pull/5978
[filter_transform]: /docs/reference/configuration/transforms/filter/
[ignore_checkpoints]: /docs/reference/configuration/sources/file/#ignore_checkpoints
[read_from]: /docs/reference/configuration/sources/file/#read_from
[reduce_transform]: /docs/reference/configuration/transforms/reduce/
[regex_parser_transform]: /docs/reference/vrl/functions/#parse_regex
[remove_fields_transform]: /docs/reference/vrl/functions/#del
[remove_tags_transform]: /docs/reference/configuration/transforms/remap
[rename_fields_transform]: /docs/reference/configuration/transforms/remap
[route_transform]: /docs/reference/configuration/transforms/route/
[split_transform]: /docs/reference/vrl/functions/#split
[tokenizer_transform]: /docs/reference/vrl/functions/#parse_tokens
[vrl_announcement]: /blog/vector-remap-language/
[vrl_boolean_expression]: /docs/reference/vrl/expressions/#comparison
[vrl_reference]: /docs/reference/vrl/
