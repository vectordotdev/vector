# The `mongodb_metrics` source requires MongoDB 4.4 or newer {#mongodb-metrics-min-server-4-4}

## Summary

The `mongodb` driver is updated to 3.9.1, which removes support for MongoDB 4.2. The
`mongodb_metrics` source now requires MongoDB server 4.4 or newer (wire version 9 or newer).
Connections to MongoDB 4.2 or older servers now fail with an incompatible server error. MongoDB 4.2
reached end of life on April 30, 2023.

## Migration

Upgrade the monitored MongoDB servers to version 4.4 or newer before you upgrade Vector.

authors: thomasqueirozb
