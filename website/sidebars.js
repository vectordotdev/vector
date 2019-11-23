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
                  "setup/installation/operating-systems/raspberry-pi",
                  "setup/installation/operating-systems/rhel",
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
      label: 'Reference',
      items: [
        "reference",
        "reference/global-options",
        {
          type: 'category',
          label: 'Sources',
          items: [
            "reference/sources",
            
              "reference/sources/docker",
            
              "reference/sources/file",
            
              "reference/sources/journald",
            
              "reference/sources/kafka",
            
              "reference/sources/statsd",
            
              "reference/sources/stdin",
            
              "reference/sources/syslog",
            
              "reference/sources/tcp",
            
              "reference/sources/udp",
            
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
            
              "reference/transforms/coercer",
            
              "reference/transforms/field_filter",
            
              "reference/transforms/grok_parser",
            
              "reference/transforms/json_parser",
            
              "reference/transforms/log_to_metric",
            
              "reference/transforms/lua",
            
              "reference/transforms/regex_parser",
            
              "reference/transforms/remove_fields",
            
              "reference/transforms/remove_tags",
            
              "reference/transforms/sampler",
            
              "reference/transforms/split",
            
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
            
              "reference/sinks/aws_kinesis_streams",
            
              "reference/sinks/aws_s3",
            
              "reference/sinks/blackhole",
            
              "reference/sinks/clickhouse",
            
              "reference/sinks/console",
            
              "reference/sinks/datadog_metrics",
            
              "reference/sinks/elasticsearch",
            
              "reference/sinks/file",
            
              "reference/sinks/http",
            
              "reference/sinks/kafka",
            
              "reference/sinks/prometheus",
            
              "reference/sinks/splunk_hec",
            
              "reference/sinks/statsd",
            
              "reference/sinks/tcp",
            
              "reference/sinks/vector",
            
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
