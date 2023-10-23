---
title: TLS configuration
short: TLS configuration
weight: 5
aliases: [
  "/docs/reference/tls",
  "/docs/reference/configuration/tls",
]
---

Vector implements cryptography and secure communication using the [OpenSSL][openssl] library.
In particular, the official Vector binaries are statically linked against OpenSSL version
{{< param openssl_version >}} and do not use any OpenSSL library installed on the running system.

**Note**: OpenSSL recognizes a number of [environment variables][openssl-env] independently of Vector.

## Trusted certificates

Trusted certificates (also called certificate authorities) are used for client and server verification.

By default, OpenSSL looks for trusted certificates in the following locations:

* A single file containing several certificates specified by the `SSL_CERT_FILE` environment variable.
* A directory containing multiple certificate files specified by the `SSL_CERT_DIR` environment variable.

In addition, Vector also looks for trusted certificates in the following locations:

* Probing of common default locations widely used by current operating systems.
  * This probing functionality is provided to Vector by the [`openssl-probe`][openssl-probe] Rust crate.
  * Trusted certificate location probing can be disabled by using the `--openssl-no-probe` command line
    flag or the `VECTOR_OPENSSL_NO_PROBE` environment variable (refer to the [CLI][cli] documentation).

**Note:** It is possible to use specific trusted certificates only for Vector using `SSL_CERT_FILE` or `SSL_CERT_DIR`.

## OpenSSL configuration

The OpenSSL library in Vector can be configured using a [configuration file][openssl-config].

By default, OpenSSL looks for a configuration file in the following locations:

* A configuration file specified by the `OPENSSL_CONF` environment variable.
* The predefined `/usr/local/ssl/openssl.cnf` configuration file.

**Note**: It is possible to use specific OpenSSL configurations only for Vector using the `OPENSSL_CONF` variable.

## OpenSSL implementation providers

In OpenSSL, a [provider][openssl-providers] is a code module that provides one or more implementations
for various operations and algorithms used for cryptography and secure communication.

OpenSSL provides a number of its own providers. The most important ones for Vector are:

* The [default][openssl-providers-default] provider. This provider is built in as part of the _libcrypto_
  library and contains all of the most commonly used modern and secure algorithm implementations.
* The [legacy][openssl-providers-legacy] provider. This provider is a dynamically loadable module, and must
  therefore be loaded and configured explicitly, using an [OpenSSL configuration](#openssl-configuration).
  It contains algorithm implementations that are considered insecure, or are no longer in common use such as MD2 or RC4.
* The [FIPS][openssl-providers-fips] provider. This provider is a dynamically loadable module, and must
  therefore be loaded and configured explicitly, using an [OpenSSL configuration](#openssl-configuration).
  It contains algorithm implementations that have been validated according to the [FIPS 140-2][fips-140-2] standard.

By default, the OpenSSL library in Vector uses the _default_ provider which includes modern and secure
algorithm implementations. If necessary, the _legacy_ provider can be used instead for deployments where
older and more insecure algorithms are still in use.

### Legacy Provider Example

To use the _legacy_ provider in Vector, first create an OpenSSL configuration file as follows:

```ini
openssl_conf = openssl_init

[openssl_init]
providers = provider_sect

[provider_sect]
default = default_sect
legacy = legacy_sect

[default_sect]
activate = 1

[legacy_sect]
activate = 1
```

Then, run Vector with `OPENSSL_CONF` set to the path where the file above can be found:

```sh
OPENSSL_CONF=/path/to/openssl-legacy.cnf \
    vector --config /path/to/vector.yaml
```

**Note**: If the above configuration file is saved in `/usr/local/ssl/openssl.cnf` Vector automatically
finds it without using `OPENSSL_CONF`. However, this approach is not recommended because other applications
in the running system may also use this file and unintentionally switch to the legacy provider.

### FIPS provider example

To use the _FIPS_ provider in Vector, the [OpenSSL FIPS module][openssl-fips-module] must be installed
and [configured][openssl-fips-module]. This is beyond the scope of this document, however
[instructions][openssl-fips] can be found in the OpenSSL repository.

Not all versions of the OpenSSL FIPS module have been validated. However, it is possible to use previous
validated versions of the FIPS module with newer versions of OpenSSL, such as the version used in Vector.
This use case is also documented in the installation instructions linked above.

Once the FIPS module is installed and configured, a `fips.so` (on Unix) or `fips.dll` (on Windows)
module file, and a `fipsmodule.cnf` configuration file should be available to use in Vector.

An OpenSSL configuration file must be then created as follows:

```ini
config_diagnostics = 1
openssl_conf = openssl_init

.include /path/to/fipsmodule.cnf

[openssl_init]
providers = provider_sect
alg_section = algorithm_sect

[provider_sect]
fips = fips_sect
base = base_sect

[base_sect]
activate = 1

[algorithm_sect]
default_properties = fips=yes
```

Then, run Vector with `OPENSSL_CONF` set to the path where the file above can be found and
`OPENSSL_MODULES` set to the path where the FIPS module files are installed:

```sh
OPENSSL_CONF=/path/to/openssl-fips.cnf \
OPENSSL_MODULES=/path/to/fips-modules \
    vector --config /path/to/vector.yaml
```

**Note**: If the running system already has a system-wide OpenSSL FIPS installation and an OpenSSL
configuration file for it, Vector can also use them directly with the above environment variables.

[cli]: /docs/reference/cli
[fips-140-2]: https://en.wikipedia.org/wiki/FIPS_140-2
[openssl]: https://www.openssl.org/
[openssl-config]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man5/config.html
[openssl-env]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/openssl-env.html
[openssl-fips]: https://github.com/openssl/openssl/blob/master/README-FIPS.md
[openssl-fips-module]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/fips_module.html
[openssl-probe]: https://github.com/alexcrichton/openssl-probe
[openssl-providers]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/provider.html
[openssl-providers-default]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/OSSL_PROVIDER-default.html
[openssl-providers-fips]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/OSSL_PROVIDER-FIPS.html
[openssl-providers-legacy]: https://www.openssl.org/docs/man{{< param openssl_version >}}/man7/OSSL_PROVIDER-legacy.html
