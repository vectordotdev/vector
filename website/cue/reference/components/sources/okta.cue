package metadata

components: sources: okta: {

	title: "Okta"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		multiline: enabled: false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.okta.configuration

	output: logs: event: {
		description: "An Okta system log event"
		fields: {
			"*": {
				description: "fields from the Okta system log"
				required:    true
				type: object: {examples: [{
					"actor": {
						"id":          "00util3j01jqL21aM1d6"
						"type":        "User"
						"alternateId": "john.doe@example.com"
						"displayName": "John Doe"
						"detailEntry": null
					}
					"client": {
						"userAgent": {
							"rawUserAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36"
							"os":           "Mac OS X"
							"browser":      "CHROME"
						}
						"zone":      null
						"device":    "Computer"
						"id":        null
						"ipAddress": "10.0.0.1"
						"geographicalContext": {
							"city":       "New York"
							"state":      "New York"
							"country":    "United States"
							"postalCode": 10013
							"geolocation": {
								"lat": 40.3157
								"lon": -74.01
							}
						}
					}
					"device": {
						"id":                      "gu1fd8yj3x1feOg3N1d9"
						"name":                    "Mac15,6"
						"os_platform":             "OSX"
						"os_version":              "14.6.0"
						"managed":                 false
						"registered":              true
						"device_integrator":       null
						"disk_encryption_type":    "ALL_INTERNAL_VOLUMES"
						"screen_lock_type":        "BIOMETRIC"
						"jailbreak":               null
						"secure_hardware_present": true
					}
					"authenticationContext": {
						"authenticationProvider": null
						"credentialProvider":     null
						"credentialType":         null
						"issuer":                 null
						"interface":              null
						"authenticationStep":     0
						"rootSessionId":          "idxBagel62CatsUkTankATonA"
						"externalSessionId":      "idxBagel62CatsUkTankATonA"
					}
					"displayMessage": "User login to Okta"
					"eventType":      "user.session.start"
					"outcome": {
						"result": "SUCCESS"
						"reason": null
					}
					"published": "2024-08-13T15:58:20.353Z"
					"securityContext": {
						"asNumber": 394089
						"asOrg":    "ASN 0000"
						"isp":      "google"
						"domain":   null
						"isProxy":  false
					}
					"severity": "INFO"
					"debugContext": {
						"debugData": {
							"requestId":  "ab609228fe84ce59cd3bf4690bcce016"
							"requestUri": "/idp/idx/authenticators/poll"
							"url":        "/idp/idx/authenticators/poll"
						}
					}
					"legacyEventType": "core.user_auth.login_success"
					"transaction": {
						"type":   "WEB"
						"id":     "ab609228fe84ce59cat7fa690big3016"
						"detail": null
					}
					"uuid":    "dc9fd3c0-598c-11ef-8478-2b7584bf8d5a"
					"version": 0
					"request": {
						"ipChain": [
							{
								"ip": "10.0.0.1"
								"geographicalContext": {
									"city":       "New York"
									"state":      "New York"
									"country":    "United States"
									"postalCode": 10013
									"geolocation": {
										"lat": 40.3157
										"lon": -74.01
									}
								}
								"version": "V4"
								"source":  null
							},
						]
					}
					"target": [
						{
							"id":          "p7d7dh1jf0HM0kP2e1d7"
							"type":        "AuthenticatorEnrollment"
							"alternateId": "unknown"
							"displayName": "Okta Verify"
							"detailEntry": null
						},
						{
							"id":          "0oatLeaf9sQv1qInq5d6"
							"type":        "AppInstance"
							"alternateId": "Okta Admin Console"
							"displayName": "Okta Admin Console"
							"detailEntry": null
						}]
				}]
				}
			}
		}
	}

	how_it_works: {
		api_token: {
			title: "API Token"
			body: """
				The `okta` source uses the Okta HTTP API, you will need to generate an API token in the
				Okta admin console with sufficient permissions.
				"""
		}
		lookback: {
			title: "Lookback & Polling"
			body: """
				The `okta` source polls Okta for new log events, by default beginning at the current time on
				startup, following the API's pagination links for the next interval.

				The `since` parameter begins fetching logs generated prior to Vector's startup
				"""
		}
	}
}
