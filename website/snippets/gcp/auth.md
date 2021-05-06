GCP offers a [variety of authentication methods][auth_methods] and Vector is concerned with the [server-to-server methods][server_to_server] and will find credentials in the following order:

1. If the [`credentials_path`](#credentials_path) option is set.
1. If the `api_key` option is set.
1. If the [`GOOGLE_APPLICATION_CREDENTIALS`](#GOOGLE_APPLICATION_CREDENTIALS) envrionment variable is set.
1. Finally, Vector will check for an [instance service account][account]. If credentials are not found the [healt check](#health-checks) will fail and an error will be [logged][logs].

[account]: https://cloud.google.com/docs/authentication/production#obtaining_and_providing_service_account_credentials_manually
[auth_methods]: https://cloud.google.com/docs/authentication
[logs]: /docs/administration/monitoring/#logs
[server_to_server]: https://cloud.google.com/docs/authentication/production
