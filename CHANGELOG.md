# v0.5 Changelog

<p align="center">
  <strong>
    Get notified of new releases and project updates! <a href="https://vector.dev/mailing_list/">Join our mailing list<a/>.
  </strong>
</p>

---

* [**Unreleased**](#unreleased) - [download][urls.vector_nightly_builds], [install][docs.installation.manual], [compare][urls.compare_v0.5.0...master]
* [**0.5.0**](#050---oct-8-2019) - [download][urls.v0.5.0], [install][docs.installation], [compare][urls.compare_v0.4.0...v0.5.0]

---

## Unreleased

Vector follows the [conventional commits specification][urls.conventional_commits] and unrelease changes are rolled up into a changelog when they are released:

* [Compare `v0.5.0` to `master`][urls.compare_v0.5.0...master]
* [Download nightly builds][urls.vector_nightly_builds]

## [0.5.0][urls.v0.5.0] - Oct 8, 2019

### New features

* *[`clickhouse` sink][docs.sinks.clickhouse]*: Add support for basic auth ([#937][urls.pr_937])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Add support for tls options ([#953][urls.pr_953])
* *[`kafka` sink][docs.sinks.kafka]*: Add support for tls (ssl) ([#912][urls.pr_912])
* *[`kafka` sink][docs.sinks.kafka]*: Use pkcs#12 keys instead of jks ([#934][urls.pr_934])
* *new sink*: Initial `statsd` implementation ([#821][urls.pr_821])
* *new source*: Initial [`docker` source][docs.sources.docker] implementation ([#787][urls.pr_787])
* *[observability][docs.monitoring]*: Add rate limited debug messages ([#971][urls.pr_971])

### Enhancements

* *[observability][docs.monitoring]*: Show information about why a retry needs to happen ([#835][urls.pr_835])
* *security*: Unify the different tls options ([#972][urls.pr_972])

### Bug fixes

* *[config][docs.configuration]*: Default data_dir to /var/lib/vector ([#995][urls.pr_995])


[docs.configuration]: https://docs.vector.dev/usage/configuration
[docs.installation.manual]: https://docs.vector.dev/setup/installation/manual
[docs.installation]: https://docs.vector.dev/setup/installation
[docs.monitoring]: https://docs.vector.dev/usage/administration/monitoring
[docs.sinks.clickhouse]: https://docs.vector.dev/usage/configuration/sinks/clickhouse
[docs.sinks.elasticsearch]: https://docs.vector.dev/usage/configuration/sinks/elasticsearch
[docs.sinks.kafka]: https://docs.vector.dev/usage/configuration/sinks/kafka
[docs.sources.docker]: https://docs.vector.dev/usage/configuration/sources/docker
[urls.compare_v0.4.0...v0.5.0]: https://github.com/timberio/vector/compare/v0.4.0...v0.5.0
[urls.compare_v0.5.0...master]: https://github.com/timberio/vector/compare/v0.5.0...master
[urls.conventional_commits]: https://www.conventionalcommits.org
[urls.pr_787]: https://github.com/timberio/vector/pull/787
[urls.pr_821]: https://github.com/timberio/vector/pull/821
[urls.pr_835]: https://github.com/timberio/vector/pull/835
[urls.pr_912]: https://github.com/timberio/vector/pull/912
[urls.pr_934]: https://github.com/timberio/vector/pull/934
[urls.pr_937]: https://github.com/timberio/vector/pull/937
[urls.pr_953]: https://github.com/timberio/vector/pull/953
[urls.pr_971]: https://github.com/timberio/vector/pull/971
[urls.pr_972]: https://github.com/timberio/vector/pull/972
[urls.pr_995]: https://github.com/timberio/vector/pull/995
[urls.v0.5.0]: https://github.com/timberio/vector/releases/tag/v0.5.0
[urls.vector_nightly_builds]: http://packages.timber.io/vector/nightly/latest/
