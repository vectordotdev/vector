Fixed a bug where a URI template's `{{ field }}` reference landing inside the host would silently drop every event instead of failing to build.

Before, this built successfully but dropped every event at render time:

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

Now, Vector refuses to build these configs, since the dynamic part of the URL sits inside (or right up against) the hostname rather than after a `/`. This is a breaking change for any config relying on the second pattern above, even if `path` always rendered with a leading `/`.

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
