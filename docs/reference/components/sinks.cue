package metadata

components: sinks: [Name=string]: {
  kind: "sink"

  _features: {
    batch: {
      enabled: bool
      common: bool,
      max_bytes: uint | null,
      max_events: uint | null,
      timeout_secs: uint8
    }
    buffer: enabled: bool
  }
}
