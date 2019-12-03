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
                  "setup/installation/operating-systems/raspbian",
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
            "setup/guides/unit-testing",
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
            
          ]
        },
        {
          type: 'category',
          label: 'Transforms',
          items: [
            "reference/transforms",
            
          ]
        },
        {
          type: 'category',
          label: 'Sinks',
          items: [
            "reference/sinks",
            
          ],
        },
        {
          type: 'category',
          label: 'Advanced',
          items: [
            "reference/env-vars",
            "reference/global-options",
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
      ],
    },
  ]
};
