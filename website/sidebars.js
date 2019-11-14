module.exports = {
  docs: [
    {
      type: 'category',
      label: 'About',
      items: [
        "docs/about",
        "docs/about/what-is-vector",
        "docs/about/concepts",
        {
          type: 'category',
          label: 'Data Model',
          items: [
            "docs/about/data-model",
            "docs/about/data-model/log",
            "docs/about/data-model/metric",
          ]
        },
        "docs/about/guarantees",
      ],
    },
    {
      type: 'category',
      label: 'Setup',
      items: [
        {
          type: 'category',
          label: 'Installation',
          items: [
            "docs/setup/installation",
            {
              type: 'category',
              label: 'Containers',
              items: [
                "docs/setup/installation/containers",
                  "docs/setup/installation/containers/docker",
              ],
            },
            {
              type: 'category',
              label: 'Package Managers',
              items: [
                "docs/setup/installation/package-managers",
                  "docs/setup/installation/package-managers/dpkg",
                  "docs/setup/installation/package-managers/homebrew",
                  "docs/setup/installation/package-managers/rpm",
              ],
            },
            {
              type: 'category',
              label: 'Operating Systems',
              items: [
                "docs/setup/installation/operating-systems",
                  "docs/setup/installation/operating-systems/amazon-linux",
                  "docs/setup/installation/operating-systems/centos",
                  "docs/setup/installation/operating-systems/debian",
                  "docs/setup/installation/operating-systems/macos",
                  "docs/setup/installation/operating-systems/rhel",
                  "docs/setup/installation/operating-systems/ubuntu",
              ],
            },
            {
              type: 'category',
              label: 'Manual',
              items: [
                "docs/setup/installation/manual",
                "docs/setup/installation/manual/from-archives",
                "docs/setup/installation/manual/from-source",              
              ],
            },
          ],
        },
        "docs/setup/configuration",
        {
          type: 'category',
          label: 'Deployment',
          items: [
            "docs/setup/deployment",
            {
              type: 'category',
              label: 'Roles',
              items: [
                "docs/setup/deployment/roles",
                "docs/setup/deployment/roles/agent",
                "docs/setup/deployment/roles/service",
              ]
            },
            "docs/setup/deployment/topologies",
          ]
        },
      ],
    },
    {
      type: 'category',
      label: 'Components',
      items: [
        "docs/components",
        {
          type: 'category',
          label: 'Sources',
          items: [
            "docs/components/sources",
            
              "docs/components/sources/docker",
            
              "docs/components/sources/file",
            
              "docs/components/sources/journald",
            
              "docs/components/sources/kafka",
            
              "docs/components/sources/statsd",
            
              "docs/components/sources/stdin",
            
              "docs/components/sources/syslog",
            
              "docs/components/sources/tcp",
            
              "docs/components/sources/udp",
            
              "docs/components/sources/vector",
            
          ]
        },
        {
          type: 'category',
          label: 'Transforms',
          items: [
            "docs/components/transforms",
            
              "docs/components/transforms/add_fields",
            
              "docs/components/transforms/add_tags",
            
              "docs/components/transforms/coercer",
            
              "docs/components/transforms/field_filter",
            
              "docs/components/transforms/grok_parser",
            
              "docs/components/transforms/json_parser",
            
              "docs/components/transforms/log_to_metric",
            
              "docs/components/transforms/lua",
            
              "docs/components/transforms/regex_parser",
            
              "docs/components/transforms/remove_fields",
            
              "docs/components/transforms/remove_tags",
            
              "docs/components/transforms/sampler",
            
              "docs/components/transforms/split",
            
              "docs/components/transforms/tokenizer",
            
          ]
        },
        {
          type: 'category',
          label: 'Sinks',
          items: [
            "docs/components/sinks",
            
              "docs/components/sinks/aws_cloudwatch_logs",
            
              "docs/components/sinks/aws_cloudwatch_metrics",
            
              "docs/components/sinks/aws_kinesis_streams",
            
              "docs/components/sinks/aws_s3",
            
              "docs/components/sinks/blackhole",
            
              "docs/components/sinks/clickhouse",
            
              "docs/components/sinks/console",
            
              "docs/components/sinks/datadog_metrics",
            
              "docs/components/sinks/elasticsearch",
            
              "docs/components/sinks/file",
            
              "docs/components/sinks/http",
            
              "docs/components/sinks/kafka",
            
              "docs/components/sinks/prometheus",
            
              "docs/components/sinks/splunk_hec",
            
              "docs/components/sinks/statsd",
            
              "docs/components/sinks/tcp",
            
              "docs/components/sinks/vector",
            
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Administration',
      items: [
        "docs/administration/process-management",
        "docs/administration/monitoring",
        "docs/administration/tuning",
        "docs/administration/updating",
        "docs/administration/validating",
        "docs/administration/env-vars",
      ],
    },
    {
      type: 'category',
      label: 'Meta',
      items: [
        "docs/meta/glossary",
      ],
    },
  ]
};
