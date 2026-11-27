The Azure Blob sink now supports the `api_version` option to override the `x-ms-version` request
header for Azure Stack Hub and other compatible storage services. When unset, Shared Key
authentication continues to use `2025-11-05`, and other authentication methods retain the Azure
SDK default.

authors: opencow
