# Boolean Vector sink compression removed {#vector-compression-bool-removed}

## Summary

The deprecated boolean syntax for the `vector` sink's `compression` option has
been removed.

## Migration

Replace `compression: true` with `compression: "gzip"`, and replace
`compression: false` with `compression: "none"`.

authors: pront
