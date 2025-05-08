Added `relaxed_wildcard_matching` global config option to enable configurations with wildcards that do not match any inputs to be accepted without causing an error.

Example config:

```yaml
relaxed_wildcard_matching: true

sources:
  stdin:
    type: stdin

# note - no transforms

sinks:
  stdout:
    type: console
    encoding:
      codec: json
    inputs:
      - "runtime-added-transform-*"

```

authors: simplepad
