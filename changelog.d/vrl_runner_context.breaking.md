# Update VRL runner implementations {#vrl-runner-context}

## Summary

`VrlRunner::new` now receives a `TransformContext` argument.

## Migration

Custom `VrlRunner` implementations must accept `&TransformContext` in `new`.
Implementations that do not use the context can ignore it.

authors: gwenaskell
