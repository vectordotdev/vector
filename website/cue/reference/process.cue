package metadata

process: {
	#ExitCode: {
		code:        int
		description: string
	}

	#ExitCodes: [Name=string]: #ExitCode

	#Signal: {
		description: string
		name:        string
	}

	#Signals: [Name=string]: #Signal & {name: Name}

	exit_codes: #ExitCodes
	signals:    #Signals

	exit_codes: {
		"0": {
			code:        0
			description: "Exited successfully."
		}
		"1": {
			code:        1
			description: "Exited with a generic error."
		}
		"78": {
			code:        78
			description: "Configuration is invalid."
		}
	}

	signals: {
		SIGHUP: {
			description: "Reloads configuration on the fly."
		}

		SIGTERM: {
			description: "Initiates graceful shutdown process."
		}
	}
}
