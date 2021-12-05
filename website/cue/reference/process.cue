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

	exit_codes: {
		description: string
		codes:       #ExitCodes
	}

	process_signals: {
		description: string
		signals:     #Signals
	}

	exit_codes: {
		description: """
			You can find a full list of exit codes in the [`exitcodes` Rust crate](\(urls.exit_codes)). Vector uses the
			codes listed in the table below.
			"""

		codes: {
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
	}

	process_signals: {
		description: """
			The Vector is built to handle the inter-process communication [signals](\(urls.signal)) listed in the
			table below.
			"""

		signals: {
			SIGHUP: {
				description: "Reloads configuration on the fly."
			}

			SIGTERM: {
				description: "Initiates graceful shutdown process."
			}
		}
	}
}
