Fixed a bug in the topology builder causing component metrics registered at build
time to miss the component tags if the component build function awaits non-trivially.

This notably affected sinks using a disk buffer, and source or sinks performing
IO work in the build function.

authors: gwenaskell
