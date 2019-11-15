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
        {
          type: 'category',
          label: 'Installation',
          items: [
            "setup/installation",
            {
              type: 'category',
              label: 'Containers',
              items: [
                "setup/installation/containers",
                  "setup/installation/containers/docker",
              ],
            },
            {
              type: 'category',
              label: 'Package Managers',
              items: [
                "setup/installation/package-managers",
                  "setup/installation/package-managers/dpkg",
                  "setup/installation/package-managers/homebrew",
                  "setup/installation/package-managers/rpm",
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
                  "setup/installation/operating-systems/rhel",
                  "setup/installation/operating-systems/ubuntu",
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
            {
              type: 'category',
              label: 'Roles',
              items: [
                "setup/deployment/roles",
                "setup/deployment/roles/agent",
                "setup/deployment/roles/service",
              ]
            },
            "setup/deployment/topologies",
          ]
        },
        {
          type: 'category',
          label: 'Guides',
          items: [
            "setup/guides",
            "setup/guides/getting-started",
            "setup/guides/troubleshooting",
          ]
        },
      ],
    },
    {
      type: 'category',
      label: 'Components',
      items: [
        "components",
        {
          type: 'category',
          label: 'Sources',
          items: [
            "components/sources",
            
              "components/sources/docker",
            
              "components/sources/file",
            
              "components/sources/journald",
            
              "components/sources/kafka",
            
              "components/sources/statsd",
            
              "components/sources/stdin",
            
              "components/sources/syslog",
            
              "components/sources/tcp",
            
              "components/sources/udp",
            
              "components/sources/vector",
            
          ]
        },
        {
          type: 'category',
          label: 'Transforms',
          items: [
            "components/transforms",
            
              "components/transforms/add_fields",
            
              "components/transforms/add_tags",
            
              "components/transforms/coercer",
            
              "components/transforms/field_filter",
            
              "components/transforms/grok_parser",
            
              "components/transforms/json_parser",
            
              "components/transforms/log_to_metric",
            
              "components/transforms/lua",
            
              "components/transforms/regex_parser",
            
              "components/transforms/remove_fields",
            
              "components/transforms/remove_tags",
            
              "components/transforms/sampler",
            
              "components/transforms/split",
            
              "components/transforms/tokenizer",
            
          ]
        },
        {
          type: 'category',
          label: 'Sinks',
          items: [
            "components/sinks",
            
              "components/sinks/aws_cloudwatch_logs",
            
              "components/sinks/aws_cloudwatch_metrics",
            
              "components/sinks/aws_kinesis_streams",
            
              "components/sinks/aws_s3",
            
              "components/sinks/blackhole",
            
              "components/sinks/clickhouse",
            
              "components/sinks/console",
            
              "components/sinks/datadog_metrics",
            
              "components/sinks/elasticsearch",
            
              "components/sinks/file",
            
              "components/sinks/http",
            
              "components/sinks/kafka",
            
              "components/sinks/prometheus",
            
              "components/sinks/splunk_hec",
            
              "components/sinks/statsd",
            
              "components/sinks/tcp",
            
              "components/sinks/vector",
            
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Administration',
      items: [
        "administration/process-management",
        "administration/monitoring",
        "administration/tuning",
        "administration/updating",
        "administration/validating",
        "administration/env-vars",
      ],
    },
    {
      type: 'category',
      label: 'Meta',
      items: [
        "meta/glossary",
      ],
    },
  ]
};
