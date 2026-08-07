# URI template field references inside the authority are rejected {#uri-template-partial-authority}

## Summary

Vector now refuses to build configs where a `{{ field }}` reference lands inside
the hostname (or immediately adjacent to it without a path separator). Previously,
such templates built successfully but silently dropped every event at render time.

## Migration

If `{{ field }}` appears inside the host or directly after the host with no leading
`/`, add a static `/` before the dynamic segment or move the dynamic part into the
path with a static hostname.

#### Old (silently dropped events)

This built successfully but dropped every event at render time:

```yaml
sinks:
  my_sink:
    uri: "https://tenant.{{ env }}.example.com/"
```

This also built successfully, but only worked correctly if `path` always rendered with its own leading `/` (e.g. `/v1`); otherwise it silently dropped every event:

```yaml
sinks:
  my_sink:
    uri: "https://api.internal{{ path }}"
```

#### New

To fix, either add a static `/` before the dynamic part:

```yaml
sinks:
  my_sink:
    uri: "https://api.internal/{{ path }}"
```

or move the dynamic part fully into the path, keeping the hostname static:

```yaml
sinks:
  my_sink:
    uri: "https://example.com/tenant/{{ env }}/"
```

authors: thomasqueirozb
