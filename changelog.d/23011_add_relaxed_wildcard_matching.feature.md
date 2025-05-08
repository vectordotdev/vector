Added `wildcard_matching` global config option to set wildcard matching mode for inputs. Relaxed mode allows configurations with wildcards that do not match any inputs to be accepted without causing an error.

Example config:

```yaml
wildcard_matching: relaxed

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
