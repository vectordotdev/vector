package metadata

components: transforms: concat: {
  title: "Concat"
  short_description: "Accepts log events and allows you to concat (substrings) of other fields to a new one."
  long_description: "Accepts log events and allows you to concat (substrings) of other fields to a new one."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "schema"
  }

  statuses: {
    development: "beta"
  }

  support: {
      input_types: ["log"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: []
    warnings: []
  }

  configuration: {
    items: {
      description: "A list of substring definitons in the format of source_field[start..end]. For both start and end negative values are counted from the end of the string."
      required: true
      warnings: []
      type: "[string]": {
        examples: [["first[..3]","second[-5..]","third[3..6]"]]
      }
    }
    joiner: {
      common: false
      description: "The string that is used to join all items."
      required: false
      warnings: []
      type: string: {
        default: " "
        examples: [" ",",","_","+"]
      }
    }
    target: {
      description: "The name for the new label."
      required: true
      warnings: []
      type: string: {
        examples: ["root_field_name","parent.child","array[0]"]
      }
    }
  }
}
