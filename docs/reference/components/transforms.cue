package metadata

components: transforms: [Name=string]: {
  kind: "transform"

  _features: tls: enabled: false

  // Example uses for the component.
  examples: {
    log: [
      ...{
        input: #Fields
      }
    ]
  }
}
