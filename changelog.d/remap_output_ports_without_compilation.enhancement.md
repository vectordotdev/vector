Loading and reloading configurations with many or large `remap` transforms is now significantly
faster. Vector no longer compiles every `remap` program while building the configuration graph just
to look up the transform's output ports, so a reload no longer recompiles every program in the
configuration.

authors: ccdd4ever
