package metadata

remap: functions: parse_spring_boot: {
	category: "Parse"
	description: #"""
		Parses the `value` as a spring boot log.
		"""#
	notices: [
		"""
            All values are returned as strings. It pasres spring boot log and returns timestamp, level(log level), pid, thread, logger, message.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string log to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["object"]

	examples: [
		{
			title: "Parse spring boot log"
			source: #"""
				pasrse_spring_boot("2023-01-30 22:37:33.495 INFO 72972 --- [ main] o.s.i.monitor.IntegrationMBeanExporter : Registering MessageChannel cacheConsumer-in-0")
				"""#
			return: {
				timestamp: "2023-01-30 22:37:33.495"
				level: "INFO"
				pid: "72972"
                thread: "main"
                logger: "o.s.i.monitor.IntegrationMBeanExporter"
                message: "Registering MessageChannel cacheConsumer-in-0"
			}
		},
		{
			title: "Parse spring boot error trace log"
			source: #"""
				pasrse_spring_boot("2023-01-30 22:37:33.495 ERROR 72972 --- [ main] o.s.i.monitor.IntegrationMBeanExporter : java.lang.NullPointerException: null\n\tat io.test.EmployerController.getAllEmployers(EmployerController.java:20) ~[classes/:na]")
				"""#
			return: {
				timestamp: "2023-01-30 22:37:33.495"
                level: "ERROR"
                pid: "72972"
                thread: "main"
                logger: "o.s.i.monitor.IntegrationMBeanExporter"
                message: "java.lang.NullPointerException: null\n\tat io.test.EmployerController.getAllEmployers(EmployerController.java:20) ~[classes/:na]"
			}
		},
	]
}