# Remove `http` source and `greptimedb` sink deprecated component aliases {#remove-deprecated-component-aliases}

## Summary

The deprecated `http` source and `greptimedb` sink aliases have been removed. They were deprecated in Vector 0.26.0 and 0.41.0, respectively.

## Migration

Change source type `http` to `http_server`, and sink type `greptimedb` to `greptimedb_metrics`.

authors: pront
