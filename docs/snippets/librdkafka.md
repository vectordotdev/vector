The `kafka` source and sink for Vector both use [`librdkafka`][librdkafka] under the hood. This is a battle-tested, high-performance, and reliable library that facilitates communication with Kafka. As Vector produces static MUSL builds, this dependency is packaged with Vector, which means that you don't need to install it.

[librdkafka]: https://github.com/edenhill/librdkafka
