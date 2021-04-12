The following feature flags are supported via the `FEATURES` env var when executing `make build`:

```shell
[FEATURES="<flag1>,<flag2>,..."] make build
```

There are three meta-features that can be used when compiling for the corresponding targets. If no features are specified, the `default` is used.


Feature | Description | Enabled by default?
:-------|:------------|:-------------------
`default` | Default set of features for `*-unknown-linux-gnu` and `*-apple-darwin` targets. | ✅
`default-cmake` | Default set of features for `*-unknown-linux-*` targets which uses `cmake` and `perl` as build dependencies.
`default-msvc` | Default set of features for `*-pc-windows-msvc` targets. Requires `cmake` and `perl` as build dependencies.

Alternatively, for finer control over dependencies and operating system features, it is possible to use specific features from the list below:

Feature | Description | Included in `default` feature?
:-------|:------------|:------------------------------
`unix` | Enables features that require `cfg(unix)` to be present on the platform, namely support for Unix domain sockets in the [`docker_logs` source][docker_logs] and [jemalloc] instead of the default memory allocator. | ✅
`vendored` | Forces vendoring of [OpenSSL] and [ZLib] dependencies instead of using their versions installed in the system. Requires `perl` as a build dependency. | ✅
`leveldb-plain` | Enables support for [disk buffers][buffer] using vendored [LevelDB]. | ✅
`leveldb-cmake` | The same as `leveldb-plain`, but more portable. Requires `cmake` as a build dependency. Use this in case of compilation issues with `leveldb-plain`. |
`rdkafka-plain` | Enables vendored [`librdkafka`] dependency, which is required for the [`kafka` source][kafka_source] and [`kafka` sink][kafka_sink]. | ✅
`rdkafka-cmake` | The same as `rdkafka-plain` but more portable. Requires `cmake` as a build dependency. Use this in case of compilation issues with `rdkafka-plain`. |

In addition, it is possible to pick only a subset of Vector's components for the build using feature flags. In order to do it, it instead of default features one has to pass a comma-separated list of component features.

[buffer]: /docs/meta/glossary/#buffer
[docker_logs]: /docs/reference/configuration/sources/docker_logs
[jemalloc]: https://github.com/jemalloc/jemalloc
[kafka_sink]: /docs/reference/configuration/sinks/kafka
[kafka_source]: /docs/reference/configuration/sources/kafka
[leveldb]: https://github.com/google/leveldb
[librdkafka]: https://github.com/edenhill/librdkafka
[openssl]: https://www.openssl.org
[zlib]: https://www.zlib.net
