package metadata

components: transforms: lua: {
  title: "#{component.title}"
  short_description: "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."
  description: "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    function: "program"
  }

  statuses: {
    development: "beta"
  }

  support: {
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
    hooks: {
      common: true
      description: "Configures hooks handlers."
      required: true
        type: object: {
          examples: []
          options: {
            type: string: {
              default: null
              examples: ["init","init"]
            }
            type: string: {
              examples: ["function (event, emit)\n  event.log.field = \"value\" -- set value of a field\n  event.log.another_field = nil -- remove field\n  event.log.first, event.log.second = nil, event.log.first -- rename field\n\n  -- Very important! Emit the processed event.\n  emit(event)\nend","process","process"]
            }
            type: string: {
              default: null
              examples: ["shutdown","shutdown"]
            }
          }
        }
    }
    search_dirs: {
      common: false
      description: "A list of directories to search when loading a Lua file via the `require` function. If not specified, the modules are looked up in the directories of Vector's configs."
      required: false
        type: "[string]": {
          default: null
          examples: [["/etc/vector/lua"]]
        }
    }
    source: {
      common: false
      description: "The source which is evaluated when the transform is created."
      required: false
        type: string: {
          default: null
          examples: ["function init()\n  count = 0\nend\n\nfunction process()\n  count = count + 1\nend\n\nfunction timer_handler(emit)\n  emit(make_counter(counter))\n  counter = 0\nend\n\nfunction shutdown(emit)\n  emit(make_counter(counter))\nend\n\nfunction make_counter(value)\n  return metric = {\n    name = \"event_counter\",\n    kind = \"incremental\",\n    timestamp = os.date(\"!*t\"),\n    counter = {\n      value = value\n    }\n  }\nend","-- external file with hooks and timers defined\nrequire('custom_module')"]
        }
    }
    timers: {
      common: false
      description: "Configures timers which are executed periodically at given interval."
      required: false

    }
    version: {
      common: true
      description: "Transform API version. Specifying this version ensures that Vector does not break backward compatibility."
      required: true
        type: string: {
          enum: {
            2: "Lua transform API version 2"
          }
        }
    }
  }
}