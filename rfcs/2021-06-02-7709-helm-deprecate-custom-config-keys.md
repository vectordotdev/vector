# RFC 7709 - 2021-06-02 - Helm: Deprecate "custom" configuration keys

Today the templates that user's leverage to generate their Vector config are separate from the ones used to generate the "default" Vector configuration.
While these are not very complex in their functionality they do duplicate much of the same logic.

## Scope

This RFC will cover only the existing "default" `sinks` and `sources` defined in our Helm charts today.

vector-agent:

- `kubernetes_logs`
- `vector`
- `internal_metrics`
- `host_metrics`
- `prometheus_exporter`

vector-aggregator:

- `vector`
- `internal_metrics`
- `prometheus_exporter`

## Motivation

We're duplicating the templating used to generate Vector's configuration file in Helm based deployments. While I was working on transitioning the
currently TOML config to YAML realized the configuration building had several unexpected complications due to the separation between Vector's provided
"default" configurations (listed in the Scope section) and a user provided configuration located under the `sources`, `transforms`, and `sinks` keys.

## Internal Proposal

The "custom" `values.yaml` keys be deprecated and replaced with default values under the `sources`, `transforms`, and `sinks` keys that users configure
for their provided Vector configuration. An example is below:

```yaml
-  internalMetricsSource:
-    enabled: true
-    sourceId: internal_metrics
-    config: {}
-  hostMetricsSource:
-    enabled: true
-    sourceId: host_metrics
-    config:
-      filesystem:
-        devices:
-          excludes: [binfmt_misc]
-        filesystems:
-          excludes: [binfmt_misc]
-        mountpoints:
-          excludes: ["*/proc/sys/fs/binfmt_misc"]
+  sources:
+    internal_metrics:
+      type: internal_metrics
+    host_metrics:
+      type: host_metrics
+      filesystem:
+        devices:
+          excludes: ["binfmt_misc"]
+        filesystems:
+          excludes: ["binfmt_misc"]
+        mountpoints:
+          excludes: ["*/proc/sys/fs/binfmt_misc"]
```

## Doc-level Proposal

The only page we currently have with documentation around Helm is on the Kubernetes platform installation page, which today does not mention how to configure
any of the default components.

## Rationale

This has us use the same configuration levers as our users do, and unifies _how_ Vector can be configured in Kubernetes. The default values can also be used
as "in-line" documentation users can refer to as an example configuration. This also reduces the amount of changes needed if we need to adjust configuration
templating in the future, reducing some long term burden (slight, and I can't imagine that will change much, or ever, once we stop converting the config into TOML).

## Drawbacks

This change does make opting in or out to our default configurations slightly more difficult, since there would no longer be an "enabled" toggle. To opt in
(unless we move to have all the configurations active by default) uncomment our provided configuration. To opt out the user would need to pass the following (based on example above):

```yaml
sources:
  internal_metrics: null
  host_metrics: null
```

## Alternatives

- Do nothing - as noted it's not a _huge_ amount of burden but it does make our planned transition to configuring Vector with a YAML file more tricky.
- Change the component keys to arrays, which are not merged when providing overrides - but forces the user to replace all entries if they set custom values.

## Plan Of Attack

- [ ] Announce plan to deprecate the existing keys
- [ ] Update Helm charts to have all of the "defaults" under the generic `sources`, `transforms`, and `sinks` keys (commented out to not interfere with existing configurations)
- [ ] Phase our the "custom" keys in an upcoming release by defaulting to the generic keys and setting all "defaults" to have `enabled = false`
- [ ] Remove the eight existing "custom" keys we use for configuration
