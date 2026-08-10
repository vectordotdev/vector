`vector validate --no-environment` now catches sink configuration errors that previously
only surfaced when Vector booted, including template path-confinement violations. Validation
is pure and does not access the filesystem, so codecs that read files at startup (such as
protobuf descriptor sets) are only checked when Vector builds the sink.

authors: thomasqueirozb
