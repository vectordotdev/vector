# RFC <issue#> - 2022-02-xx - Datadog Agent style secret management

The Datadog Agent has a straighforward secret resolution facility to avoid having sensitive information stored directly
in its config, it relies on a user-provided external program that is run to retrieve sensitive value from a third party
system. This RFC aims to propose a similar mechanism for Vector.

## Context

The [Datadog Agent documentation][dd-agent-secret-mgmt] provide all user-relevant information to use that feature. It
covers the specification for the user-provided executable that loads/decrypts secrets along with the agent configuration
and the syntax to retrieve encrypted config value.

## Cross cutting concerns

- The ongoing [configuration schema work][vector-config-schema-work].
- `vector config` and related Vector enterprise work.

## Scope

### In scope

- User shall be able to use the same kind of executable to load/decrypt secrets for Vector.
- This new feature will have a deterministic behaviour when used in conjunction with templates.
- Situations like topology reload and load/decryption failure will be accounted for.

### Out of scope

- Integration with other secret or distribution configuration like vault, however this RFC will account for that kind of
  future extension

## Pain

- As of today, secrets like authentication tokens and passwords should be provided inside the topology configuration in
  plain text or in [environment variables][env-var-in-vector-config] and that may not be acceptable in some
  circumstances.
- Decoupling secret management and key rotation from configuration management.

## Proposal

### User Experience

- Use the same kind of API between Vector and a user-provided executable as the [one between the Agent and the secret
  retrieving executable][dd-secret-backend-exec-api].
- A set of top level options like the ones the Datadog Agent [exposes][dd-agent-secret-knobs].
- A convenient syntax for config option to indicate Vector that a secret should be retrieved for this option (Subject to
  be changed but the Datadog Agent uses `ENC[secret_key]`)

Datadog Secret API, as per the official [doc][dd-secret-backend-exec-api]:

- Communication happens on stdout/sdin
- It uses plain text json

Vector would run the user-provided executable and feed its standard input with the list of secrets to retrieve:

```json
{"version": "1.0", "secrets": ["secret1", "secret2"]}
```

The version used by the Datadog agent is 1.0. The user provided executable should then reply on its standard output with
a JSON formatted string:

```json
{
  "secret1": {"value": "secret_value", "error": null},
  "secret2": {"value": null, "error": "could not fetch the secret"}
}
```

**Note**: The version field can be used to introduce specific behaviour, one major useful thing that could be introduced
is the ability for the user provided executable to provide a secrete expiration date. This is mentioned in the
improvements section.

New top level options to be added will be sitting inside the `secret` namespace, this would lead to something like:

```toml
[secret.local_exec]
type = "exec"
path = "/path/to/the/command"
argument = "--config foo=bar"
timeout = 5

[secret.prod_vault]
type = "vault"
address = "https://vault.corp.tld/"
token = "secret://local_exec/vault_token"
timeout = 5

[sources.system_logs]
type = "file"
includes = ["/var/log/system.log"]

[sinks.app_logs]
type = "datadog_logs"
default_api_key = "secret://prod_vault/dd_api_2022_02"
inputs = ["system_logs"]
```

The first implementation would only support the `exec` backend but with extensibility point clearly identified to easily
implement additional backend if needed.

Overall the behaviour for corner cases should follow what's in place for environmment variable interpolation as this is
a very close feature.

### Implementation

A secret backend that would lie in `./src/config/secret.rs` and would call the user provided executable for secrets,
cache secrets to avoid calling the backend for the same key multiple time. It would read the configuration file once to
get its config before further processing, ideally env var interpolating should be supported (this should not be a
problem). In `./src/config/builder.rs`, `load_builder_from_paths` will still be returning a complete configuration with
placeholders replaced by secrets.

The `ConfigBuilder` struct will get a new `secret_backend` field (type to something like `SecretBackend`), this means
that `load_builder_from_paths` will then assert if this field is present before returning to the caller, and if it, the
config will be reloaded with this `SecretBackend` passed to downstream callee and hook the secret interpolation around
the same point as [the environment variable interpolation][env-var-hook] it would then query this `SecretBackend` for
each `ENC[secret_key]`.

The implementation should ease future extension and split the internal API queried by the interpolation logic and the
secret provider that may see other implementation like: `executable` (the one documented in this RFC), `vault`,
`k8s-config-map`, `aws secretsmanager`, etc.

## Rationale

- Some users just can't put sensitive information inside their configuration.
- Using environment variable violates security requirements for sensitive environments since environment variables can
  be leaked by an attacker and access to those variables is complex to audit.

## Drawbacks

- Integrating with other third party tools directly like Vault would provide better error management and avoid relying on a custom, user-provided binary.
- This binary might have to be injected into container images which may be inconvenient, other options like using an external volume may be more acceptable but it would still involve a third party executable and all the associated risks.

## Prior Art

- The Datadog Agent has exactly the same feature, despite its simple approach it works reasonnably well, but it is
  cannot easily support advanced secret management like certificate distribution/revocation, key rotation, etc.
- Vault is the standard in the industry, and it comes with all kind of advanced features that cannot really supported
  by the user-provided executable solution.

## Alternatives

- Integrate with other third party tools: Vault and CSP APIs for secret management to start with.
- Stick to environment variables interpolation and leverage [K8s ability to expose secret][k8s-env-var-from-secrets] as
  environment variables, relevant examples are already in the [Vector helm char][env-var-from-k8s-secrets]. Note that
  the Datadog Agent is now capable to [do that out-of-the-box][dd-agent-with-k8s-secret].

Note: doing nothing is not really an alternative here, as plain text secret in config is a strong blocker for some
users.

## Outstanding Questions

- Sticking to env var from K8s secret still seems a reasonnable approach as K8s is the reference deployement in many
  situations.
- Secret syntax in config, the Datadog Agent uses `ENC[secret_key]`, whereas an URL scheme like `secret://<backend>/<key>`
  may be a more extensible and futur proof scheme. It would easily provide a convenient user facing syntax if multiple
  backen are required in the future.
- Specific security constraints that may have been missed.

## Plan Of Attack

- [ ] Implement the secret backend logic with the minimal set of options.
- [ ] Document typical usecases.

## Future Improvements

- Support additional backend.
- Embed/implement helpers like the [Agent][dd-agent-secret-helper].
- Possible extention to the API, it will mostly depends on user feedback


[dd-agent-secret-mgmt]: https://docs.datadoghq.com/agent/guide/secrets-management/
[dd-agent-secret-knobs]: https://github.com/DataDog/datadog-agent/blob/abc8351/pkg/config/config.go#L356-L362
[env-var-hook]: https://github.com/vectordotdev/vector/blob/ed0ca37/src/config/loading.rs#L414
[k8s-env-var-from-secrets]: https://kubernetes.io/docs/concepts/configuration/secret/#using-secrets-as-environment-variables
[dd-agent-with-k8s-secret]: https://docs.datadoghq.com/agent/guide/secrets-management/?tab=linux#script-for-reading-from-multiple-secret-providers
[dd-agent-secret-helper]: https://github.com/DataDog/datadog-agent/tree/331a3fc2c6f4f49f9bcc06c4f0675f6a8b65a523/cmd/secrets
[vector-config-schema-work]: https://github.com/vectordotdev/vector/issues/9115
[dd-secret-backend-exec-api]: https://docs.datadoghq.com/agent/guide/secrets-management/?tab=linux#the-executable-api
[env-var-from-k8s-secrets]: https://github.com/vectordotdev/helm-charts/blob/5a92272/charts/vector/values.yaml#L131-L143
[env-var-in-vector-config]: https://vector.dev/docs/reference/configuration/#environment-variables