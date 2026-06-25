The `file` sink now rejects startup when `path` contains `{{ field }}`
references but has no usable literal directory prefix to derive a confinement
base from.

A new `base_dir` config field can be used to set the confinement root
explicitly when the `path` template has no usable literal prefix.

To restore the previous (unconfined) behavior instead, set:

```yaml
dangerously_allow_unconfined_template_resolution: true
```

authors: pront
