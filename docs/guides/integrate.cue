package metadata

#ComponentConfig: {
	_args: {
		component: _
	}
}

guides: integrate: {
	sources: {
		for source_type, source in components.sources {
			"\(source_type)": {
				config: {
					sources: in: {
						for config_name, source_config in source.configuration {
							"\( config_name )"?: _ | *null
						}
					}
				}
				sinks: {
					for sink_type, sink in components.sinks {
						"\(sink_type)": {
							if sink.input.logs == true && source.output.logs != _|_ {
								config: {
									sources: in: {
										type: source_type
									}
									sinks: out: {
										type: sink_type
										inputs: ["in"]
									}
								}
							}
						}
					}
				}
			}
		}
	}

	sinks: {
		for type, component in components.sinks {
			"\(type)": "hi"
		}
	}
}
