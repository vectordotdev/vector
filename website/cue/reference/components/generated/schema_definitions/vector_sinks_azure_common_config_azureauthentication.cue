package metadata

_schemaDefinitions: "vector::sinks::azure_common::config::AzureAuthentication": object: options: {
	azure_client_id: {
		description: """
			The [Azure Client ID][azure_client_id].

			[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
			"""
		relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
		required:      true
		type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID:?err}"]
	}
	azure_client_secret: {
		description: """
			The [Azure Client Secret][azure_client_secret].

			[azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
			"""
		relevant_when: "azure_credential_kind = \"client_secret_credential\""
		required:      true
		type: string: examples: ["00-00~000000-0000000~0000000000000000000", "${AZURE_CLIENT_SECRET:?err}"]
	}
	azure_credential_kind: {
		description: "The kind of Azure credential to use."
		required:    true
		type: string: enum: {
			azure_cli:                         "Use Azure CLI credentials"
			client_certificate_credential:     "Use certificate credentials"
			client_secret_credential:          "Use client ID/secret credentials"
			managed_identity:                  "Use Managed Identity credentials"
			managed_identity_client_assertion: "Use Managed Identity with Client Assertion credentials"
			workload_identity:                 "Use Workload Identity credentials"
		}
	}
	azure_tenant_id: {
		description: """
			The [Azure Tenant ID][azure_tenant_id].

			[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
			"""
		relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
		required:      true
		type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID:?err}"]
	}
	certificate_password: {
		description:   "The password for the client certificate, if applicable."
		relevant_when: "azure_credential_kind = \"client_certificate_credential\""
		required:      false
		type: string: examples: ["${AZURE_CLIENT_CERTIFICATE_PASSWORD}"]
	}
	certificate_path: {
		description:   "PKCS12 certificate with RSA private key."
		relevant_when: "azure_credential_kind = \"client_certificate_credential\""
		required:      true
		type: string: examples: ["path/to/certificate.pfx", "${AZURE_CLIENT_CERTIFICATE_PATH:?err}"]
	}
	client_assertion_client_id: {
		description:   "The target Client ID to use."
		relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
		required:      true
		type: string: examples: ["00000000-0000-0000-0000-000000000000"]
	}
	client_assertion_tenant_id: {
		description:   "The target Tenant ID to use."
		relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
		required:      true
		type: string: examples: ["00000000-0000-0000-0000-000000000000"]
	}
	client_id: {
		description: """
			The [Azure Client ID][azure_client_id]. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.

			[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
			"""
		relevant_when: "azure_credential_kind = \"workload_identity\""
		required:      false
		type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID}"]
	}
	tenant_id: {
		description: """
			The [Azure Tenant ID][azure_tenant_id]. Defaults to the value of the environment variable `AZURE_TENANT_ID`.

			[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
			"""
		relevant_when: "azure_credential_kind = \"workload_identity\""
		required:      false
		type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID}"]
	}
	token_file_path: {
		description:   "Path of a file containing a Kubernetes service account token. Defaults to the value of the environment variable `AZURE_FEDERATED_TOKEN_FILE`."
		relevant_when: "azure_credential_kind = \"workload_identity\""
		required:      false
		type: string: examples: ["/var/run/secrets/azure/tokens/azure-identity-token", "${AZURE_FEDERATED_TOKEN_FILE}"]
	}
	user_assigned_managed_identity_id: {
		description:   "The User Assigned Managed Identity to use."
		relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
		required:      false
		type: string: examples: ["00000000-0000-0000-0000-000000000000"]
	}
	user_assigned_managed_identity_id_type: {
		description: """
			The type of the User Assigned Managed Identity ID provided (Client ID, Object ID,
			or Resource ID). Defaults to Client ID.
			"""
		relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
		required:      false
		type: string: enum: {
			client_id:   "Client ID"
			object_id:   "Object ID"
			resource_id: "Resource ID"
		}
	}
}
