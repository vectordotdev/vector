---
date: "2022-06-07"
title: "Updates to event metadata and secrets"
description: "Updates to event metadata and secrets"
authors: ["fuchsnj"]
pr_numbers: [12767]
release: "0.23.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

VRL already contained three methods for working with metadata.

- `set_metadata_field`
- `get_metadata_field`
- `remove_metadata_field`

However, these were restricted to only storing a datadog api key, or splunk hec token, which were used by some sinks.
These functions have been expanded to support using arbitrary keys (the full path syntax is supported) and can also
store arbitrary data, similar to normal event data today.

To replace the existing functionality these provided, new "secret" functions have been added.

- `set_secret`
- `get_secret`
- `remove_secret`

These functions allow storage of arbitrary secrets, and should now be used instead of the metadata functions.
They securely store a map of string keys to string values.

All of these changes were backwards compatible. Secrets that were previously stored using the metadata functions
will still be used by sinks, and will also be accessible through the new secret functions. It is encouraged
to upgrade to the secret functions immediately.

Here are some examples using the new features

```coffeescript
# This is existing functionality that will continue to work. Setting the datadog API key
set_metadata_field("datadog_api_key", "my secret key")

# This is the new way to set secrets
set_secret("datadog_api_key", "my secret key")

# Both of these will return the key set above
get_metadata_field("datadog_api_key")
get_secret("datadog_api_key")

# These are new features added to the metadata functions. Notice the key is a path and not a string
set_metadata_field(.foo.bar, "my metadata")
set_metadata_field(.foo.baz, {"msg": "Any VRL type is supported"})

# All metadata can be retrieved this way
get_metadata_field(.)

```

## Future Plans

These changes are forming the basis for many future plans we have. These include:

- Adding special syntax to access metadata. Right now you have to use the metadata functions, but we'd like to improve that even more
- Expanding VRL type definition support to metadata. Right now values may be more difficult to work with since they are lacking type definitions
- Add more security around secret storage, such as potentially encryption at rest.
