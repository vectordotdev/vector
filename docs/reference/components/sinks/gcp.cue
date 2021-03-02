package metadata

import (
	"strings"
)

components: sinks: [Name=string]: {
	if strings.HasPrefix(Name, "gcp_") {
		env_vars: {
			GOOGLE_APPLICATION_CREDENTIALS: {
				description:   "The filename for a Google Cloud service account credentials JSON file used for authentication."
				relevant_when: "endpoint = null"
				type: string: {
					default: null
					examples: ["/path/to/credentials.json"]
					syntax: "literal"
				}
			}
		}

		how_it_works: {
			gcp_authentication: {
				title: "GCP Authentication"
				body:  """
						GCP offers a [variety of authentication methods](\(urls.gcp_authentication)) and
						Vector is concerned with the [server to server methods](\(urls.gcp_authentication_server_to_server))
						and will find credentials in the following order:

						1. If the [`credentials_path`](#credentials_path) option is set.
						1. If the `api_key` option is set.
						1. If the [`GOOGLE_APPLICATION_CREDENTIALS`](#google_application_credentials) envrionment variable is set.
						1. Finally, Vector will check for an [instance service account](\(urls.gcp_authentication_service_account)).

						If credentials are not found the [healtcheck](#healthchecks) will fail and an
						error will be [logged][docs.monitoring#logs].
						"""
			}
		}
	}
}
