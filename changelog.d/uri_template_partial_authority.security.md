Fixed a bug where a URI template's `{{ field }}` reference landing inside the host would silently drop every event instead of failing to build.

Before, these built successfully but dropped every event at render time:

```yaml
sinks:
  my_sink:
    uri: "https://tenant.{{ env }}.example.com/"
```

```yaml
sinks:
  my_sink:
    uri: "https://api.internal{{ path }}"
```

Now, Vector refuses to build these configs, since the dynamic part of the URL sits inside (or right up against) the hostname rather than after a `/`. To fix, either add a static `/` before the dynamic part:

```yaml
sinks:
  my_sink:
    uri: "https://api.internal/{{ path }}"
```

or move the dynamic part fully into the path, keeping the hostname static:

```yaml
sinks:
  my_sink:
    uri: "https://tenant-{{ env }}.example.com/"
```

authors: thomasqueirozb
