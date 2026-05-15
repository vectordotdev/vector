Deprecation notices are now tracked in [`deprecation.d/`](../deprecation.d/) at the root of
the repository.

Each file in that directory is a self-contained deprecation notice with YAML frontmatter.
See [`deprecation.d/README.md`](../deprecation.d/README.md) for the file format and lifecycle.

To view all current notices, run:

```shell
cargo vdev deprecation show
```

To validate the directory, run:

```shell
cargo vdev check deprecations
```
