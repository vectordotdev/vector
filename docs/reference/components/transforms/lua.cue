package metadata

components: transforms: lua: {
  title: "#{component.title}"
  short_description: "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."
  long_description: "Accepts log and metric events and allows you to transform events with a full embedded [Lua][urls.lua] engine."

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
      input_types: ["log","metric"]

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
      description: "Configures hooks handlers."
      groups: ["simple","inline","module"]
      required: true
      warnings: []
      type: object: {
        examples: []
        options: {
          init: {
            common: false
            description: "A function which is called when the first event comes, before calling `hooks.process`"
            groups: ["inline","module"]
            required: false
            warnings: []
            type: string: {
              default: null
              examples: ["init","init"]
            }
          }
          process: {
            description: "A function which is called for each incoming event. It can produce new events using `emit` function."
            groups: ["simple","inline","module"]
            required: true
            warnings: []
            type: string: {
              examples: ["function (event, emit)\n  event.log.field = \"value\" -- set value of a field\n  event.log.another_field = nil -- remove field\n  event.log.first, event.log.second = nil, event.log.first -- rename field\n\n  -- Very important! Emit the processed event.\n  emit(event)\nend","process","process"]
            }
          }
          shutdown: {
            common: false
            description: "A function which is called when Vector is stopped. It can produce new events using `emit` function."
            groups: ["inline","module"]
            required: false
            warnings: []
            type: string: {
              default: null
              examples: ["shutdown","shutdown"]
            }
          }
        }
      }
    }
    search_dirs: {
      common: false
      description: "A list of directories to search when loading a Lua file via the `require` function. If not specified, the modules are looked up in the directories of Vector's configs."
      groups: ["module"]
      required: false
      warnings: []
      type: "[string]": {
        default: null
        examples: [["/etc/vector/lua"]]
      }
    }
    source: {
      common: false
      description: "The source which is evaluated when the transform is created."
      groups: ["inline","module"]
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["function init()\n  count = 0\nend\n\nfunction process()\n  count = count + 1\nend\n\nfunction timer_handler(emit)\n  emit(make_counter(counter))\n  counter = 0\nend\n\nfunction shutdown(emit)\n  emit(make_counter(counter))\nend\n\nfunction make_counter(value)\n  return metric = {\n    name = \"event_counter\",\n    kind = \"incremental\",\n    timestamp = os.date(\"!*t\"),\n    counter = {\n      value = value\n    }\n  }\nend","-- external file with hooks and timers defined\nrequire('custom_module')"]
      }
    }
    timers: {
      common: false
      description: "Configures timers which are executed periodically at given interval."
      groups: ["inline","module"]
      required: false
      warnings: []
    }
    version: {
      description: "Transform API version. Specifying this version ensures that Vector does not break backward compatibility."
      groups: ["simple","inline","module"]
      required: true
      warnings: []
      type: string: {
        enum: {
          2: "Lua transform API version 2"
        }
      }
    }
  }
}
