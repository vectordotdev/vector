# v0.4 Changelog

#### [Releases][urls.vector_releases]

* [**Unreleased**](#unreleased) - [download][urls.vector_nightly_builds], [compare][urls.compare_v0.4.0...master]
* [**0.4.0**](#040---sep-24-2019) - [download][urls.v0.4.0], [compare][urls.compare_v0.3.0...v0.4.0]

#### [Installation][docs.installation]

* [**Platforms**][docs.platforms] - [Docker][docs.docker]
* [**Operating systems**][docs.operating_systems] - [Amazon Linux][docs.amazon-linux], [CentOS][docs.centos], [Debian][docs.debian], [MacOS][docs.macos], [RHEL][docs.rhel], [Ubuntu][docs.ubuntu]
* [**Package managers**][docs.package_managers] - [APT][docs.apt], [Homebrew][docs.homebrew], [YUM][docs.yum]
* [**Manual**][docs.manual] - [From archives][docs.from_archives], [From source][docs.from_source]

## Unreleased

Vector follows the [conventional commits specification][urls.conventional_commits] and unrelease changes are rolled up into a changelog when they are released:

* [Compare `v0.4.0` to `master`][urls.compare_v0.4.0...master]
* [Download nightly builds][urls.vector_nightly_builds]

## [0.4.0][urls.v0.4.0] - Sep 24, 2019

### New features

* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Support aws authentication ([#864][urls.pr_864])
* *new*: `aws_cloudwatch_metrics` sink ([#707][urls.pr_707])
* *new*: [`clickhouse` sink][docs.sinks.clickhouse] ([#693][urls.pr_693])
* *new*: [`file` sink][docs.sinks.file] ([#688][urls.pr_688])
* *new*: [`udp` source][docs.sources.udp] ([#738][urls.pr_738])
* *new*: [`kafka` source][docs.sources.kafka] ([#774][urls.pr_774])
* *new*: [`journald` source][docs.sources.journald] ([#702][urls.pr_702])
* *new*: [`coercer` transform][docs.transforms.coercer] ([#666][urls.pr_666])
* *new*: [`add_tags` transform][docs.transforms.add_tags] ([#785][urls.pr_785])
* *new*: [`remove_tags` transform][docs.transforms.remove_tags] ([#785][urls.pr_785])
* *new*: [`split` transform][docs.transforms.split] ([#850][urls.pr_850])
* *[`syslog` source][docs.sources.syslog]*: Add all parsed syslog fields to event ([#836][urls.pr_836])

### Enhancements

* *[`aws_cloudwatch_logs` sink][docs.sinks.aws_cloudwatch_logs]*: Add cloudwatch partitioning and refactor partition buffer ([#519][urls.pr_519])
* *[`aws_cloudwatch_logs` sink][docs.sinks.aws_cloudwatch_logs]*: Add retry ability to cloudwatch ([#663][urls.pr_663])
* *[`aws_cloudwatch_logs` sink][docs.sinks.aws_cloudwatch_logs]*: Add dynamic group creation ([#759][urls.pr_759])
* *[`aws_kinesis_streams` sink][docs.sinks.aws_kinesis_streams]*: Add configurable partition keys ([#692][urls.pr_692])
* *[`aws_s3` sink][docs.sinks.aws_s3]*: Add filename extension option and fix trailing slash ([#596][urls.pr_596])
* *cli*: Add `--color` option and tty check for ansi colors ([#623][urls.pr_623])
* *[config][docs.configuration]*: Improve configuration validation and make it more strict ([#552][urls.pr_552])
* *[config][docs.configuration]*: Reusable templating system for event values ([#656][urls.pr_656])
* *[config][docs.configuration]*: Validation of sinks and sources for non-emptiness. ([#739][urls.pr_739])
* *[config][docs.configuration]*: Default config path "/etc/vector/vector.toml" ([#900][urls.pr_900])
* *[`console` sink][docs.sinks.console]*: Accept both logs and metrics ([#631][urls.pr_631])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Default `doc_type` to `_doc` and make it op… ([#695][urls.pr_695])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Use templates for es index and s3 key prefix ([#686][urls.pr_686])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Add http basic authorization ([#749][urls.pr_749])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Add support for additional headers to the elasticsearch sink ([#758][urls.pr_758])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Add support for custom query parameters ([#766][urls.pr_766])
* *[`file` source][docs.sources.file]*: Add file checkpoint feature. ([#609][urls.pr_609])
* *[`file` source][docs.sources.file]*: Fall back to global data_dir option (#644) ([#673][urls.pr_673])
* *[`file` source][docs.sources.file]*: Make fingerprinting strategy configurable ([#780][urls.pr_780])
* *[`file` source][docs.sources.file]*: Allow aggregating multiple lines into one event ([#809][urls.pr_809])
* *[`file` source][docs.sources.file]*: Favor older files and allow configuring greedier reads ([#810][urls.pr_810])
* *[`file` source][docs.sources.file]*: Log a single warning when ignoring small files ([#863][urls.pr_863])
* *[`grok_parser` transform][docs.transforms.grok_parser]*: Add type coercion ([#632][urls.pr_632])
* *[`http` sink][docs.sinks.http]*: Add support for unverified https ([#815][urls.pr_815])
* *[`journald` source][docs.sources.journald]*: Add checkpointing support ([#816][urls.pr_816])
* *[`log_to_metric` transform][docs.transforms.log_to_metric]*: Output multiple metrics from a single log ([d8eadb0][urls.commit_d8eadb08f469e7e411138ed9ff9e318bd4f9954c])
* *[`log_to_metric` transform][docs.transforms.log_to_metric]*: Push histogram and set metrics from logs ([#650][urls.pr_650])
* *[`log_to_metric` transform][docs.transforms.log_to_metric]*: Use templates for metric names in log_to_metric ([#668][urls.pr_668])
* *[`lua` transform][docs.transforms.lua]*: Add tags support to log_to_metric transform ([#786][urls.pr_786])
* *[metric data model][docs.data-model.metric]*: Use floats for metrics values ([#553][urls.pr_553])
* *[metric data model][docs.data-model.metric]*: Add timestamps into metrics ([#726][urls.pr_726])
* *[metric data model][docs.data-model.metric]*: Add tags into metrics model ([#754][urls.pr_754])
* *[observability][docs.monitoring]*: Initial rate limit subscriber ([#494][urls.pr_494])
* *[observability][docs.monitoring]*: Add rate limit notice when it starts ([#696][urls.pr_696])
* *operations*: Add `jemallocator` feature flag ([#653][urls.pr_653])
* *operations*: Build for x86_64-unknown-linux-musl with all features and optimized binary size ([#689][urls.pr_689])
* *[`prometheus` sink][docs.sinks.prometheus]*: Support histograms ([#675][urls.pr_675])
* *[`prometheus` sink][docs.sinks.prometheus]*: Support sets ([#733][urls.pr_733])
* *[`prometheus` sink][docs.sinks.prometheus]*: Add labels support ([#773][urls.pr_773])
* *[`prometheus` sink][docs.sinks.prometheus]*: Add namespace config ([#782][urls.pr_782])
* *[`regex_parser` transform][docs.transforms.regex_parser]*: Log when regex does not match ([#618][urls.pr_618])
* *[`tcp` sink][docs.sinks.tcp]*: Add support for tls ([#765][urls.pr_765])
* *[`tokenizer` transform][docs.transforms.tokenizer]*: Convert "-" into "nil" ([#580][urls.pr_580])
* *topology*: Adjust transform trait for multiple output events ([fe7f2b5][urls.commit_fe7f2b503443199a65a79dad129ed89ace3e287a])
* *topology*: Add sink healthcheck disable ([#731][urls.pr_731])

### Performance improvements

* *[observability][docs.monitoring]*: Add initial rework of rate limited logs ([#778][urls.pr_778])

### Bug fixes

* *[`add_fields` transform][docs.transforms.add_fields]*: Rename config tag ([#902][urls.pr_902])
* *[`aws_cloudwatch_logs` sink][docs.sinks.aws_cloudwatch_logs]*: `encoding = "text"` overrides ([#803][urls.pr_803])
* *[`aws_s3` sink][docs.sinks.aws_s3]*: Retry httpdispatch errors for s3 and kinesis ([#651][urls.pr_651])
* *[config][docs.configuration]*: Reload with unparseable config ([#752][urls.pr_752])
* *[`elasticsearch` sink][docs.sinks.elasticsearch]*: Make the headers and query tables optional. ([#831][urls.pr_831])
* *[log data model][docs.data-model.log]*: Unflatten event before outputting ([#678][urls.pr_678])
* *[log data model][docs.data-model.log]*: Don't serialize mapvalue::null as a string ([#725][urls.pr_725])
* *networking*: Retry requests on timeouts ([#691][urls.pr_691])
* *operations*: Use gnu ld instead of llvm lld for x86_64-unknown-linux-musl ([#794][urls.pr_794])
* *operations*: Fix docker nightly builds ([#830][urls.pr_830])
* *operations*: Use openssl instead of libressl for x86_64-unknown-linux-musl ([#904][urls.pr_904])
* *[`prometheus` sink][docs.sinks.prometheus]*: Update metric::set usage ([#756][urls.pr_756])
* *security*: Rustsec-2019-0011 by updating crossbeam-epoch ([#723][urls.pr_723])
* *topology*: It is now possible to reload a with a non-overlap… ([#681][urls.pr_681])


[docs.amazon-linux]: https://docs.vector.dev/setup/installation/operating-systems/amazon-linux
[docs.apt]: https://docs.vector.dev/setup/installation/package-managers/apt
[docs.centos]: https://docs.vector.dev/setup/installation/operating-systems/centos
[docs.configuration]: https://docs.vector.dev/usage/configuration
[docs.data-model.log]: https://docs.vector.dev/about/data-model/log
[docs.data-model.metric]: https://docs.vector.dev/about/data-model/metric
[docs.debian]: https://docs.vector.dev/setup/installation/operating-systems/debian
[docs.docker]: https://docs.vector.dev/setup/installation/platforms/docker
[docs.from_archives]: https://docs.vector.dev/setup/installation/manual/from-archives
[docs.from_source]: https://docs.vector.dev/setup/installation/manual/from-source
[docs.homebrew]: https://docs.vector.dev/setup/installation/package-managers/homebrew
[docs.installation]: https://docs.vector.dev/setup/installation
[docs.macos]: https://docs.vector.dev/setup/installation/operating-systems/macos
[docs.manual]: https://docs.vector.dev/setup/installation/manual
[docs.monitoring]: https://docs.vector.dev/usage/administration/monitoring
[docs.operating_systems]: https://docs.vector.dev/setup/installation/operating-systems
[docs.package_managers]: https://docs.vector.dev/setup/installation/package-managers
[docs.platforms]: https://docs.vector.dev/setup/installation/platforms
[docs.rhel]: https://docs.vector.dev/setup/installation/operating-systems/rhel
[docs.sinks.aws_cloudwatch_logs]: https://docs.vector.dev/usage/configuration/sinks/aws_cloudwatch_logs
[docs.sinks.aws_kinesis_streams]: https://docs.vector.dev/usage/configuration/sinks/aws_kinesis_streams
[docs.sinks.aws_s3]: https://docs.vector.dev/usage/configuration/sinks/aws_s3
[docs.sinks.clickhouse]: https://docs.vector.dev/usage/configuration/sinks/clickhouse
[docs.sinks.console]: https://docs.vector.dev/usage/configuration/sinks/console
[docs.sinks.elasticsearch]: https://docs.vector.dev/usage/configuration/sinks/elasticsearch
[docs.sinks.file]: https://docs.vector.dev/usage/configuration/sinks/file
[docs.sinks.http]: https://docs.vector.dev/usage/configuration/sinks/http
[docs.sinks.prometheus]: https://docs.vector.dev/usage/configuration/sinks/prometheus
[docs.sinks.tcp]: https://docs.vector.dev/usage/configuration/sinks/tcp
[docs.sources.file]: https://docs.vector.dev/usage/configuration/sources/file
[docs.sources.journald]: https://docs.vector.dev/usage/configuration/sources/journald
[docs.sources.kafka]: https://docs.vector.dev/usage/configuration/sources/kafka
[docs.sources.syslog]: https://docs.vector.dev/usage/configuration/sources/syslog
[docs.sources.udp]: https://docs.vector.dev/usage/configuration/sources/udp
[docs.transforms.add_fields]: https://docs.vector.dev/usage/configuration/transforms/add_fields
[docs.transforms.add_tags]: https://docs.vector.dev/usage/configuration/transforms/add_tags
[docs.transforms.coercer]: https://docs.vector.dev/usage/configuration/transforms/coercer
[docs.transforms.grok_parser]: https://docs.vector.dev/usage/configuration/transforms/grok_parser
[docs.transforms.log_to_metric]: https://docs.vector.dev/usage/configuration/transforms/log_to_metric
[docs.transforms.lua]: https://docs.vector.dev/usage/configuration/transforms/lua
[docs.transforms.regex_parser]: https://docs.vector.dev/usage/configuration/transforms/regex_parser
[docs.transforms.remove_tags]: https://docs.vector.dev/usage/configuration/transforms/remove_tags
[docs.transforms.split]: https://docs.vector.dev/usage/configuration/transforms/split
[docs.transforms.tokenizer]: https://docs.vector.dev/usage/configuration/transforms/tokenizer
[docs.ubuntu]: https://docs.vector.dev/setup/installation/operating-systems/ubuntu
[docs.yum]: https://docs.vector.dev/setup/installation/package-managers/yum
[urls.commit_d8eadb08f469e7e411138ed9ff9e318bd4f9954c]: https://github.com/timberio/vector/commit/d8eadb08f469e7e411138ed9ff9e318bd4f9954c
[urls.commit_fe7f2b503443199a65a79dad129ed89ace3e287a]: https://github.com/timberio/vector/commit/fe7f2b503443199a65a79dad129ed89ace3e287a
[urls.compare_v0.3.0...v0.4.0]: https://github.com/timberio/vector/compare/v0.3.0...v0.4.0
[urls.compare_v0.4.0...master]: https://github.com/timberio/vector/compare/v0.4.0...master
[urls.conventional_commits]: https://www.conventionalcommits.org
[urls.pr_494]: https://github.com/timberio/vector/pull/494
[urls.pr_519]: https://github.com/timberio/vector/pull/519
[urls.pr_552]: https://github.com/timberio/vector/pull/552
[urls.pr_553]: https://github.com/timberio/vector/pull/553
[urls.pr_580]: https://github.com/timberio/vector/pull/580
[urls.pr_596]: https://github.com/timberio/vector/pull/596
[urls.pr_609]: https://github.com/timberio/vector/pull/609
[urls.pr_618]: https://github.com/timberio/vector/pull/618
[urls.pr_623]: https://github.com/timberio/vector/pull/623
[urls.pr_631]: https://github.com/timberio/vector/pull/631
[urls.pr_632]: https://github.com/timberio/vector/pull/632
[urls.pr_650]: https://github.com/timberio/vector/pull/650
[urls.pr_651]: https://github.com/timberio/vector/pull/651
[urls.pr_653]: https://github.com/timberio/vector/pull/653
[urls.pr_656]: https://github.com/timberio/vector/pull/656
[urls.pr_663]: https://github.com/timberio/vector/pull/663
[urls.pr_666]: https://github.com/timberio/vector/pull/666
[urls.pr_668]: https://github.com/timberio/vector/pull/668
[urls.pr_673]: https://github.com/timberio/vector/pull/673
[urls.pr_675]: https://github.com/timberio/vector/pull/675
[urls.pr_678]: https://github.com/timberio/vector/pull/678
[urls.pr_681]: https://github.com/timberio/vector/pull/681
[urls.pr_686]: https://github.com/timberio/vector/pull/686
[urls.pr_688]: https://github.com/timberio/vector/pull/688
[urls.pr_689]: https://github.com/timberio/vector/pull/689
[urls.pr_691]: https://github.com/timberio/vector/pull/691
[urls.pr_692]: https://github.com/timberio/vector/pull/692
[urls.pr_693]: https://github.com/timberio/vector/pull/693
[urls.pr_695]: https://github.com/timberio/vector/pull/695
[urls.pr_696]: https://github.com/timberio/vector/pull/696
[urls.pr_702]: https://github.com/timberio/vector/pull/702
[urls.pr_707]: https://github.com/timberio/vector/pull/707
[urls.pr_723]: https://github.com/timberio/vector/pull/723
[urls.pr_725]: https://github.com/timberio/vector/pull/725
[urls.pr_726]: https://github.com/timberio/vector/pull/726
[urls.pr_731]: https://github.com/timberio/vector/pull/731
[urls.pr_733]: https://github.com/timberio/vector/pull/733
[urls.pr_738]: https://github.com/timberio/vector/pull/738
[urls.pr_739]: https://github.com/timberio/vector/pull/739
[urls.pr_749]: https://github.com/timberio/vector/pull/749
[urls.pr_752]: https://github.com/timberio/vector/pull/752
[urls.pr_754]: https://github.com/timberio/vector/pull/754
[urls.pr_756]: https://github.com/timberio/vector/pull/756
[urls.pr_758]: https://github.com/timberio/vector/pull/758
[urls.pr_759]: https://github.com/timberio/vector/pull/759
[urls.pr_765]: https://github.com/timberio/vector/pull/765
[urls.pr_766]: https://github.com/timberio/vector/pull/766
[urls.pr_773]: https://github.com/timberio/vector/pull/773
[urls.pr_774]: https://github.com/timberio/vector/pull/774
[urls.pr_778]: https://github.com/timberio/vector/pull/778
[urls.pr_780]: https://github.com/timberio/vector/pull/780
[urls.pr_782]: https://github.com/timberio/vector/pull/782
[urls.pr_785]: https://github.com/timberio/vector/pull/785
[urls.pr_786]: https://github.com/timberio/vector/pull/786
[urls.pr_794]: https://github.com/timberio/vector/pull/794
[urls.pr_803]: https://github.com/timberio/vector/pull/803
[urls.pr_809]: https://github.com/timberio/vector/pull/809
[urls.pr_810]: https://github.com/timberio/vector/pull/810
[urls.pr_815]: https://github.com/timberio/vector/pull/815
[urls.pr_816]: https://github.com/timberio/vector/pull/816
[urls.pr_830]: https://github.com/timberio/vector/pull/830
[urls.pr_831]: https://github.com/timberio/vector/pull/831
[urls.pr_836]: https://github.com/timberio/vector/pull/836
[urls.pr_850]: https://github.com/timberio/vector/pull/850
[urls.pr_863]: https://github.com/timberio/vector/pull/863
[urls.pr_864]: https://github.com/timberio/vector/pull/864
[urls.pr_900]: https://github.com/timberio/vector/pull/900
[urls.pr_902]: https://github.com/timberio/vector/pull/902
[urls.pr_904]: https://github.com/timberio/vector/pull/904
[urls.v0.4.0]: https://github.com/timberio/vector/releases/tag/v0.4.0
[urls.vector_nightly_builds]: http://google.com
[urls.vector_releases]: https://github.com/timberio/vector/releases
