module.exports = {
  docs: [
    {
      type: 'category',
      label: 'About',
      items: [
        "about",
        "about/what-is-vector",
        "about/concepts",
        {
          type: 'category',
          label: 'Data Model',
          items: [
            "about/data-model",
            "about/data-model/log",
            "about/data-model/metric",
          ]
        },
        "about/guarantees",
      ],
    },
    {
      type: 'category',
      label: 'Setup',
      items: [
        "setup",
        {
          type: 'category',
          label: 'Installation',
          items: [
            "setup/installation",
            {
              type: 'category',
              label: 'Package Managers',
              items: [
                "setup/installation/package-managers",
                  "setup/installation/package-managers/dpkg",
                  "setup/installation/package-managers/homebrew",
                  "setup/installation/package-managers/msi",
                  "setup/installation/package-managers/nix",
                  "setup/installation/package-managers/rpm",
              ],
            },
            {
              type: 'category',
              label: 'Platforms',
              items: [
                "setup/installation/platforms",
                  "setup/installation/platforms/docker",
              ],
            },
            {
              type: 'category',
              label: 'Operating Systems',
              items: [
                "setup/installation/operating-systems",
                  "setup/installation/operating-systems/amazon-linux",
                  "setup/installation/operating-systems/centos",
                  "setup/installation/operating-systems/debian",
                  "setup/installation/operating-systems/macos",
                  "setup/installation/operating-systems/nixos",
                  "setup/installation/operating-systems/rhel",
                  "setup/installation/operating-systems/raspbian",
                  "setup/installation/operating-systems/ubuntu",
                  "setup/installation/operating-systems/windows",
              ],
            },
            {
              type: 'category',
              label: 'Manual',
              items: [
                "setup/installation/manual",
                "setup/installation/manual/from-archives",
                "setup/installation/manual/from-source",
              ],
            },
          ],
        },
        "setup/configuration",
        {
          type: 'category',
          label: 'Deployment',
          items: [
            "setup/deployment",
            "setup/deployment/strategies",
            "setup/deployment/topologies",
          ]
        },
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      items: [
        "reference",
        {
          type: 'category',
          label: 'Sources',
          items: [
            "reference/sources",
            
              "reference/sources/docker",
            
              "reference/sources/file",
            
              "reference/sources/http",
            
              "reference/sources/journald",
            
              "reference/sources/kafka",
            
              "reference/sources/logplex",
            
              "reference/sources/prometheus",
            
              "reference/sources/socket",
            
              "reference/sources/splunk_hec",
            
              "reference/sources/statsd",
            
              "reference/sources/stdin",
            
              "reference/sources/syslog",
            
              "reference/sources/vector",
            
          ]
        },
        {
          type: 'category',
          label: 'Transforms',
          items: [
            "reference/transforms",
            
              "reference/transforms/add_fields",
            
              "reference/transforms/add_tags",
            
              "reference/transforms/ansi_stripper",
            
              "reference/transforms/aws_ec2_metadata",
            
              "reference/transforms/coercer",
            
              "reference/transforms/concat",
            
              "reference/transforms/dedupe",
            
              "reference/transforms/filter",
            
              "reference/transforms/geoip",
            
              "reference/transforms/grok_parser",
            
              "reference/transforms/json_parser",
            
              "reference/transforms/log_to_metric",
            
              "reference/transforms/logfmt_parser",
            
              "reference/transforms/lua",
            
              "reference/transforms/merge",
            
              "reference/transforms/regex_parser",
            
              "reference/transforms/remove_fields",
            
              "reference/transforms/remove_tags",
            
              "reference/transforms/rename_fields",
            
              "reference/transforms/sampler",
            
              "reference/transforms/split",
            
              "reference/transforms/swimlanes",
            
              "reference/transforms/tag_cardinality_limit",
            
              "reference/transforms/tokenizer",
            
          ]
        },
        {
          type: 'category',
          label: 'Sinks',
          items: [
            "reference/sinks",
            
              "reference/sinks/aws_cloudwatch_logs",
            
              "reference/sinks/aws_cloudwatch_metrics",
            
              "reference/sinks/aws_kinesis_firehose",
            
              "reference/sinks/aws_kinesis_streams",
            
              "reference/sinks/aws_s3",
            
              "reference/sinks/blackhole",
            
              "reference/sinks/clickhouse",
            
              "reference/sinks/console",
            
              "reference/sinks/datadog_logs",
            
              "reference/sinks/datadog_metrics",
            
              "reference/sinks/elasticsearch",
            
              "reference/sinks/file",
            
              "reference/sinks/gcp_cloud_storage",
            
              "reference/sinks/gcp_pubsub",
            
              "reference/sinks/gcp_stackdriver_logs",
            
              "reference/sinks/honeycomb",
            
              "reference/sinks/http",
            
              "reference/sinks/humio_logs",
            
              "reference/sinks/influxdb_metrics",
            
              "reference/sinks/kafka",
            
              "reference/sinks/logdna",
            
              "reference/sinks/loki",
            
              "reference/sinks/new_relic_logs",
            
              "reference/sinks/papertrail",
            
              "reference/sinks/prometheus",
            
              "reference/sinks/pulsar",
            
              "reference/sinks/sematext_logs",
            
              "reference/sinks/socket",
            
              "reference/sinks/splunk_hec",
            
              "reference/sinks/statsd",
            
              "reference/sinks/vector",
            
          ],
        },
        {
          type: 'category',
          label: 'Advanced',
          items: [
            "reference/env-vars",
            "reference/field-path-notation",
            "reference/global-options",
            "reference/templating",
            "reference/tests",
          ]
        },
      ],
    },
    {
      type: 'category',
      label: 'Administration',
      items: [
        "administration",
        "administration/process-management",
        "administration/monitoring",
        "administration/tuning",
        "administration/updating",
        "administration/validating",
      ],
    },
    {
      type: 'category',
      label: 'Meta',
      items: [
        "meta/glossary",
        {
          type: 'link',
          label: 'Privacy',
          href: 'https://github.com/timberio/vector/blob/master/PRIVACY.md'
        },
        {
          type: 'link',
          label: 'Security',
          href: 'https://github.com/timberio/vector/security/policy'
        },
      ],
    },
  ]
};
