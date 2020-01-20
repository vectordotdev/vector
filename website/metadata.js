module.exports = {
  "installation": {
    "containers": [
      {
        "archs": [
          "x86_64",
          "ARM64",
          "ARMv7"
        ],
        "id": "docker",
        "name": "Docker",
        "oss": [
          "Linux",
          "MacOS"
        ]
      }
    ],
    "downloads": [
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-unknown-linux-musl.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (x86_64)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "ARM64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-aarch64-unknown-linux-musl.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (ARM64)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "ARMv7",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-armv7-unknown-linux-musleabihf.tar.gz",
        "file_type": "tar.gz",
        "name": "Linux (ARMv7)",
        "os": "Linux",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-apple-darwin.tar.gz",
        "file_type": "tar.gz",
        "name": "MacOS (x86_64)",
        "os": "MacOS",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64-pc-windows-msvc.zip",
        "file_type": "zip",
        "name": "Windows (x86_64, 7+)",
        "os": "Windows",
        "type": "archive"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x64.msi",
        "file_type": "msi",
        "name": "Windows (x86_64, 7+)",
        "os": "Windows",
        "package_manager": "MSI",
        "type": "package"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-amd64.deb",
        "file_type": "deb",
        "name": "Deb (x86_64)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "ARM64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-arm64.deb",
        "file_type": "deb",
        "name": "Deb (ARM64)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "ARMv7",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-armhf.deb",
        "file_type": "deb",
        "name": "Deb (ARMv7)",
        "os": "Linux",
        "package_manager": "DPKG",
        "type": "package"
      },
      {
        "arch": "x86_64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-x86_64.rpm",
        "file_type": "rpm",
        "name": "RPM (x86_64)",
        "os": "Linux",
        "package_manager": "RPM",
        "type": "package"
      },
      {
        "arch": "ARM64",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-aarch64.rpm",
        "file_type": "rpm",
        "name": "RPM (ARM64)",
        "os": "Linux",
        "package_manager": "RPM",
        "type": "package"
      },
      {
        "arch": "ARMv7",
        "available_on_latest": true,
        "available_on_nightly": true,
        "file_name": "vector-armv7hl.rpm",
        "file_type": "rpm",
        "name": "RPM (ARMv7)",
        "os": "Linux",
        "package_manager": "RPM",
        "type": "package"
      }
    ],
    "operating_systems": [
      {
        "id": "amazon-linux",
        "name": "Amazon Linux",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "centos",
        "name": "CentOS",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "debian",
        "name": "Debian",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "macos",
        "name": "MacOS",
        "os": "Linux",
        "package_manager": "Homebrew"
      },
      {
        "id": "raspbian",
        "name": "Raspbian",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "rhel",
        "name": "RHEL",
        "os": "Linux",
        "package_manager": "RPM"
      },
      {
        "id": "ubuntu",
        "name": "Ubuntu",
        "os": "Linux",
        "package_manager": "DPKG"
      },
      {
        "id": "windows",
        "name": "Windows",
        "os": "Windows"
      }
    ],
    "package_managers": [
      {
        "archs": [
          "x86_64",
          "ARM64",
          "ARMv7"
        ],
        "id": "dpkg",
        "name": "DPKG"
      },
      {
        "archs": [
          "x86_64"
        ],
        "id": "homebrew",
        "name": "Homebrew"
      },
      {
        "archs": [
          "x86_64"
        ],
        "id": "rpm",
        "name": "RPM"
      },
      {
        "archs": [
          "x86_64"
        ],
        "id": "msi",
        "name": "MSI"
      }
    ]
  },
  "latest_post": {
    "author_id": "ashley",
    "date": "2019-11-25",
    "description": "Today we're excited to announce beta support for unit testing Vector\nconfigurations, allowing you to define tests directly within your Vector\nconfiguration file. These tests are used to assert the output from topologies of\ntransform components given certain input events, ensuring\nthat your configuration behavior does not regress; a very powerful feature for\nmission-critical production pipelines that are collaborated on.",
    "id": "unit-testing-vector-config-files",
    "path": "website/blog/2019-11-25-unit-testing-vector-config-files.md",
    "permalink": "https://vector.dev/blog/unit-testing-vector-config-files",
    "tags": [
      "type: announcement",
      "domain: config"
    ],
    "title": "Unit Testing Your Vector Config Files"
  },
  "latest_release": {
    "commits": [
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-10 15:01:52 +0000",
        "deletions_count": 1,
        "description": "Push docker images so that `latest` tags are last",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Push docker images so that `latest` tags are last",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "15b44d04a06c91d5e0d1017b251c32ac165f2bd6",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-10 15:19:21 +0000",
        "deletions_count": 1,
        "description": "Print grease command output",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Print grease command output",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "4bc7696077e691f59811e8b1e078f1b029fe63a6",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-11 09:58:21 +0000",
        "deletions_count": 7,
        "description": "Do not release Github or Homebrew on nightly",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 0,
        "message": "chore(operations): Do not release Github or Homebrew on nightly",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "0f5266193c6ae8d7d47907c906e34598e36f2057",
        "type": "chore"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-10-11 09:08:43 +0000",
        "deletions_count": 40,
        "description": "Make global options actually use default",
        "files_count": 6,
        "group": "fix",
        "insertions_count": 56,
        "message": "fix(cli): Make global options actually use default (#1013)",
        "pr_number": 1013,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "1e1d66e04722841e3e0dc9b6d7d85c75379d1caf",
        "type": "fix"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-10-11 10:23:18 +0000",
        "deletions_count": 2,
        "description": "Add relevant when details to config spec",
        "files_count": 17,
        "group": "docs",
        "insertions_count": 74,
        "message": "docs: Add relevant when details to config spec (#1016)",
        "pr_number": 1016,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "a7f7ffa879cd310beca498a600537707b7aee896",
        "type": "docs"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-10-11 12:26:22 +0000",
        "deletions_count": 3683,
        "description": "List out component options as linkable sections",
        "files_count": 95,
        "group": "docs",
        "insertions_count": 3115,
        "message": "docs: List out component options as linkable sections (#1019)",
        "pr_number": 1019,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "1f0c52bcb931bd2e10fa09557e343af50513e166",
        "type": "docs"
      },
      {
        "author": "Lincoln Lee",
        "breaking_change": false,
        "date": "2019-10-14 02:13:53 +0000",
        "deletions_count": 0,
        "description": "Add ca certificates for docker image",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 2,
        "message": "fix(docker platform): Add ca certificates for docker image (#1014)",
        "pr_number": 1014,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "docker platform"
        },
        "sha": "5510b176ce0645d9893ea0e92ac2f73d58515e38",
        "type": "fix"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-13 18:50:02 +0000",
        "deletions_count": 3593,
        "description": "Further improve options documentation for each component",
        "files_count": 122,
        "group": "docs",
        "insertions_count": 3957,
        "message": "docs: Further improve options documentation for each component",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "d4aac2e13c8c3f285cfeb95a6c22695fe07cb18e",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-13 18:53:10 +0000",
        "deletions_count": 456,
        "description": "Remove superflous tags in config examples",
        "files_count": 42,
        "group": "docs",
        "insertions_count": 458,
        "message": "docs: Remove superflous tags in config examples",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "21506409f8bf1311dfb4cd7ce8539d049dd4a5cd",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-13 19:47:18 +0000",
        "deletions_count": 480,
        "description": "Dont repeat default value in configuration examples",
        "files_count": 45,
        "group": "docs",
        "insertions_count": 468,
        "message": "docs: Dont repeat default value in configuration examples",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "aa02c432cca22a9fd8f7425c839156f2613e3e7b",
        "type": "docs"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-10-14 15:10:55 +0000",
        "deletions_count": 1,
        "description": "Initial `datadog_metrics` implementation",
        "files_count": 16,
        "group": "feat",
        "insertions_count": 1085,
        "message": "feat(new sink): Initial `datadog_metrics` implementation (#967)",
        "pr_number": 967,
        "scope": {
          "category": "sink",
          "component_name": null,
          "component_type": "sink",
          "name": "new sink"
        },
        "sha": "d04a3034e3a6ea233be44ddaf59e07c6340d5824",
        "type": "feat"
      },
      {
        "author": "Lincoln Lee",
        "breaking_change": false,
        "date": "2019-10-15 01:43:09 +0000",
        "deletions_count": 1,
        "description": "Remove debian cache to reduce image size",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Remove debian cache to reduce image size (#1028)",
        "pr_number": 1028,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "1378575334e0032de645c8277683f73cf640eb97",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-13 19:49:38 +0000",
        "deletions_count": 76,
        "description": "Dont label unit in config examples",
        "files_count": 20,
        "group": "docs",
        "insertions_count": 80,
        "message": "docs: Dont label unit in config examples",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "c1b36be946a2103a6c5eff77e288f32898a3bbe3",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-14 19:25:25 +0000",
        "deletions_count": 334,
        "description": "Add back section references to option descriptions",
        "files_count": 45,
        "group": "docs",
        "insertions_count": 348,
        "message": "docs: Add back section references to option descriptions",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "571e1390bd4a5455a5b1305ace8fd1724a761ddd",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-15 12:31:14 +0000",
        "deletions_count": 5,
        "description": "Ensure log_to_metric tags option shows in example",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 9,
        "message": "docs: Ensure log_to_metric tags option shows in example",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "22efd48c90d91c9fa9a4d102e54ffb3d869945f3",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-15 12:32:52 +0000",
        "deletions_count": 1,
        "description": "Fix metrics examples syntax error",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 1,
        "message": "docs: Fix metrics examples syntax error",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "5dd167a462930da589f842a366334d65be17d185",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-15 12:36:11 +0000",
        "deletions_count": 1,
        "description": "Fix log data model",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Fix log data model",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "f804cebad4ed97f0da105effbe72b593a846ff9d",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-16 15:30:34 +0000",
        "deletions_count": 5,
        "description": "Add `commit_interval_ms` option",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 17,
        "message": "enhancement(kafka source): Add `commit_interval_ms` option (#944)",
        "pr_number": 944,
        "scope": {
          "category": "source",
          "component_name": "kafka",
          "component_type": "source",
          "name": "kafka source"
        },
        "sha": "a3c7c752e3fec7d3c5d84d4452e1243b263a3ae8",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-10-16 19:19:15 +0000",
        "deletions_count": 8,
        "description": "Fix typos",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 8,
        "message": "docs: Fix typos",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8aaa22524c13a184a8ce0c8eeaa744d556ed4841",
        "type": "docs"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-10-17 14:38:27 +0000",
        "deletions_count": 0,
        "description": "Put buffering tests behind `leveldb` feature",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(testing): Put buffering tests behind `leveldb` feature (#1046)",
        "pr_number": 1046,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "20bc1a29af0ad4cab9f86482873e942627d366c2",
        "type": "chore"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-10-17 15:45:52 +0000",
        "deletions_count": 3,
        "description": "Update `tower-limit` to `v0.1.1`",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 4,
        "message": "chore(operations): Update `tower-limit` to `v0.1.1` (#1018)",
        "pr_number": 1018,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "22fd9ef6f07b4372512185270b729ad0fd21b49c",
        "type": "chore"
      },
      {
        "author": "AlyHKafoury",
        "breaking_change": false,
        "date": "2019-10-17 22:47:58 +0000",
        "deletions_count": 17,
        "description": "Resolve inability to shutdown Vector when std…",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 39,
        "message": "fix(stdin source): Resolve inability to shutdown Vector when std… (#960)",
        "pr_number": 960,
        "scope": {
          "category": "source",
          "component_name": "stdin",
          "component_type": "source",
          "name": "stdin source"
        },
        "sha": "32ed04fb529fcb6a10dfed101dff04447357cf13",
        "type": "fix"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-10-17 18:41:54 +0000",
        "deletions_count": 0,
        "description": "Add address and path to the syslog source example config",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 8,
        "message": "docs: Add address and path to the syslog source example config",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "3e8c906e791505732cea3608fbac9c1878a141bd",
        "type": "docs"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-10-18 13:04:52 +0000",
        "deletions_count": 0,
        "description": "Bump version in Cargo.toml before releasing",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 23,
        "message": "chore(operations): Bump version in Cargo.toml before releasing (#1048)",
        "pr_number": 1048,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "fe26627b13797465d7a94a7ea1e63a7266bf7d42",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-18 22:15:06 +0000",
        "deletions_count": 3,
        "description": "Update leveldb-sys up to 2.0.5",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 3,
        "message": "enhancement(platforms): Update leveldb-sys up to 2.0.5 (#1055)",
        "pr_number": 1055,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "platforms"
        },
        "sha": "875de183748ba7939f53d1c712f1ea1aff7017a8",
        "type": "enhancement"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-10-21 14:19:44 +0000",
        "deletions_count": 204,
        "description": "Apply some fixes for clippy lints",
        "files_count": 36,
        "group": "chore",
        "insertions_count": 188,
        "message": "chore: Apply some fixes for clippy lints (#1034)",
        "pr_number": 1034,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "b2a3c25bbf9e33a9d167eef1ca28d606f405b670",
        "type": "chore"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": true,
        "date": "2019-10-21 16:54:41 +0000",
        "deletions_count": 61,
        "description": "Require `encoding` option for console and file sinks",
        "files_count": 17,
        "group": "breaking change",
        "insertions_count": 116,
        "message": "fix(config)!: Require `encoding` option for console and file sinks (#1033)",
        "pr_number": 1033,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "616d14abf59ac6e29c356fbf43e108dd7a438d35",
        "type": "fix"
      },
      {
        "author": "Yeonghoon Park",
        "breaking_change": false,
        "date": "2019-10-23 06:22:55 +0000",
        "deletions_count": 5,
        "description": "Bundle install should print output on error",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(operations): Bundle install should print output on error (#1068)",
        "pr_number": 1068,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "b6a8778949d9fbb36637bec13bf9a9b03762663b",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-22 16:32:08 +0000",
        "deletions_count": 70,
        "description": "Add support for systemd socket activation",
        "files_count": 23,
        "group": "enhancement",
        "insertions_count": 199,
        "message": "enhancement(networking): Add support for systemd socket activation (#1045)",
        "pr_number": 1045,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "networking"
        },
        "sha": "f90f50abec9f5848b12c216e2962ad45f1a87652",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-23 15:08:45 +0000",
        "deletions_count": 2,
        "description": "Add OpenSSL and pkg-config to development requirements",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 9,
        "message": "docs: Add OpenSSL and pkg-config to development requirements (#1066)",
        "pr_number": 1066,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "afc1edab8b726291850674d6fbbf7c66af2ba6aa",
        "type": "docs"
      },
      {
        "author": "Kruno Tomola Fabro",
        "breaking_change": false,
        "date": "2019-10-23 18:27:01 +0000",
        "deletions_count": 1,
        "description": "Set default `drop_field` to true",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 13,
        "message": "enhancement(regex_parser transform): Set default `drop_field` to true",
        "pr_number": null,
        "scope": {
          "category": "transform",
          "component_name": "regex_parser",
          "component_type": "transform",
          "name": "regex_parser transform"
        },
        "sha": "e56f9503f09a7f97d96093775856a019d738d402",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-10-24 09:02:53 +0000",
        "deletions_count": 83,
        "description": "Add `validate` sub command",
        "files_count": 5,
        "group": "feat",
        "insertions_count": 269,
        "message": "feat(cli): Add `validate` sub command (#1064)",
        "pr_number": 1064,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "018db5f4c65662367cc749f3e4458271a2003e75",
        "type": "feat"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-10-24 12:11:00 +0000",
        "deletions_count": 136,
        "description": "Metrics buffer & aggregation",
        "files_count": 7,
        "group": "enhancement",
        "insertions_count": 875,
        "message": "enhancement(metric data model): Metrics buffer & aggregation (#930)",
        "pr_number": 930,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "metric data model"
        },
        "sha": "c112c4ac7f45e69fea312e7691566a3f9e8e3066",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-24 14:57:57 +0000",
        "deletions_count": 127,
        "description": "Use rdkafka crate from the upstream Git repository",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 118,
        "message": "chore(operations): Use rdkafka crate from the upstream Git repository (#1063)",
        "pr_number": 1063,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "137e9ea7495eabca272207a904b9dd4c2f82d6af",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-10-24 15:13:08 +0000",
        "deletions_count": 635,
        "description": "Check config examples",
        "files_count": 37,
        "group": "chore",
        "insertions_count": 18,
        "message": "chore(config): Check config examples (#1082)",
        "pr_number": 1082,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "4cde6dc5021d06e07393af135d0625178385802a",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-24 11:52:44 +0000",
        "deletions_count": 12,
        "description": "Fix a couple minor issues with checkpointing",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 17,
        "message": "fix(journald source): Fix a couple minor issues with checkpointing (#1086)",
        "pr_number": 1086,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "ef5ec5732fd4f677f0b25e3f6e470c37d0f73855",
        "type": "fix"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-24 13:17:07 +0000",
        "deletions_count": 1,
        "description": "Fix merge problem in Cargo.lock",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(operations): Fix merge problem in Cargo.lock (#1087)",
        "pr_number": 1087,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "8fef7056a1d1c515014e721a2940d04ff269a704",
        "type": "chore"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-10-25 09:40:42 +0000",
        "deletions_count": 17,
        "description": "Use metric buffer in Datadog sink",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 17,
        "message": "enhancement(datadog_metrics sink): Use metric buffer in Datadog sink (#1080)",
        "pr_number": 1080,
        "scope": {
          "category": "sink",
          "component_name": "datadog_metrics",
          "component_type": "sink",
          "name": "datadog_metrics sink"
        },
        "sha": "c97173fb472ffeb11902e3385dc212fdef8a0ffa",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-28 14:20:14 +0000",
        "deletions_count": 6,
        "description": "Update `ctor` dependency",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 6,
        "message": "chore(operations): Update `ctor` dependency (#1095)",
        "pr_number": 1095,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "25813de321b097677e7c23069082b8e3597928e8",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-28 14:50:20 +0000",
        "deletions_count": 1,
        "description": "Avoid dependency on platform-specific machine word size",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(operations): Avoid dependency on platform-specific machine word size (#1096)",
        "pr_number": 1096,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "ccae97b37b04b590ddf64284fd593afdfb024b22",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-28 15:02:09 +0000",
        "deletions_count": 13,
        "description": "Rework option to limit records to current boot in journald source",
        "files_count": 7,
        "group": "fix",
        "insertions_count": 36,
        "message": "fix(journald source): Rework option to limit records to current boot in journald source (#1105)",
        "pr_number": 1105,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "7ca6dc31a3af3e6e08ef89a469923fa385e5df30",
        "type": "fix"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-10-28 18:34:13 +0000",
        "deletions_count": 7,
        "description": "Wrap provider call with a tokio runtime",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 11,
        "message": "enhancement(elasticsearch sink): Wrap provider call with a tokio runtime (#1104)",
        "pr_number": 1104,
        "scope": {
          "category": "sink",
          "component_name": "elasticsearch",
          "component_type": "sink",
          "name": "elasticsearch sink"
        },
        "sha": "f9a6776a4467cd8a5c4ffdaa44a8a5593f6471ac",
        "type": "enhancement"
      },
      {
        "author": "David O'Rourke",
        "breaking_change": false,
        "date": "2019-10-29 17:26:32 +0000",
        "deletions_count": 77,
        "description": "Update Rusoto to 0.38.0",
        "files_count": 8,
        "group": "chore",
        "insertions_count": 80,
        "message": "chore(operations): Update Rusoto to 0.38.0 (#1112)",
        "pr_number": 1112,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "603f1e3331e44c2b486cb8f5570109987b0a261e",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-29 20:30:57 +0000",
        "deletions_count": 1,
        "description": "Increase sleep interval in the tests for file source",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(file source): Increase sleep interval in the tests for file source (#1113)",
        "pr_number": 1113,
        "scope": {
          "category": "source",
          "component_name": "file",
          "component_type": "source",
          "name": "file source"
        },
        "sha": "9e2f98e780fdca4380f701508eb6f35e924d8d8b",
        "type": "chore"
      },
      {
        "author": "David O'Rourke",
        "breaking_change": false,
        "date": "2019-10-29 18:01:52 +0000",
        "deletions_count": 116,
        "description": "Update Rusoto to 0.41.x",
        "files_count": 5,
        "group": "chore",
        "insertions_count": 79,
        "message": "chore(operations): Update Rusoto to 0.41.x (#1114)",
        "pr_number": 1114,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "539f7086459692fe8b52493cdf053220af687d92",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-29 14:42:21 +0000",
        "deletions_count": 12,
        "description": "Cursor/checkpoint fixes",
        "files_count": 5,
        "group": "fix",
        "insertions_count": 77,
        "message": "fix(journald source): Cursor/checkpoint fixes (#1106)",
        "pr_number": 1106,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "ddffd3b91588da87b3c3a1623ac1f7be842f2392",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-30 20:12:56 +0000",
        "deletions_count": 6,
        "description": "Use `rlua` crate from a fork with Pairs implementation",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 16,
        "message": "chore(operations): Use `rlua` crate from a fork with Pairs implementation (#1119)",
        "pr_number": 1119,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "a5d442c9d311fb100d1912d5a0c422a847dbbdc3",
        "type": "chore"
      },
      {
        "author": "Steven Aerts",
        "breaking_change": false,
        "date": "2019-10-30 18:13:29 +0000",
        "deletions_count": 0,
        "description": "Allow iteration over fields",
        "files_count": 2,
        "group": "enhancement",
        "insertions_count": 61,
        "message": "enhancement(lua transform): Allow iteration over fields (#1111)",
        "pr_number": 1111,
        "scope": {
          "category": "transform",
          "component_name": "lua",
          "component_type": "transform",
          "name": "lua transform"
        },
        "sha": "219b9259bad71e36a7e1863c8add85a902bc057f",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-10-30 20:48:54 +0000",
        "deletions_count": 13,
        "description": "Move example of iterating over all fields out of the autogenerated file",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 26,
        "message": "docs(lua transform): Move example of iterating over all fields out of the autogenerated file (#1120)",
        "pr_number": 1120,
        "scope": {
          "category": "transform",
          "component_name": "lua",
          "component_type": "transform",
          "name": "lua transform"
        },
        "sha": "ec2c9970ed16c3b06f5dc328b7edd6460db4f310",
        "type": "docs"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-30 14:16:04 +0000",
        "deletions_count": 0,
        "description": "Flatten out region configuration in elasticsearch sink",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(elasticsearch sink): Flatten out region configuration in elasticsearch sink (#1116)",
        "pr_number": 1116,
        "scope": {
          "category": "sink",
          "component_name": "elasticsearch",
          "component_type": "sink",
          "name": "elasticsearch sink"
        },
        "sha": "608e21abe8198a90b1100868b46550d63ab95c8c",
        "type": "fix"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-10-31 12:07:34 +0000",
        "deletions_count": 22,
        "description": "Improve topology tracing spans",
        "files_count": 47,
        "group": "fix",
        "insertions_count": 276,
        "message": "fix(observability): Improve topology tracing spans (#1123)",
        "pr_number": 1123,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "observability"
        },
        "sha": "55766802be0a6c35eb6e1f8d35be9081401b27de",
        "type": "fix"
      },
      {
        "author": "Michael Nitschinger",
        "breaking_change": false,
        "date": "2019-10-31 20:03:31 +0000",
        "deletions_count": 4,
        "description": "Update grok to version 1.0.1",
        "files_count": 2,
        "group": "enhancement",
        "insertions_count": 4,
        "message": "enhancement(grok_parser transform): Update grok to version 1.0.1 (#1124)",
        "pr_number": 1124,
        "scope": {
          "category": "transform",
          "component_name": "grok_parser",
          "component_type": "transform",
          "name": "grok_parser transform"
        },
        "sha": "641bc4242c7e86cde031a51e4228edb0a66bec27",
        "type": "enhancement"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-10-31 14:56:23 +0000",
        "deletions_count": 11,
        "description": "Limit journald records to the current boot",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 34,
        "message": "fix(journald source): Limit journald records to the current boot (#1122)",
        "pr_number": 1122,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "67ee5cc3055da22e5f9eb4861f8be383c2f72f1c",
        "type": "fix"
      },
      {
        "author": "Michael-J-Ward",
        "breaking_change": false,
        "date": "2019-11-01 08:44:37 +0000",
        "deletions_count": 98,
        "description": "Abstracts runtime into runtime.rs",
        "files_count": 23,
        "group": "chore",
        "insertions_count": 170,
        "message": "chore(operations): Abstracts runtime into runtime.rs (#1098)",
        "pr_number": 1098,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "70482ab33c44226f392877461cb8be833f8bbdd6",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-04 14:29:31 +0000",
        "deletions_count": 10,
        "description": "Add Cargo.toml version check to CI",
        "files_count": 5,
        "group": "chore",
        "insertions_count": 84,
        "message": "chore(operations): Add Cargo.toml version check to CI (#1102)",
        "pr_number": 1102,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "e13b2131dbe297be8ce53f627affe52a9a26ca5d",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-04 15:23:32 +0000",
        "deletions_count": 2,
        "description": "Handle edge cases in the Cargo.toml version check",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(operations): Handle edge cases in the Cargo.toml version check (#1138)",
        "pr_number": 1138,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "933fd510ba4e8ae7a6184515371d7a3c0d97dc75",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-04 15:29:42 +0000",
        "deletions_count": 1,
        "description": "Bump version in Cargo.toml to 0.6.0",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Bump version in Cargo.toml to 0.6.0 (#1139)",
        "pr_number": 1139,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "6f236505b5808e0da01cd08df20334ced2f48edf",
        "type": "chore"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-11-04 10:13:29 +0000",
        "deletions_count": 28,
        "description": "Automatically create missing directories",
        "files_count": 6,
        "group": "enhancement",
        "insertions_count": 121,
        "message": "enhancement(file sink): Automatically create missing directories (#1094)",
        "pr_number": 1094,
        "scope": {
          "category": "sink",
          "component_name": "file",
          "component_type": "sink",
          "name": "file sink"
        },
        "sha": "3b3c824e98c8ae120f32ffb3603077792c165141",
        "type": "enhancement"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-11-04 11:35:33 +0000",
        "deletions_count": 1,
        "description": "Update lock file for 0.6",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Update lock file for 0.6 (#1140)",
        "pr_number": 1140,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "d9550711ebcc3bd1033b4985efb3af469e8a4384",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-04 23:33:29 +0000",
        "deletions_count": 17,
        "description": "Show Git version and target triple in `vector --version` output",
        "files_count": 5,
        "group": "enhancement",
        "insertions_count": 40,
        "message": "enhancement(cli): Show Git version and target triple in `vector --version` output (#1044)",
        "pr_number": 1044,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "a0a5bee914ea94353d545e2d772978ba7963b20f",
        "type": "enhancement"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-11-04 15:51:53 +0000",
        "deletions_count": 1380,
        "description": "Update lock file",
        "files_count": 10,
        "group": "chore",
        "insertions_count": 880,
        "message": "chore: Update lock file (#1133)",
        "pr_number": 1133,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8be060fc48eb504c30f874fead15f144570cbeb3",
        "type": "chore"
      },
      {
        "author": "David Howell",
        "breaking_change": false,
        "date": "2019-11-05 09:15:57 +0000",
        "deletions_count": 1,
        "description": "Flush and reset any current filter before applying new filter",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(journald source): Flush and reset any current filter before applying new filter (#1135)",
        "pr_number": 1135,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "96bd716fc1c022831eb04afd633ede3efe809d28",
        "type": "fix"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-11-06 09:10:51 +0000",
        "deletions_count": 0,
        "description": "Ensure internal rate limiting is logged",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 1,
        "message": "enhancement(observability): Ensure internal rate limiting is logged (#1151)",
        "pr_number": 1151,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "observability"
        },
        "sha": "c7ad707ed296a93e3d82bff2b3d7793178d50bcc",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-06 22:17:55 +0000",
        "deletions_count": 40,
        "description": "Use inventory for plugins",
        "files_count": 42,
        "group": "chore",
        "insertions_count": 280,
        "message": "chore(config): Use inventory for plugins (#1115)",
        "pr_number": 1115,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "eb0566313849002fa820d57cc15d8a9ec957b9d3",
        "type": "chore"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-11-07 10:22:10 +0000",
        "deletions_count": 17,
        "description": "Fix metrics batch strategy in sinks",
        "files_count": 6,
        "group": "fix",
        "insertions_count": 7,
        "message": "fix(aws_cloudwatch_metrics sink): Fix metrics batch strategy in sinks (#1141)",
        "pr_number": 1141,
        "scope": {
          "category": "sink",
          "component_name": "aws_cloudwatch_metrics",
          "component_type": "sink",
          "name": "aws_cloudwatch_metrics sink"
        },
        "sha": "fefe9ef4c8f1f20513bc31545d36ab00ed09c4a7",
        "type": "fix"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-08 14:30:47 +0000",
        "deletions_count": 130,
        "description": "Refactor the batching configuration",
        "files_count": 12,
        "group": "enhancement",
        "insertions_count": 132,
        "message": "enhancement(config): Refactor the batching configuration (#1154)",
        "pr_number": 1154,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "f4adfd716034141f367e93bebf283d703c09dfaa",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-08 20:35:06 +0000",
        "deletions_count": 1,
        "description": "Add `list` subcommand",
        "files_count": 4,
        "group": "feat",
        "insertions_count": 98,
        "message": "feat(cli): Add `list` subcommand (#1156)",
        "pr_number": 1156,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "cfab2339b9b3f8117d816015d6523976b38190cc",
        "type": "feat"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-11-08 15:58:14 +0000",
        "deletions_count": 6,
        "description": "Stop accidentally requiring region for ES",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 24,
        "message": "fix(elasticsearch sink): Stop accidentally requiring region for ES (#1161)",
        "pr_number": 1161,
        "scope": {
          "category": "sink",
          "component_name": "elasticsearch",
          "component_type": "sink",
          "name": "elasticsearch sink"
        },
        "sha": "200dccccc58cf5f7fec86b3124ed00e9ad0d5366",
        "type": "fix"
      },
      {
        "author": "dependabot[bot]",
        "breaking_change": false,
        "date": "2019-11-09 18:36:10 +0000",
        "deletions_count": 3,
        "description": "Bump loofah from 2.2.3 to 2.3.1 in /scripts",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 3,
        "message": "chore(operatons): Bump loofah from 2.2.3 to 2.3.1 in /scripts (#1163)",
        "pr_number": 1163,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operatons"
        },
        "sha": "4b831475ed4cb6a016b18b4fa4f2457f0591ce21",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-11 17:30:27 +0000",
        "deletions_count": 17,
        "description": "Use vendored OpenSSL",
        "files_count": 3,
        "group": "enhancement",
        "insertions_count": 20,
        "message": "enhancement(platforms): Use vendored OpenSSL (#1170)",
        "pr_number": 1170,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "platforms"
        },
        "sha": "32cfe37c87a01ae08b61627d31be73ecf840d375",
        "type": "enhancement"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-11-11 09:37:36 +0000",
        "deletions_count": 1,
        "description": "upgrade to rust 1.39.0",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): upgrade to rust 1.39.0 (#1159)",
        "pr_number": 1159,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "fb9c17a26959e8276770a86307807721cd2ded25",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-11 20:34:23 +0000",
        "deletions_count": 0,
        "description": "Add `clean` target to Makefile",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 3,
        "message": "enhancement(operations): Add `clean` target to Makefile (#1171)",
        "pr_number": 1171,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "8de50f4603b3e7626af27b24d9a350eaadb9b4e7",
        "type": "enhancement"
      },
      {
        "author": "Kruno Tomola Fabro",
        "breaking_change": false,
        "date": "2019-11-12 00:09:45 +0000",
        "deletions_count": 4,
        "description": "Fixes a bug droping parsed field",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 24,
        "message": "fix(json_parser transform): Fixes a bug droping parsed field (#1167)",
        "pr_number": 1167,
        "scope": {
          "category": "transform",
          "component_name": "json_parser",
          "component_type": "transform",
          "name": "json_parser transform"
        },
        "sha": "f9d3111015352910e71dab210c376b09cdd26333",
        "type": "fix"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-13 13:16:25 +0000",
        "deletions_count": 60,
        "description": "`host` is not required when provider is AWS",
        "files_count": 5,
        "group": "fix",
        "insertions_count": 112,
        "message": "fix(elasticsearch sink): `host` is not required when provider is AWS (#1164)",
        "pr_number": 1164,
        "scope": {
          "category": "sink",
          "component_name": "elasticsearch",
          "component_type": "sink",
          "name": "elasticsearch sink"
        },
        "sha": "a272f633464ce06ab28e5d9a7c1e7d6b595c61ec",
        "type": "fix"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-13 15:34:38 +0000",
        "deletions_count": 1,
        "description": " Limit the number of CircleCI build jobs to 8",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations):  Limit the number of CircleCI build jobs to 8 (#1176)",
        "pr_number": 1176,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "2100100b5cda0f57292a17bbf4473ed543811f39",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-13 15:34:59 +0000",
        "deletions_count": 1,
        "description": "Fix missed `cargo fmt` run on elasticsearch sink",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 3,
        "message": "chore: Fix missed `cargo fmt` run on elasticsearch sink (#1175)",
        "pr_number": 1175,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "2e2af43786ff0dbc292f98cedc830791d1e20937",
        "type": "chore"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-11-13 17:21:05 +0000",
        "deletions_count": 1,
        "description": "Don't drop parsed field",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 20,
        "message": "fix(grok_parser transform): Don't drop parsed field (#1172)",
        "pr_number": 1172,
        "scope": {
          "category": "transform",
          "component_name": "grok_parser",
          "component_type": "transform",
          "name": "grok_parser transform"
        },
        "sha": "cfb66e5b90007d9a5dc461afa80e6d3e190febcf",
        "type": "fix"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-13 21:23:21 +0000",
        "deletions_count": 3,
        "description": "Add support for target field configuration",
        "files_count": 6,
        "group": "enhancement",
        "insertions_count": 152,
        "message": "enhancement(json_parser transform): Add support for target field configuration (#1165)",
        "pr_number": 1165,
        "scope": {
          "category": "transform",
          "component_name": "json_parser",
          "component_type": "transform",
          "name": "json_parser transform"
        },
        "sha": "e0433fd1ada425c1f5c9505426fa362aae14249e",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-14 10:49:59 +0000",
        "deletions_count": 6,
        "description": "Add `generate` subcommand",
        "files_count": 6,
        "group": "feat",
        "insertions_count": 272,
        "message": "feat(cli): Add `generate` subcommand (#1168)",
        "pr_number": 1168,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "e503057ff3616569521a208abbbed8c3e8fbc848",
        "type": "feat"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-14 21:24:43 +0000",
        "deletions_count": 28,
        "description": "Use `strptime` instead of `strftime` in docs where appropriate",
        "files_count": 13,
        "group": "docs",
        "insertions_count": 28,
        "message": "docs: Use `strptime` instead of `strftime` in docs where appropriate (#1183)",
        "pr_number": 1183,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "de0a6734710a6c63c969048a06d3b55ae1637c87",
        "type": "docs"
      },
      {
        "author": "Jean Mertz",
        "breaking_change": false,
        "date": "2019-11-14 20:23:38 +0000",
        "deletions_count": 4,
        "description": "Support default environment variable values",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 11,
        "message": "enhancement(config): Support default environment variable values (#1185)",
        "pr_number": 1185,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "fc2c1db5824f8499190efa078c993f3f52737043",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-14 23:49:51 +0000",
        "deletions_count": 2,
        "description": "Update rdkafka to fix rdkafka/cmake feature",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(operations): Update rdkafka to fix rdkafka/cmake feature (#1186)",
        "pr_number": 1186,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "20ba2575f40944b36c7bbd9e4d821452626f288b",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-14 23:50:35 +0000",
        "deletions_count": 4,
        "description": "Use leveldb from fork with improved portability",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 4,
        "message": "chore(operations): Use leveldb from fork with improved portability (#1184)",
        "pr_number": 1184,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "84d830b57de1798b2aac61279f7a0ae99f854241",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-14 23:50:59 +0000",
        "deletions_count": 8,
        "description": "Increase wait timeouts in tests which otherwise fail on slow CPUs",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 8,
        "message": "fix(testing): Increase wait timeouts in tests which otherwise fail on slow CPUs (#1181)",
        "pr_number": 1181,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "3ce0b4ed645d2844f1f6c5308409e2e9466c0799",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-19 17:35:50 +0000",
        "deletions_count": 3,
        "description": "Control which version of leveldb-sys to use with features",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 3,
        "message": "chore(operations): Control which version of leveldb-sys to use with features (#1191)",
        "pr_number": 1191,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "0884f5d90ca2162aaa0ea6b9ab5d2e10a026a286",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-19 17:39:05 +0000",
        "deletions_count": 0,
        "description": "Support `armv7-unknown-linux` (Raspberry Pi, etc) platforms",
        "files_count": 4,
        "group": "feat",
        "insertions_count": 366,
        "message": "feat(new platform): Support `armv7-unknown-linux` (Raspberry Pi, etc) platforms (#1054)",
        "pr_number": 1054,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "new platform"
        },
        "sha": "90388ed57afea24d569b2317d97df7035211b252",
        "type": "feat"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-19 17:41:09 +0000",
        "deletions_count": 10,
        "description": "Support `aarch64-unknown-linux` (ARM64, Raspberry Pi, etc) platforms",
        "files_count": 4,
        "group": "feat",
        "insertions_count": 347,
        "message": "feat(new platform): Support `aarch64-unknown-linux` (ARM64, Raspberry Pi, etc) platforms (#1193)",
        "pr_number": 1193,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "new platform"
        },
        "sha": "d58139caf6cdb15b4622360d7c9a04a8c86724d6",
        "type": "feat"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-11-19 15:24:03 +0000",
        "deletions_count": 37,
        "description": "Re-fix journald cursor handling and libsystemd name",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 34,
        "message": "fix(journald source): Re-fix journald cursor handling and libsystemd name (#1202)",
        "pr_number": 1202,
        "scope": {
          "category": "source",
          "component_name": "journald",
          "component_type": "source",
          "name": "journald source"
        },
        "sha": "1b833eb6d693d4c281aa51c332202eb2796ba4db",
        "type": "fix"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-19 16:51:07 +0000",
        "deletions_count": 23643,
        "description": "New website and documentation",
        "files_count": 496,
        "group": "docs",
        "insertions_count": 39821,
        "message": "docs: New website and documentation (#1207)",
        "pr_number": 1207,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "2d2fadb2599d99ded3d73286fe17a67d20d23805",
        "type": "docs"
      },
      {
        "author": "Jean Mertz",
        "breaking_change": false,
        "date": "2019-11-20 00:27:10 +0000",
        "deletions_count": 0,
        "description": "Initial `ansi_stripper` transform implementation",
        "files_count": 5,
        "group": "feat",
        "insertions_count": 158,
        "message": "feat(new transform): Initial `ansi_stripper` transform implementation (#1188)",
        "pr_number": 1188,
        "scope": {
          "category": "transform",
          "component_name": null,
          "component_type": "transform",
          "name": "new transform"
        },
        "sha": "2d419d57d5ab6072bc1058126bc3be50fa57c835",
        "type": "feat"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-20 14:37:14 +0000",
        "deletions_count": 2,
        "description": "Fix README banner",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 146,
        "message": "docs: Fix README banner",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "55b68910ee3d80fdf302faf5a5bc9aa1f68e8dce",
        "type": "docs"
      },
      {
        "author": "Amit Saha",
        "breaking_change": false,
        "date": "2019-11-21 08:36:02 +0000",
        "deletions_count": 0,
        "description": "Initial `geoip` transform implementation",
        "files_count": 6,
        "group": "feat",
        "insertions_count": 286,
        "message": "feat(new transform): Initial `geoip` transform implementation (#1015)",
        "pr_number": 1015,
        "scope": {
          "category": "transform",
          "component_name": null,
          "component_type": "transform",
          "name": "new transform"
        },
        "sha": "458f6cc0e3fbc6fded1fdf8d47dedb2d0be3bb2d",
        "type": "feat"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-20 21:31:34 +0000",
        "deletions_count": 307,
        "description": "Small website and documentation improvements",
        "files_count": 28,
        "group": "docs",
        "insertions_count": 880,
        "message": "docs: Small website and documentation improvements (#1215)",
        "pr_number": 1215,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "803c7f98349a4d07bfc68bc7f10a80c165698f1a",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-21 00:14:23 +0000",
        "deletions_count": 5,
        "description": "Small changes to website homepage styles",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 9,
        "message": "docs: Small changes to website homepage styles",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "fb6a1dc7d41a73869b36d20863f410a3f3d9a844",
        "type": "docs"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-21 15:28:49 +0000",
        "deletions_count": 11,
        "description": "Fix some URLs",
        "files_count": 4,
        "group": "docs",
        "insertions_count": 7,
        "message": "docs: Fix some URLs",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "42ca451408b42db43ea2597509e0ce85b44059a9",
        "type": "docs"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-21 15:39:33 +0000",
        "deletions_count": 91,
        "description": "Allow >1 config targets for validate command",
        "files_count": 3,
        "group": "enhancement",
        "insertions_count": 82,
        "message": "enhancement(cli): Allow >1 config targets for validate command (#1218)",
        "pr_number": 1218,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "9fe1eeb4786b27843673c05ff012f6b5cf5c3e45",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-21 23:53:20 +0000",
        "deletions_count": 2,
        "description": "Fix components link in README",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Fix components link in README (#1222)",
        "pr_number": 1222,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "3af177516728cc4a78a198f69d1cb6b0f0b093fc",
        "type": "docs"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-21 16:13:16 +0000",
        "deletions_count": 4232,
        "description": "Rename components section to reference in docs",
        "files_count": 134,
        "group": "docs",
        "insertions_count": 740,
        "message": "docs: Rename components section to reference in docs (#1223)",
        "pr_number": 1223,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "58246b306f0e927cfc2ffcfb6f023c146846db0e",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-21 16:30:11 +0000",
        "deletions_count": 4,
        "description": "Styling fixes",
        "files_count": 4,
        "group": "docs",
        "insertions_count": 13,
        "message": "docs: Styling fixes",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "89c50b177689cbacf4dc3f930ebbe2b264046b8a",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 00:49:04 +0000",
        "deletions_count": 3,
        "description": "Fix restoring of `rust-toolchain` file",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(operations): Fix restoring of `rust-toolchain` file (#1224)",
        "pr_number": 1224,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "5b38129d0de1185235e630a571e31c3e9f5ab85c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 01:25:18 +0000",
        "deletions_count": 1,
        "description": "Produce archives for `armv7-unknown-linux-musleabihf`",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 29,
        "message": "chore(operations): Produce archives for `armv7-unknown-linux-musleabihf` (#1225)",
        "pr_number": 1225,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "5f39c2f3515d958d40c9a6187c59806c4731c91c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 02:01:41 +0000",
        "deletions_count": 72,
        "description": "Support `x86_64-pc-windows-msvc` (Windows 7+) platform",
        "files_count": 15,
        "group": "feat",
        "insertions_count": 337,
        "message": "feat(new platform): Support `x86_64-pc-windows-msvc` (Windows 7+) platform (#1205)",
        "pr_number": 1205,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "new platform"
        },
        "sha": "a1410f69382bd8036a7046a156c64f56e8f9ef33",
        "type": "feat"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-21 23:06:41 +0000",
        "deletions_count": 53,
        "description": "Update downloads links",
        "files_count": 11,
        "group": "docs",
        "insertions_count": 144,
        "message": "docs: Update downloads links",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "bf9402b2151d976edd42b35d08c1722de7ec2b9b",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 12:58:49 +0000",
        "deletions_count": 374,
        "description": "Fix `check-generate` check in CI",
        "files_count": 8,
        "group": "chore",
        "insertions_count": 398,
        "message": "chore(operations): Fix `check-generate` check in CI (#1226)",
        "pr_number": 1226,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "5062b39a82949c86fdc80658085a88b78a24a27c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 14:15:54 +0000",
        "deletions_count": 5,
        "description": "Use bash from Docker containers as a shell in Circle CI",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 13,
        "message": "chore(operations): Use bash from Docker containers as a shell in Circle CI (#1227)",
        "pr_number": 1227,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "befb29916c2d19827303109769ca824fbd167870",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 14:51:24 +0000",
        "deletions_count": 12,
        "description": "Fix invocation of check jobs",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 12,
        "message": "chore(operations): Fix invocation of check jobs (#1229)",
        "pr_number": 1229,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "6052cbc9a00eac0b2db96651730bd730c39ca83e",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 16:04:48 +0000",
        "deletions_count": 10,
        "description": "Verify `zip` archives for `x86_64-pc-windows-msvc` in `wine`",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 17,
        "message": "chore(operations): Verify `zip` archives for `x86_64-pc-windows-msvc` in `wine` (#1228)",
        "pr_number": 1228,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "d7a0fd1362f7b99a3bac344434d2a50305f1fa2e",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 10:25:57 +0000",
        "deletions_count": 90,
        "description": "Update to docusaurus alpha.36",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 82,
        "message": "chore(website): Update to docusaurus alpha.36",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "7906dcae3c0a43c99880f2cea9aeb01de629157c",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 11:22:16 +0000",
        "deletions_count": 3,
        "description": "Fix curl commands mentioned in #1234",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 4,
        "message": "docs: Fix curl commands mentioned in #1234",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "49a861ab3045570f1e173c56fa23291e014856a2",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-22 16:49:49 +0000",
        "deletions_count": 1,
        "description": "Run nightly builds at 5pm UTC",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Run nightly builds at 5pm UTC",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "39bd126fe67b048003532c178c64be90ef4cec62",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 13:19:53 +0000",
        "deletions_count": 6,
        "description": "Redraw diagram to fix an initial load issue in Chrome",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 4,
        "message": "docs: Redraw diagram to fix an initial load issue in Chrome",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "fe32fdc5d222182f18e4118af28d72d4b06dca0d",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 15:45:12 +0000",
        "deletions_count": 7,
        "description": "Rerender diagram to fix Chrome update issue",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 10,
        "message": "docs: Rerender diagram to fix Chrome update issue",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "6de3e4f3a725c978ccaa95c5a9180df202c5a074",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 16:43:22 +0000",
        "deletions_count": 4,
        "description": "More Chrome fixes",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 4,
        "message": "chore(website): More Chrome fixes",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "12d36bbe2eb223ab89335b61dfbb7e18c4649981",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 17:00:30 +0000",
        "deletions_count": 8,
        "description": "Fix Chrome sorting issue",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 8,
        "message": "chore(website): Fix Chrome sorting issue",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "f9396da79b49f617ce93d6be233f9592831fab2d",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 19:32:52 +0000",
        "deletions_count": 182,
        "description": "Fix readme",
        "files_count": 5,
        "group": "docs",
        "insertions_count": 47,
        "message": "docs: Fix readme",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "662c5d1346ea2b01c0bc3c11c648cbdf92035fe2",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-22 19:36:11 +0000",
        "deletions_count": 11,
        "description": "Fix readme component counts",
        "files_count": 4,
        "group": "docs",
        "insertions_count": 11,
        "message": "docs: Fix readme component counts",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "cb6571798af5b80c123905b4cac3a56a67fc3181",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-23 11:50:14 +0000",
        "deletions_count": 7,
        "description": "Make `openssl/vendored` feature optional",
        "files_count": 2,
        "group": "enhancement",
        "insertions_count": 7,
        "message": "enhancement(platforms): Make `openssl/vendored` feature optional (#1239)",
        "pr_number": 1239,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "platforms"
        },
        "sha": "1f401a68bdb5c0bcfc9d0385f49a70f22fbce5d9",
        "type": "enhancement"
      },
      {
        "author": "Austin Seipp",
        "breaking_change": false,
        "date": "2019-11-23 04:21:20 +0000",
        "deletions_count": 6,
        "description": "Accept metric events, too",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 8,
        "message": "enhancement(blackhole sink): Accept metric events, too (#1237)",
        "pr_number": 1237,
        "scope": {
          "category": "sink",
          "component_name": "blackhole",
          "component_type": "sink",
          "name": "blackhole sink"
        },
        "sha": "52a49d5a32f091eec7c174b02803f7fc3ca5af34",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-23 13:27:51 +0000",
        "deletions_count": 14,
        "description": "Update `openssl` dependency",
        "files_count": 2,
        "group": "enhancement",
        "insertions_count": 14,
        "message": "enhancement(platforms): Update `openssl` dependency (#1240)",
        "pr_number": 1240,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "platforms"
        },
        "sha": "457f964bde42fce3b92e5bd1a65ef6192c404a16",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-23 15:49:09 +0000",
        "deletions_count": 0,
        "description": "Don't put *.erb files to configs directory",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 2,
        "message": "fix(platforms): Don't put *.erb files to configs directory (#1241)",
        "pr_number": 1241,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "platforms"
        },
        "sha": "cdee561f8c1a023b77c5db712cc081b90570eb55",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-23 22:51:25 +0000",
        "deletions_count": 351,
        "description": "Document installation on Windows",
        "files_count": 37,
        "group": "docs",
        "insertions_count": 1064,
        "message": "docs: Document installation on Windows (#1235)",
        "pr_number": 1235,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "b449b2b67f077760215294c418688c27f3f629a0",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-23 15:01:47 +0000",
        "deletions_count": 1,
        "description": "Add docker to homepage",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 151,
        "message": "docs: Add docker to homepage",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "663be72997339cb9c30f935d9ef4c8e7732bc56c",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-23 15:13:26 +0000",
        "deletions_count": 1,
        "description": "Update docker image",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Update docker image",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "732265e9be0ae4c5add4679ef11fe808032c8f78",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-23 15:40:52 +0000",
        "deletions_count": 1,
        "description": "Fix administrating doc",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 31,
        "message": "docs: Fix administrating doc",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "5c15a3c6c7811315ff980e57f685d7fd3616ca7e",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-23 15:41:36 +0000",
        "deletions_count": 0,
        "description": "Add administration to docs sidebar",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Add administration to docs sidebar",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "068ae60a963523e540f2f404545e287a8b161037",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-23 20:46:47 +0000",
        "deletions_count": 5,
        "description": "Add C++ toolchain installation step",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 9,
        "message": "docs: Add C++ toolchain installation step",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "cdcd624da93fd36676e84426b8ec93917a90c8e1",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 01:14:17 +0000",
        "deletions_count": 20,
        "description": "Attempt to fix website theme flickering",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 25,
        "message": "chore(website): Attempt to fix website theme flickering",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "d7b7735ae57e362e8255a59a578ac12f4b438119",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-24 10:26:30 +0000",
        "deletions_count": 25,
        "description": "Describe build features",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 82,
        "message": "docs: Describe build features (#1243)",
        "pr_number": 1243,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "1ec95b9df9a1f0456c02dcfd9824024ed7516fcc",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 12:03:02 +0000",
        "deletions_count": 3,
        "description": "Add ARMv7 to installation docs",
        "files_count": 6,
        "group": "docs",
        "insertions_count": 84,
        "message": "docs: Add ARMv7 to installation docs",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "37e60137b4fab70dc97cc177ecd6f1c81b1c86b0",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 12:24:10 +0000",
        "deletions_count": 15,
        "description": "Various installation docs corrections, closes #1234",
        "files_count": 8,
        "group": "docs",
        "insertions_count": 27,
        "message": "docs: Various installation docs corrections, closes #1234",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8698eb922c5e1a1a0906fe25e2e9f2a39acb9c06",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 12:26:07 +0000",
        "deletions_count": 5,
        "description": "Remove Alogia search until it has indexed everything",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(website): Remove Alogia search until it has indexed everything",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "818c28228965d9d0b691e18298127eb5666d7865",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-24 21:56:40 +0000",
        "deletions_count": 7,
        "description": "Fix passing environment variables inside the CI Docker containers",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 16,
        "message": "chore(operations): Fix passing environment variables inside the CI Docker containers (#1233)",
        "pr_number": 1233,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "b00996fc6949d6d34fcd13f685b5b91d116f4e8c",
        "type": "chore"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-24 15:06:09 +0000",
        "deletions_count": 141,
        "description": "Add operating system as a compenent attribute and filter",
        "files_count": 59,
        "group": "chore",
        "insertions_count": 619,
        "message": "chore(website): Add operating system as a compenent attribute and filter (#1244)",
        "pr_number": 1244,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "604b40d15bcbfb62eae0ca314ffad06a365ccc85",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 15:56:01 +0000",
        "deletions_count": 2,
        "description": "Fix operating system filter",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(website): Fix operating system filter",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "dde45458aa375d5c9e1eb7beb4bf9fe102ccb0db",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 16:02:39 +0000",
        "deletions_count": 33,
        "description": "Dont show operating systems for transforms",
        "files_count": 16,
        "group": "chore",
        "insertions_count": 33,
        "message": "chore(website): Dont show operating systems for transforms",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "0cad20f837f1f682f9a5b976e150417484e4839f",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 17:14:28 +0000",
        "deletions_count": 1,
        "description": "Fix broken link on homepage",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 1,
        "message": "docs: Fix broken link on homepage",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "cad2349778d5d42e71ed12c7cf974e6f9ef731d5",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-24 21:43:05 +0000",
        "deletions_count": 1,
        "description": "Add sidebar background and ga id",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(website): Add sidebar background and ga id",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "9bdaf14ee089da0ab6dff3b464a3086fc709cec6",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-25 11:12:50 +0000",
        "deletions_count": 2,
        "description": "Fix link",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Fix link",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "545ea5b0c1f88fc8ee42c9bce13358155bbf34fe",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-25 15:25:18 +0000",
        "deletions_count": 2,
        "description": "Fix name of `shiplift/unix-socket` feature",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Fix name of `shiplift/unix-socket` feature (#1251)",
        "pr_number": 1251,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "f9c486ce4abcd77cf61ddc7fe2fadb4aeae3b806",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 00:08:26 +0000",
        "deletions_count": 641,
        "description": "Update dependencies",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 644,
        "message": "chore(website): Update dependencies",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "0e26cfd64a421b3b8296697e5dfca8d8ab35df6c",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 00:15:02 +0000",
        "deletions_count": 13,
        "description": "Fix Github issues links",
        "files_count": 6,
        "group": "docs",
        "insertions_count": 13,
        "message": "docs: Fix Github issues links",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "9863f819c001827c400803b9fc0b1b71ea862244",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 10:42:39 +0000",
        "deletions_count": 7,
        "description": "Use the proper font in the configuration digram, ref #1234",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 7,
        "message": "chore(website): Use the proper font in the configuration digram, ref #1234",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "42eabf66dc5138f43c7310b067064beaf3f8c29d",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 11:10:47 +0000",
        "deletions_count": 5,
        "description": "Enable Algolia search",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(website): Enable Algolia search",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "9358c7a2d51ca259e38e49de5c2a46049146fead",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 11:30:11 +0000",
        "deletions_count": 5,
        "description": "Remove paginator from main doc content so that it is not included in search results",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 11,
        "message": "chore(website): Remove paginator from main doc content so that it is not included in search results",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "8f18ad80302bf5975ad704271eb2c8d986b1c7d0",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 12:20:05 +0000",
        "deletions_count": 9,
        "description": "Fix search field styling",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 42,
        "message": "chore(website): Fix search field styling",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "d8fef3c66ce2072c003ba30704276e51c5267dc4",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 12:25:24 +0000",
        "deletions_count": 4,
        "description": "Move main links in header to the left",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 4,
        "message": "chore(website): Move main links in header to the left",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "5492ae39c48d67e22fb983b9e55fa1cf5ee09dae",
        "type": "chore"
      },
      {
        "author": "James Sewell",
        "breaking_change": false,
        "date": "2019-11-26 05:38:57 +0000",
        "deletions_count": 17,
        "description": "Add JSON encoding option",
        "files_count": 6,
        "group": "enhancement",
        "insertions_count": 102,
        "message": "enhancement(http sink): Add JSON encoding option (#1174)",
        "pr_number": 1174,
        "scope": {
          "category": "sink",
          "component_name": "http",
          "component_type": "sink",
          "name": "http sink"
        },
        "sha": "357bdbbe9bf142eaf028a46e016e7b37e73a6e88",
        "type": "enhancement"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-25 14:38:10 +0000",
        "deletions_count": 61,
        "description": "Reference exact latest version instead of \"latest\" in download URLs",
        "files_count": 7,
        "group": "docs",
        "insertions_count": 153,
        "message": "docs: Reference exact latest version instead of \"latest\" in download URLs (#1254)",
        "pr_number": 1254,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "969a426e0f9826e5bebf45ffb87fe7b2f785e7e7",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 14:38:34 +0000",
        "deletions_count": 11,
        "description": "Fix search bar styling on mobile",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 24,
        "message": "chore(website): Fix search bar styling on mobile",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "b29e4e309b9a13eff12f46cf00e21a76090e46fd",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 14:52:15 +0000",
        "deletions_count": 101,
        "description": "Add auto-generated comments to files that are auto-generated, closes #1256",
        "files_count": 114,
        "group": "docs",
        "insertions_count": 655,
        "message": "docs: Add auto-generated comments to files that are auto-generated, closes #1256",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "ea81323033974a347bca458e5ab7e446b24228a3",
        "type": "docs"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-11-25 14:27:35 +0000",
        "deletions_count": 6,
        "description": "Sleep to avoid split reads",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 16,
        "message": "fix(file source): Sleep to avoid split reads (#1236)",
        "pr_number": 1236,
        "scope": {
          "category": "source",
          "component_name": "file",
          "component_type": "source",
          "name": "file source"
        },
        "sha": "26333d9cf00bb5e44ae73aa17a7cab5583dc7d22",
        "type": "fix"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-25 15:49:57 +0000",
        "deletions_count": 0,
        "description": "Add CODEOWNERS file",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 71,
        "message": "chore(operations): Add CODEOWNERS file (#1248)",
        "pr_number": 1248,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "9b7fdca9f9f0d5818afbd821210f9f2c17ccc564",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-25 21:56:15 +0000",
        "deletions_count": 79,
        "description": "Add `test` sub-command",
        "files_count": 38,
        "group": "feat",
        "insertions_count": 2446,
        "message": "feat(cli): Add `test` sub-command (#1220)",
        "pr_number": 1220,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "a9fbcb3ddbb3303f981257be064a995db59b7dbb",
        "type": "feat"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-25 22:43:40 +0000",
        "deletions_count": 0,
        "description": "Re-generate unit test spec",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 8,
        "message": "docs: Re-generate unit test spec",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "3e92c1eac7a44b0661f25b452a112e5024edf7b3",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-25 19:44:24 +0000",
        "deletions_count": 8,
        "description": "Add hash links to all headings",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 20,
        "message": "chore(website): Add hash links to all headings",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "a282db6df013b89d84694e68ecde38c4d544c1ba",
        "type": "chore"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": true,
        "date": "2019-11-26 12:24:33 +0000",
        "deletions_count": 1036,
        "description": "Reorganise metric model",
        "files_count": 16,
        "group": "breaking change",
        "insertions_count": 1389,
        "message": "enhancement(metric data model)!: Reorganise metric model (#1217)",
        "pr_number": 1217,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "metric data model"
        },
        "sha": "aed6f1bf1cb0d3d10b360e16bd118665a49c4ea5",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-26 15:24:00 +0000",
        "deletions_count": 0,
        "description": "Turn \"executable\" bit off for some of docs files",
        "files_count": 21,
        "group": "docs",
        "insertions_count": 0,
        "message": "docs: Turn \"executable\" bit off for some of docs files",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "df3e70980bfc9f6cde60516df482949fd0bc592b",
        "type": "docs"
      },
      {
        "author": "Kruno Tomola Fabro",
        "breaking_change": false,
        "date": "2019-11-26 16:35:36 +0000",
        "deletions_count": 298,
        "description": "Enrich events with metadata",
        "files_count": 39,
        "group": "enhancement",
        "insertions_count": 505,
        "message": "enhancement(docker source): Enrich events with metadata (#1149)",
        "pr_number": 1149,
        "scope": {
          "category": "source",
          "component_name": "docker",
          "component_type": "source",
          "name": "docker source"
        },
        "sha": "f20fc4ad3ea88d112d84be58eb51b4a5e85df21f",
        "type": "enhancement"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-26 11:01:48 +0000",
        "deletions_count": 2,
        "description": "Testing documentation touchups",
        "files_count": 4,
        "group": "docs",
        "insertions_count": 718,
        "message": "docs: Testing documentation touchups",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "f5cfdfe2fb25703ea308992c3d106b5c4b3b7af1",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-26 11:17:52 +0000",
        "deletions_count": 177,
        "description": "Fix examples syntax and parsing",
        "files_count": 19,
        "group": "docs",
        "insertions_count": 198,
        "message": "docs: Fix examples syntax and parsing",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "c86b23818345136ea0bf911d92426440387b1620",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-26 11:34:17 +0000",
        "deletions_count": 12,
        "description": "Clarify guarantees language to be feature specific not component specific",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 12,
        "message": "docs: Clarify guarantees language to be feature specific not component specific",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8fae3d0a5524f0172a97a1235c13305f660bc07f",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-26 11:46:58 +0000",
        "deletions_count": 9,
        "description": "Fix docker source config examples",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 8,
        "message": "docs: Fix docker source config examples",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "57434aa05893d89300cee34f7aa2be7c6be7405b",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-26 19:15:14 +0000",
        "deletions_count": 43,
        "description": "Fix sorting in make generate",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 35,
        "message": "chore(operations): Fix sorting in make generate",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "18da561ba25843b13ce013f5a2052dfbff877b2b",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-27 12:23:58 +0000",
        "deletions_count": 2,
        "description": "Add timeouts to crash tests",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 14,
        "message": "chore(testing): Add timeouts to crash tests (#1265)",
        "pr_number": 1265,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "3db6403a24c16a36ba3367dedff006c9c9924626",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-27 17:03:31 +0000",
        "deletions_count": 1,
        "description": "Run `x86_64-pc-windows-msvc` tests in release mode",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(testing): Run `x86_64-pc-windows-msvc` tests in release mode (#1269)",
        "pr_number": 1269,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "df2b5d8016f27e868e0bb2a6feaf8bd99caaf64f",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 10:22:21 +0000",
        "deletions_count": 41,
        "description": "Move env vars to reference section",
        "files_count": 11,
        "group": "docs",
        "insertions_count": 204,
        "message": "docs: Move env vars to reference section",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "c9f96ffaef533272103a167a5900edad1ed5946c",
        "type": "docs"
      },
      {
        "author": "Kruno Tomola Fabro",
        "breaking_change": false,
        "date": "2019-11-27 19:19:04 +0000",
        "deletions_count": 3,
        "description": "Custom DNS resolution",
        "files_count": 11,
        "group": "feat",
        "insertions_count": 733,
        "message": "feat(networking): Custom DNS resolution (#1118)",
        "pr_number": 1118,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "networking"
        },
        "sha": "77e582b526680a22ea4da616cbfdb3b0ad281097",
        "type": "feat"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 13:44:27 +0000",
        "deletions_count": 1697,
        "description": "Add env_vars key to all components",
        "files_count": 109,
        "group": "docs",
        "insertions_count": 3752,
        "message": "docs: Add env_vars key to all components",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "b255a52a6b53bcc1a9361ae746dde2c5d5fb9132",
        "type": "docs"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-27 18:41:49 +0000",
        "deletions_count": 616,
        "description": "Fix rate_limit and retry option names",
        "files_count": 20,
        "group": "docs",
        "insertions_count": 625,
        "message": "docs: Fix rate_limit and retry option names (#1270)",
        "pr_number": 1270,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8fac7296e4c17969c08841a58ce7b64f2ede5331",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 18:51:15 +0000",
        "deletions_count": 832,
        "description": "Fix variable field names",
        "files_count": 25,
        "group": "docs",
        "insertions_count": 79,
        "message": "docs: Fix variable field names",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "0a06803a89aa3ca570edf72834abac52db94a0b8",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 19:10:37 +0000",
        "deletions_count": 72,
        "description": "Fix variable field names",
        "files_count": 26,
        "group": "docs",
        "insertions_count": 95,
        "message": "docs: Fix variable field names",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "e50767b1560288cb862bf9f933a4cc92e7b329a6",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 19:38:05 +0000",
        "deletions_count": 2210,
        "description": "Fix config examples category name",
        "files_count": 46,
        "group": "docs",
        "insertions_count": 894,
        "message": "docs: Fix config examples category name",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "79f28aa15f26d73175467fb621ed87bf34240991",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-27 19:52:42 +0000",
        "deletions_count": 70,
        "description": "Fix example categories",
        "files_count": 24,
        "group": "docs",
        "insertions_count": 53,
        "message": "docs: Fix example categories",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "ae90038afb5d89eb080bd7c760ce3a4f1c67f219",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-28 10:32:52 +0000",
        "deletions_count": 274,
        "description": "Build .deb packages for all musl targets",
        "files_count": 17,
        "group": "chore",
        "insertions_count": 500,
        "message": "chore(operations): Build .deb packages for all musl targets (#1247)",
        "pr_number": 1247,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "b3554b16fa333727e21c8eaae87df4533e217c96",
        "type": "chore"
      },
      {
        "author": "Dan Palmer",
        "breaking_change": false,
        "date": "2019-11-28 15:43:22 +0000",
        "deletions_count": 1,
        "description": "Typo",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 1,
        "message": "docs: Typo (#1273)",
        "pr_number": 1273,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "10de21ba24814324547d53553ed098742279f935",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-28 10:51:03 +0000",
        "deletions_count": 1,
        "description": "Remove console.log",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 0,
        "message": "docs: Remove console.log",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "9c531ca1e734234e187d82b76912bf5dfa188742",
        "type": "docs"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-11-29 15:29:25 +0000",
        "deletions_count": 0,
        "description": "Add a unit test guide",
        "files_count": 6,
        "group": "docs",
        "insertions_count": 253,
        "message": "docs: Add a unit test guide (#1278)",
        "pr_number": 1278,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "c815d27773da3acd0272ef009270f772a3103791",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-29 12:01:14 +0000",
        "deletions_count": 23,
        "description": "Add topology section",
        "files_count": 6,
        "group": "chore",
        "insertions_count": 90,
        "message": "chore(website): Add topology section",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "7b5a7f322bffdbd7638791e32effa848deb1fdea",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-29 13:59:31 +0000",
        "deletions_count": 3,
        "description": "Default to centralized topology",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 3,
        "message": "chore(website): Default to centralized topology",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "ecdb56f5f49920353e5696e936f2d711d6881bbd",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-11-29 14:21:42 +0000",
        "deletions_count": 13,
        "description": "Fix rounded tabs",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 33,
        "message": "chore(website): Fix rounded tabs",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "1dc6e303079bf6a9bb9802fe108e77edf0b0fd83",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 00:15:17 +0000",
        "deletions_count": 1,
        "description": "Increase CI output timeout",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 8,
        "message": "chore(operations): Increase CI output timeout (#1272)",
        "pr_number": 1272,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "4e98b8321cd334d780a5388bd848d83cb677003c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 00:37:24 +0000",
        "deletions_count": 24,
        "description": "Delete unused OpenSSL patch",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 0,
        "message": "chore(operations): Delete unused OpenSSL patch (#1282)",
        "pr_number": 1282,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "7dd271e9102d2a2eb2016f8d735c8d9710966210",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-29 22:11:41 +0000",
        "deletions_count": 1,
        "description": "Run nightly builds at 12am UTC",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Run nightly builds at 12am UTC",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "32e5bfc2ff07ce0dddf817d5b64a2b04cc40f9ab",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 01:14:25 +0000",
        "deletions_count": 5,
        "description": "Set up redirects for x86_64-unknown-linux-gnu archives",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 23,
        "message": "chore(operations): Set up redirects for x86_64-unknown-linux-gnu archives (#1284)",
        "pr_number": 1284,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "62992492de9c21e8a59464696b2ba226c50b82f0",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 01:42:23 +0000",
        "deletions_count": 122,
        "description": "Build multi-arch Docker images",
        "files_count": 9,
        "group": "chore",
        "insertions_count": 151,
        "message": "chore(operations): Build multi-arch Docker images (#1279)",
        "pr_number": 1279,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "5fa10916882cd07ee6c6726be10227b321f5880c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 02:06:35 +0000",
        "deletions_count": 11,
        "description": "Use `sidebar_label` as subpage title if possible",
        "files_count": 5,
        "group": "chore",
        "insertions_count": 17,
        "message": "chore(website): Use `sidebar_label` as subpage title if possible (#1283)",
        "pr_number": 1283,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "5c6942f8e52971ec3eb95750d2a79574cb0c12bd",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 02:06:47 +0000",
        "deletions_count": 8,
        "description": "Simplify platform names in \"downloads\" section",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 8,
        "message": "chore(website): Simplify platform names in \"downloads\" section (#1285)",
        "pr_number": 1285,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "4058ef356271a8276ddd6b1f41933d25ddd585a6",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 10:13:42 +0000",
        "deletions_count": 2,
        "description": "Run nightly builds at 11am UTC",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(operations): Run nightly builds at 11am UTC",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "42c2a1f75e639ff29da5419cff29848fa3163d01",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 13:15:43 +0000",
        "deletions_count": 2,
        "description": "Remove extra `setup_remote_docker` step from `relase-docker`",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 0,
        "message": "fix(operations): Remove extra `setup_remote_docker` step from `relase-docker` (#1287)",
        "pr_number": 1287,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "8f271aee3b9873b10a68ab5c747c4e895347acca",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 13:15:56 +0000",
        "deletions_count": 1,
        "description": "Fix S3 release verification",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(operations): Fix S3 release verification (#1286)",
        "pr_number": 1286,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "643716654c9049e18c057d9e88de4e78f566d983",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-11-30 18:26:36 +0000",
        "deletions_count": 22,
        "description": "Upgrade Docker on the step in which it is used",
        "files_count": 3,
        "group": "fix",
        "insertions_count": 22,
        "message": "fix(operations): Upgrade Docker on the step in which it is used (#1288)",
        "pr_number": 1288,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "08a297961a767d798ebb244a10baf05b318272e7",
        "type": "fix"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-11-30 16:14:02 +0000",
        "deletions_count": 618,
        "description": "Cleanup installation docs",
        "files_count": 32,
        "group": "docs",
        "insertions_count": 783,
        "message": "docs: Cleanup installation docs (#1289)",
        "pr_number": 1289,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "70965d8e6d0c0d850faa86fb674987a107df9b93",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-01 11:21:05 +0000",
        "deletions_count": 229,
        "description": "Update to docaurus 2.0.0-alpha.37",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 242,
        "message": "chore(website): Update to docaurus 2.0.0-alpha.37",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "469671dc457f867cee8bab247b6529026e7ae4ca",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-01 11:39:36 +0000",
        "deletions_count": 10,
        "description": "Group downloads by os",
        "files_count": 8,
        "group": "chore",
        "insertions_count": 62,
        "message": "chore(website): Group downloads by os",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "06a864b106bc2233c5d5a8ba78f045def8a937f6",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-01 13:15:28 +0000",
        "deletions_count": 25,
        "description": "Rename raspberry-pi to raspbian",
        "files_count": 10,
        "group": "docs",
        "insertions_count": 44,
        "message": "docs: Rename raspberry-pi to raspbian",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "8ee38009da9bcd41444e9cf2ed48683aa1870a1a",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-01 13:29:57 +0000",
        "deletions_count": 1,
        "description": "Fix responsive styling on homepage",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 9,
        "message": "chore(website): Fix responsive styling on homepage",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "73dc9d55803733c460f42ce38e09b8c7c8344680",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-01 23:44:37 +0000",
        "deletions_count": 5,
        "description": "Fix accessing custom front-matter in docs",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 13,
        "message": "chore(website): Fix accessing custom front-matter in docs",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "3fc6196a6b6e2df7c76e9d5924377a2054dcb5e2",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-02 09:58:25 +0000",
        "deletions_count": 62,
        "description": "Build RPM packages for ARM",
        "files_count": 5,
        "group": "chore",
        "insertions_count": 220,
        "message": "chore(operations): Build RPM packages for ARM (#1292)",
        "pr_number": 1292,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "a6668b0c1db009b537c989ef95d8c4e616440cb9",
        "type": "chore"
      },
      {
        "author": "Bruce Guenter",
        "breaking_change": false,
        "date": "2019-12-02 08:27:53 +0000",
        "deletions_count": 338,
        "description": "Refactor the sinks' request_* configuration",
        "files_count": 12,
        "group": "enhancement",
        "insertions_count": 321,
        "message": "enhancement(config): Refactor the sinks' request_* configuration (#1187)",
        "pr_number": 1187,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "config"
        },
        "sha": "62f9db5ba46a0824ed0e979743bc8aaec8e05010",
        "type": "enhancement"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-02 19:23:02 +0000",
        "deletions_count": 2,
        "description": "Fix Raspbian id capitalization",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Fix Raspbian id capitalization (#1295)",
        "pr_number": 1295,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "cbac5010444357dae078b299991304ca8055889c",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-02 22:44:56 +0000",
        "deletions_count": 0,
        "description": "Run `package-rpm*` jobs explicitly",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 3,
        "message": "fix(operations): Run `package-rpm*` jobs explicitly (#1298)",
        "pr_number": 1298,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "a0eec9935a8a2d0409e23c6cb23cba807b16a7df",
        "type": "fix"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-03 11:28:27 +0000",
        "deletions_count": 16,
        "description": "Fix section links",
        "files_count": 9,
        "group": "docs",
        "insertions_count": 24,
        "message": "docs: Fix section links",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "5ae3036f0a0de24aeeb92135621c877428bcfa02",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-02 11:36:52 +0000",
        "deletions_count": 1,
        "description": "Fix browse downloads link",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(website): Fix browse downloads link",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "1f52116c3c40dcc439bd8f32c9cdf2a0a3b197d7",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 12:29:30 +0000",
        "deletions_count": 11,
        "description": "Add slugify method to mimic Docusaurus hashing logic for links",
        "files_count": 7,
        "group": "chore",
        "insertions_count": 23,
        "message": "chore(website): Add slugify method to mimic Docusaurus hashing logic for links",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "bd865b06bc2ff68edb3a131a574572b88fcc8b87",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 12:33:09 +0000",
        "deletions_count": 20,
        "description": "Fix buffers and batches hash link",
        "files_count": 10,
        "group": "chore",
        "insertions_count": 20,
        "message": "chore(website): Fix buffers and batches hash link",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "9d38c48a10b9d3deb8d35b6e97002cab4a03b885",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 13:30:43 +0000",
        "deletions_count": 2,
        "description": "Use the Rust regex tester, closes #634",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Use the Rust regex tester, closes #634",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "0c0f07265ad4020d68116c14113d917499ca862f",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 13:35:39 +0000",
        "deletions_count": 16,
        "description": "Fix example regex",
        "files_count": 6,
        "group": "chore",
        "insertions_count": 16,
        "message": "chore(website): Fix example regex",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "d962fa60fd1e71cd2c9c02fc4e1ead2fd0a5086c",
        "type": "chore"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-03 15:55:03 +0000",
        "deletions_count": 35,
        "description": "Pass `TaskExecutor` to transform",
        "files_count": 25,
        "group": "chore",
        "insertions_count": 67,
        "message": "chore(topology): Pass `TaskExecutor` to transform (#1144)",
        "pr_number": 1144,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "topology"
        },
        "sha": "17a27b315b4e65f687adb0d64d2b6c5cf8890a95",
        "type": "chore"
      },
      {
        "author": "Binary Logic",
        "breaking_change": false,
        "date": "2019-12-03 17:28:50 +0000",
        "deletions_count": 223,
        "description": "Add community page with mailing list",
        "files_count": 13,
        "group": "chore",
        "insertions_count": 271,
        "message": "chore(website): Add community page with mailing list (#1309)",
        "pr_number": 1309,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "cf95723d77ba4bd3fa819dd45fa7676bd1a7d19d",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 17:45:00 +0000",
        "deletions_count": 2,
        "description": "Responsive styling for community page",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 14,
        "message": "chore(wensite): Responsive styling for community page",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "wensite"
        },
        "sha": "c912f16f1cbd924db1e800498dbfb240e9211212",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 18:04:41 +0000",
        "deletions_count": 5,
        "description": "Fix slide out main nav menu link labels",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 7,
        "message": "chore(website): Fix slide out main nav menu link labels",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "4c1718431e887c9a9f58392428cde6c2a33e5070",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 18:53:47 +0000",
        "deletions_count": 14,
        "description": "Re-add components list",
        "files_count": 5,
        "group": "chore",
        "insertions_count": 207,
        "message": "chore(website): Re-add components list",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "14ebf42842d90f937df7efa88f7acea1bb1859e8",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 21:44:27 +0000",
        "deletions_count": 29,
        "description": "Use ${ENV_VAR} syntax in relavant examples",
        "files_count": 9,
        "group": "docs",
        "insertions_count": 33,
        "message": "docs: Use ${ENV_VAR} syntax in relavant examples",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "6e60b2fab0de568ef47c5afdd606a60c3069531d",
        "type": "docs"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-12-04 12:21:43 +0000",
        "deletions_count": 9,
        "description": "Performance optimisations in metric buffer",
        "files_count": 2,
        "group": "perf",
        "insertions_count": 165,
        "message": "perf(metric data model): Performance optimisations in metric buffer (#1290)",
        "pr_number": 1290,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "metric data model"
        },
        "sha": "fcf6356f11ac7d80a5c378aeceabd6cf72168ef1",
        "type": "perf"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-03 23:24:55 +0000",
        "deletions_count": 5,
        "description": "Fix nav width",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 10,
        "message": "chore(website): Fix nav width",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "690d798e8cc4d08457b5ad3dd3fcee4da7fea4b3",
        "type": "chore"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-04 10:56:01 +0000",
        "deletions_count": 6,
        "description": "Update README with new links",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 8,
        "message": "docs: Update README with new links",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "53d2a9ca0ff85c8d39cf9b312265c859f079c170",
        "type": "docs"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-04 13:23:44 +0000",
        "deletions_count": 113,
        "description": "Add `SinkContext` to `SinkConfig`",
        "files_count": 23,
        "group": "chore",
        "insertions_count": 146,
        "message": "chore(topology): Add `SinkContext` to `SinkConfig` (#1306)",
        "pr_number": 1306,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "topology"
        },
        "sha": "00e21e83c54d2ca5e0b50b3b96a3390e761bf2dd",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-04 19:18:34 +0000",
        "deletions_count": 9,
        "description": "Initial `new_relic_logs` sink implementation",
        "files_count": 13,
        "group": "feat",
        "insertions_count": 1166,
        "message": "feat(new sink): Initial `new_relic_logs` sink implementation (#1303)",
        "pr_number": 1303,
        "scope": {
          "category": "sink",
          "component_name": null,
          "component_type": "sink",
          "name": "new sink"
        },
        "sha": "52e4f176f62c305a6d0adcf6fa1f5b08bd2466dc",
        "type": "feat"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-04 19:48:24 +0000",
        "deletions_count": 11,
        "description": "Fix NR build signature",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 12,
        "message": "chore: Fix NR build signature",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "4c1d8ceaef63fc9f73e5e568773bf569f6c2f460",
        "type": "chore"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-04 15:29:04 +0000",
        "deletions_count": 182,
        "description": "Add map to ServiceBuilder and s3",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 346,
        "message": "chore: Add map to ServiceBuilder and s3 (#1189)",
        "pr_number": 1189,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "772672e65920de3c0f13fa5b86c9c428b2d3fbfb",
        "type": "chore"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": true,
        "date": "2019-12-04 22:34:43 +0000",
        "deletions_count": 3,
        "description": "Rename `datadog` sink to `datadog_metrics`",
        "files_count": 1,
        "group": "breaking change",
        "insertions_count": 3,
        "message": "fix(datadog_metrics sink)!: Rename `datadog` sink to `datadog_metrics` (#1314)",
        "pr_number": 1314,
        "scope": {
          "category": "sink",
          "component_name": "datadog_metrics",
          "component_type": "sink",
          "name": "datadog_metrics sink"
        },
        "sha": "59fd318f227524a84a7520bbae004d2c75156365",
        "type": "fix"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-04 15:39:15 +0000",
        "deletions_count": 166,
        "description": "Sync with new toggle changes",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 2,
        "message": "chore(website): Sync with new toggle changes",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "e76083548a2d46664acd67a8e40f1835614d94c5",
        "type": "chore"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-12-05 09:31:01 +0000",
        "deletions_count": 0,
        "description": "Send aggregated distributions to Datadog",
        "files_count": 1,
        "group": "enhancement",
        "insertions_count": 231,
        "message": "enhancement(datadog_metrics sink): Send aggregated distributions to Datadog (#1263)",
        "pr_number": 1263,
        "scope": {
          "category": "sink",
          "component_name": "datadog_metrics",
          "component_type": "sink",
          "name": "datadog_metrics sink"
        },
        "sha": "5822ee199bafbc2558491d5ba9682b8f10ed95d0",
        "type": "enhancement"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-05 13:28:26 +0000",
        "deletions_count": 7,
        "description": "Test & validate subcommands without args target default path",
        "files_count": 3,
        "group": "enhancement",
        "insertions_count": 32,
        "message": "enhancement(cli): Test & validate subcommands without args target default path (#1313)",
        "pr_number": 1313,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "cli"
        },
        "sha": "e776d3a404810935810983caf888aa86138b448b",
        "type": "enhancement"
      },
      {
        "author": "Alexey Suslov",
        "breaking_change": false,
        "date": "2019-12-05 17:51:10 +0000",
        "deletions_count": 1,
        "description": "Fix statsd binding to loopback only",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(statsd sink): Fix statsd binding to loopback only (#1316)",
        "pr_number": 1316,
        "scope": {
          "category": "sink",
          "component_name": "statsd",
          "component_type": "sink",
          "name": "statsd sink"
        },
        "sha": "58d6e976cf81f2175e7fd6cc6d4c85c9e2bc88eb",
        "type": "fix"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-06 14:38:03 +0000",
        "deletions_count": 5,
        "description": "Fix multiple sources test",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 5,
        "message": "chore(testing): Fix multiple sources test (#1322)",
        "pr_number": 1322,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "324012b74c8879b1185ace3c5c36d9170222597e",
        "type": "chore"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-12-06 15:54:01 +0000",
        "deletions_count": 2,
        "description": "Document `drop_field`",
        "files_count": 3,
        "group": "docs",
        "insertions_count": 42,
        "message": "docs(json_parser transform): Document `drop_field` (#1323)",
        "pr_number": 1323,
        "scope": {
          "category": "transform",
          "component_name": "json_parser",
          "component_type": "transform",
          "name": "json_parser transform"
        },
        "sha": "dc21766356a422e694287bff1b70fde8a49e74af",
        "type": "docs"
      },
      {
        "author": "Ben Johnson",
        "breaking_change": false,
        "date": "2019-12-07 10:53:05 +0000",
        "deletions_count": 207,
        "description": "Update to docusaurus 2.0.0-alpha.39",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 198,
        "message": "chore(website): Update to docusaurus 2.0.0-alpha.39",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "website"
        },
        "sha": "8d15fdd267df44ac9f5079e7b6a5a2bc122b9e1f",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-09 13:11:56 +0000",
        "deletions_count": 29,
        "description": "Add \"default-{musl,msvc}\" features",
        "files_count": 7,
        "group": "chore",
        "insertions_count": 93,
        "message": "chore(operations): Add \"default-{musl,msvc}\" features (#1331)",
        "pr_number": 1331,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "2c6982502c75409806da7d74a4cc019f2c60ed08",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-09 11:06:57 +0000",
        "deletions_count": 1,
        "description": "Fix validating environment title",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 1,
        "message": "docs: Fix validating environment title",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "fb7f1f5743e464294c62d11e1be0d26e309f2061",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-09 15:35:33 +0000",
        "deletions_count": 87,
        "description": "Use LLVM-9 from the distribution repository",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 31,
        "message": "chore(operations): Use LLVM-9 from the distribution repository (#1333)",
        "pr_number": 1333,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "8cb9ec9406315d87c10f297da115ced93c2418f1",
        "type": "chore"
      },
      {
        "author": "Kruno Tomola Fabro",
        "breaking_change": false,
        "date": "2019-12-09 13:26:38 +0000",
        "deletions_count": 44,
        "description": "Initial `splunk_hec` source implementation",
        "files_count": 7,
        "group": "feat",
        "insertions_count": 1142,
        "message": "feat(new source): Initial `splunk_hec` source implementation",
        "pr_number": null,
        "scope": {
          "category": "source",
          "component_name": null,
          "component_type": "source",
          "name": "new source"
        },
        "sha": "a68c9781a12cd35f2ee1cd7686320d1bd6e52c05",
        "type": "feat"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-09 17:00:16 +0000",
        "deletions_count": 63,
        "description": "Use LLVM from an archive instead of Git",
        "files_count": 3,
        "group": "chore",
        "insertions_count": 33,
        "message": "chore(operations): Use LLVM from an archive instead of Git (#1334)",
        "pr_number": 1334,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "9c53a5dd65c4711c58a5afede4a23c048c4bed4d",
        "type": "chore"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-09 10:57:26 +0000",
        "deletions_count": 7,
        "description": "Update `shiplift 0.6`",
        "files_count": 2,
        "group": "chore",
        "insertions_count": 7,
        "message": "chore(docker source): Update `shiplift 0.6` (#1335)",
        "pr_number": 1335,
        "scope": {
          "category": "source",
          "component_name": "docker",
          "component_type": "source",
          "name": "docker source"
        },
        "sha": "86abe53556fd7647717ddfecc21834f87adaa62b",
        "type": "chore"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-09 16:04:27 +0000",
        "deletions_count": 54,
        "description": "Rewrite getting started guide.",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 76,
        "message": "docs: Rewrite getting started guide. (#1332)",
        "pr_number": 1332,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "4b93936dc588438a3023a6d86075ca75a33921f3",
        "type": "docs"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-09 16:05:58 +0000",
        "deletions_count": 18,
        "description": "Update contribution guide for docs",
        "files_count": 2,
        "group": "docs",
        "insertions_count": 53,
        "message": "docs: Update contribution guide for docs",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "5461ff419b9587264bbce823af227e1a3007a578",
        "type": "docs"
      },
      {
        "author": "Lucio Franco",
        "breaking_change": false,
        "date": "2019-12-09 11:06:50 +0000",
        "deletions_count": 0,
        "description": "Add missing rate limited log",
        "files_count": 1,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(grok_parser transform): Add missing rate limited log (#1336)",
        "pr_number": 1336,
        "scope": {
          "category": "transform",
          "component_name": "grok_parser",
          "component_type": "transform",
          "name": "grok_parser transform"
        },
        "sha": "285b967ab228a94b4a140803cec38b71bb59ad14",
        "type": "fix"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-10 09:34:53 +0000",
        "deletions_count": 2,
        "description": "Edit getting started guide",
        "files_count": 1,
        "group": "docs",
        "insertions_count": 2,
        "message": "docs: Edit getting started guide",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "137c51de9122c32cbbfba983f3068b6df1d6a68e",
        "type": "docs"
      },
      {
        "author": "Ashley Jeffs",
        "breaking_change": false,
        "date": "2019-12-10 16:42:08 +0000",
        "deletions_count": 39,
        "description": "Fix unit test spec rendering",
        "files_count": 5,
        "group": "docs",
        "insertions_count": 43,
        "message": "docs: Fix unit test spec rendering",
        "pr_number": null,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "core"
        },
        "sha": "5c2c0af26554258d746051a5861ce9aaa869a8be",
        "type": "docs"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-11 17:09:12 +0000",
        "deletions_count": 44,
        "description": "Build `msi` package for Vector",
        "files_count": 23,
        "group": "chore",
        "insertions_count": 780,
        "message": "chore(operations): Build `msi` package for Vector (#1345)",
        "pr_number": 1345,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "da89fa9fd801ff6f87412fb78d686936115b241c",
        "type": "chore"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-12-11 15:56:33 +0000",
        "deletions_count": 16,
        "description": "Remove sleeps from topology tests",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 1,
        "message": "fix(testing): Remove sleeps from topology tests (#1346)",
        "pr_number": 1346,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "testing"
        },
        "sha": "8561d42eba3c5d30d57ab47c6454f19978c5ea4b",
        "type": "fix"
      },
      {
        "author": "Luke Steensen",
        "breaking_change": false,
        "date": "2019-12-11 16:30:27 +0000",
        "deletions_count": 21,
        "description": "Detect and read gzipped files",
        "files_count": 7,
        "group": "feat",
        "insertions_count": 127,
        "message": "feat(file source): Detect and read gzipped files (#1344)",
        "pr_number": 1344,
        "scope": {
          "category": "source",
          "component_name": "file",
          "component_type": "source",
          "name": "file source"
        },
        "sha": "8c991293ee2cd478fc639e96e6c27df794a0c5ec",
        "type": "feat"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-12 15:49:31 +0000",
        "deletions_count": 11,
        "description": "Put `etc` directory only to Linux archives",
        "files_count": 2,
        "group": "fix",
        "insertions_count": 11,
        "message": "fix(operations): Put `etc` directory only to Linux archives (#1352)",
        "pr_number": 1352,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "cbba6f180a583d4d7f236b64b77fdd6406bc6c63",
        "type": "fix"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-12 16:22:49 +0000",
        "deletions_count": 1,
        "description": "Allow passing features to `make build`",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Allow passing features to `make build` (#1356)",
        "pr_number": 1356,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "1f9b9cf6eddf27557bcaa6a1e1139da0137dcb4c",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-12 16:53:31 +0000",
        "deletions_count": 1,
        "description": "Compress release archives with `gzip -9`",
        "files_count": 1,
        "group": "chore",
        "insertions_count": 1,
        "message": "chore(operations): Compress release archives with `gzip -9` (#1294)",
        "pr_number": 1294,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "779d727fb49c907d6babbd8ed48e0db2cec14604",
        "type": "chore"
      },
      {
        "author": "Alexander Rodin",
        "breaking_change": false,
        "date": "2019-12-12 19:11:22 +0000",
        "deletions_count": 1,
        "description": "Add notices for OpenSSL to the license for binary distributions",
        "files_count": 4,
        "group": "chore",
        "insertions_count": 22,
        "message": "chore(operations): Add notices for OpenSSL to the license for binary distributions (#1351)",
        "pr_number": 1351,
        "scope": {
          "category": "core",
          "component_name": null,
          "component_type": null,
          "name": "operations"
        },
        "sha": "f8ad1b5a0edcf214865e4ba1133b3a0df1465905",
        "type": "chore"
      }
    ],
    "compare_url": "https://github.com/timberio/vector/compare/v0.5.0...v0.6.0",
    "date": "2019-12-09",
    "deletions_count": 9213,
    "insertions_count": 22141,
    "last_version": "0.5.0",
    "posts": [
      {
        "author_id": "ben",
        "date": "2019-11-19",
        "description": "Vector now supports ARM architectures on the Linux platform! These\narchitectures are widely used in embeded devices and recently started to get\ntraction on servers. To get started, you can follow the installation\ninstructions for your preferred method:",
        "id": "arm-support-on-linux",
        "path": "website/blog/2019-11-19-arm-support-on-linux.md",
        "permalink": "https://vector.dev/blog/arm-support-on-linux",
        "tags": [
          "type: announcement",
          "domain: platforms",
          "platform: arm"
        ],
        "title": "ARMv7 & ARM64 Support on Linux"
      },
      {
        "author_id": "ben",
        "date": "2019-11-21",
        "description": "We're excited to announce that Vector can now be installed on Windows!\nTo get started, check out the Windows installation instructions\nor head over to the releases section and download the\nappropriate Windows archive. Just like on Linux, installation on Windows is\nquick and easy. Let us know what you think!.",
        "id": "windows-support",
        "path": "website/blog/2019-11-21-windows-support.md",
        "permalink": "https://vector.dev/blog/windows-support",
        "tags": [
          "type: announcement",
          "domain: platforms",
          "platform: windows"
        ],
        "title": "Windows Support Is Here!"
      },
      {
        "author_id": "ashley",
        "date": "2019-11-25",
        "description": "Today we're excited to announce beta support for unit testing Vector\nconfigurations, allowing you to define tests directly within your Vector\nconfiguration file. These tests are used to assert the output from topologies of\ntransform components given certain input events, ensuring\nthat your configuration behavior does not regress; a very powerful feature for\nmission-critical production pipelines that are collaborated on.",
        "id": "unit-testing-vector-config-files",
        "path": "website/blog/2019-11-25-unit-testing-vector-config-files.md",
        "permalink": "https://vector.dev/blog/unit-testing-vector-config-files",
        "tags": [
          "type: announcement",
          "domain: config"
        ],
        "title": "Unit Testing Your Vector Config Files"
      }
    ],
    "type": "initial dev",
    "type_url": "https://semver.org/#spec-item-4",
    "upgrade_guides": [
      {
        "body": "<p>\nThe `file` and `console` sinks now require an explicit `encoding` option. The previous implicit nature was confusing and this should eliminate any suprises related to the output encoding format. Migration is easy:\n</p>\n\n<pre>\n [sinks.my_console_sink]\n   type = \"console\"\n+  encoding = \"json\" # or \"text\"\n\n\n [sinks.my_file_sink]\n   type = \"file\"\n+  encoding = \"json\" # or \"text\"\n</pre>\n",
        "commits": [

        ],
        "id": "encoding-guide",
        "title": "The `file` and `console` sinks now require `encoding`"
      },
      {
        "body": "<p>\nThe `datadog` sink was incorrectly named since we'll be adding future support for DataDog logs. Migrating is as simple as renaming your sink:\n</p>\n\n<pre>\n [sinks.my_sink]\n-  type = \"datadog\"\n+  type = \"datadog_metrics\"\n</pre>\n",
        "commits": [

        ],
        "id": "datadog-guide",
        "title": "The `datadog` sink has been renamed to `datadog_metrics`"
      }
    ],
    "version": "0.6.0"
  },
  "post_tags": [
    "type: announcement",
    "domain: platforms",
    "platform: arm",
    "platform: windows",
    "domain: config"
  ],
  "posts": [
    {
      "author_id": "luke",
      "date": "2019-06-28",
      "description": "Today we're very excited to open source the Vector project! Vector is a tool for building flexible and robust pipelines for your logs and metrics data. We're still in the early stages, but our goal with Vector is to dramatically simplify your observability infrastructure while making it easy to get more value from your data.",
      "id": "introducing-vector",
      "path": "website/blog/2019-06-28-introducing-vector.md",
      "permalink": "https://vector.dev/blog/introducing-vector",
      "tags": [
        "type: announcement"
      ],
      "title": "Introducing Vector"
    },
    {
      "author_id": "ben",
      "date": "2019-11-19",
      "description": "Vector now supports ARM architectures on the Linux platform! These\narchitectures are widely used in embeded devices and recently started to get\ntraction on servers. To get started, you can follow the installation\ninstructions for your preferred method:",
      "id": "arm-support-on-linux",
      "path": "website/blog/2019-11-19-arm-support-on-linux.md",
      "permalink": "https://vector.dev/blog/arm-support-on-linux",
      "tags": [
        "type: announcement",
        "domain: platforms",
        "platform: arm"
      ],
      "title": "ARMv7 & ARM64 Support on Linux"
    },
    {
      "author_id": "ben",
      "date": "2019-11-21",
      "description": "We're excited to announce that Vector can now be installed on Windows!\nTo get started, check out the Windows installation instructions\nor head over to the releases section and download the\nappropriate Windows archive. Just like on Linux, installation on Windows is\nquick and easy. Let us know what you think!.",
      "id": "windows-support",
      "path": "website/blog/2019-11-21-windows-support.md",
      "permalink": "https://vector.dev/blog/windows-support",
      "tags": [
        "type: announcement",
        "domain: platforms",
        "platform: windows"
      ],
      "title": "Windows Support Is Here!"
    },
    {
      "author_id": "ashley",
      "date": "2019-11-25",
      "description": "Today we're excited to announce beta support for unit testing Vector\nconfigurations, allowing you to define tests directly within your Vector\nconfiguration file. These tests are used to assert the output from topologies of\ntransform components given certain input events, ensuring\nthat your configuration behavior does not regress; a very powerful feature for\nmission-critical production pipelines that are collaborated on.",
      "id": "unit-testing-vector-config-files",
      "path": "website/blog/2019-11-25-unit-testing-vector-config-files.md",
      "permalink": "https://vector.dev/blog/unit-testing-vector-config-files",
      "tags": [
        "type: announcement",
        "domain: config"
      ],
      "title": "Unit Testing Your Vector Config Files"
    }
  ],
  "releases": {
    "0.4.0": {
      "commits": [
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-12 12:12:12 +0000",
          "deletions_count": 270,
          "description": "Add initial rework of rate limited logs",
          "files_count": 5,
          "group": "perf",
          "insertions_count": 300,
          "message": "perf(observability): Add initial rework of rate limited logs (#778)",
          "pr_number": 778,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "1357a3fa6b9acd0dd1d4b9e577969bf0594a5691",
          "type": "perf"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-12 10:38:59 +0000",
          "deletions_count": 1,
          "description": "Increase docker-release timeout",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Increase docker-release timeout (#858)",
          "pr_number": 858,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "bf81efdddf801232aa44ab76184e1368f1ce4f78",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-09-12 17:32:50 +0000",
          "deletions_count": 347,
          "description": "New `add_tags` transform",
          "files_count": 35,
          "group": "feat",
          "insertions_count": 1352,
          "message": "feat(new transform): New `add_tags` transform (#785)",
          "pr_number": 785,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "9705ae833c918189f786ac72c6f974102385911b",
          "type": "feat"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-09-12 17:32:50 +0000",
          "deletions_count": 347,
          "description": "New `remove_tags` transform",
          "files_count": 35,
          "group": "feat",
          "insertions_count": 1352,
          "message": "feat(new transform): New `remove_tags` transform (#785)",
          "pr_number": 785,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "9705ae833c918189f786ac72c6f974102385911b",
          "type": "feat"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-09-11 18:55:02 +0000",
          "deletions_count": 8,
          "description": "New `file` sink",
          "files_count": 22,
          "group": "feat",
          "insertions_count": 1355,
          "message": "feat(new sink): New `file` sink (#688)",
          "pr_number": 688,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "4cd5e539565732fd1289bc9f5ddba2897404f441",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-11 11:45:30 +0000",
          "deletions_count": 261,
          "description": "update stream-based diagram",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 206,
          "message": "docs: update stream-based diagram",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ee527daf254144bdbf78e8aeb87febfb61816bde",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-11 11:33:48 +0000",
          "deletions_count": 9,
          "description": "update roadmap link",
          "files_count": 8,
          "group": "docs",
          "insertions_count": 13,
          "message": "docs: update roadmap link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ff83f94362d841270c71abbcf415776d0b6e78c3",
          "type": "docs"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-11 09:39:43 +0000",
          "deletions_count": 70,
          "description": "favor older files and allow configuring greedier reads",
          "files_count": 9,
          "group": "enhancement",
          "insertions_count": 393,
          "message": "enhancement(file source): favor older files and allow configuring greedier reads (#810)",
          "pr_number": 810,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "e331a886afbf7ce5db4296321449a16bc1ed41e1",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-11 09:36:09 +0000",
          "deletions_count": 6,
          "description": "clarify sampler transform rate documentation",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 16,
          "message": "docs: clarify sampler transform rate documentation",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c3cbc55477c477d7a7b3ff7cd7b216b412ed1c14",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:37:23 +0000",
          "deletions_count": 6,
          "description": "gitbook straight doesnt escape |, so we will have to live with \\|",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: gitbook straight doesnt escape |, so we will have to live with \\|",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a7d237573f4b60235b21973a9c3f5c0b9362e03f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:36:11 +0000",
          "deletions_count": 6,
          "description": "use &#124; for the pipe character...gitbook",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: use &#124; for the pipe character...gitbook",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "cd4637f154ec1b4e41918516d5ae0bac62bd63e6",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:35:19 +0000",
          "deletions_count": 0,
          "description": "add SUMMARY.md.erb template",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 90,
          "message": "docs: add SUMMARY.md.erb template",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2f5c8898a2867701242be3691cc9a5f5ec30ba2a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:34:57 +0000",
          "deletions_count": 6,
          "description": "use literals when escaping |",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: use literals when escaping |",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "049d94e3ba49a869dd30717ef09cf6e8854e1853",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:33:20 +0000",
          "deletions_count": 6,
          "description": "gitbook doesnt like double escaped | characters",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: gitbook doesnt like double escaped | characters",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ae507430d4e9ff704803f31ef5367319bbfb6497",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 15:30:34 +0000",
          "deletions_count": 3,
          "description": "fix file source table escaping",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: fix file source table escaping",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7b25d9170d327c6e2078cad837259b1aad7e5e6e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 12:09:51 +0000",
          "deletions_count": 5,
          "description": "kafka souce it an at_least_once delivery guarantee",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: kafka souce it an at_least_once delivery guarantee",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1f488eec08cf518e7199adb05b81d38f3cbb0995",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 12:08:39 +0000",
          "deletions_count": 10,
          "description": "add note about kafka topic pattern matching, ref https://github.com/timberio/vector/issues/819",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 16,
          "message": "docs: add note about kafka topic pattern matching, ref https://github.com/timberio/vector/issues/819",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "538b1e789330589cc970166c21daa87b629d3592",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 12:05:27 +0000",
          "deletions_count": 2,
          "description": "fix path detection",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: fix path detection",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "81778163c916b1a94756b19c7313c904fe666721",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 10:55:33 +0000",
          "deletions_count": 26,
          "description": "fix sink links",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 26,
          "message": "docs: fix sink links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "dbfbf081fcfba499c3cd152b5e2f1b84517f694a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 10:50:48 +0000",
          "deletions_count": 35,
          "description": "generate SUMMARY.md to ensure new components show up in the side bar",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 69,
          "message": "docs: generate SUMMARY.md to ensure new components show up in the side bar",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "de5940ed2592a59a3f86bb5c35b0f019304331d2",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-10 10:41:48 +0000",
          "deletions_count": 1,
          "description": "add kafka source to summary.md",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 11,
          "message": "docs: add kafka source to summary.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "33c48a6482ef7b7a5bd11bb4d867a4f97908d93e",
          "type": "docs"
        },
        {
          "author": "Matthias Endler",
          "breaking_change": false,
          "date": "2019-09-10 16:36:15 +0000",
          "deletions_count": 1,
          "description": "Add bundler to requirements",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 7,
          "message": "docs: Add bundler to requirements (#845)",
          "pr_number": 845,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e4f5b2630ad9e537b3e576ab73f468855b0f46eb",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-09 10:22:35 +0000",
          "deletions_count": 77,
          "description": "Add checkpointing support",
          "files_count": 8,
          "group": "enhancement",
          "insertions_count": 342,
          "message": "enhancement(journald source): Add checkpointing support (#816)",
          "pr_number": 816,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "94cadda25e552b0eb82e58ea85eda10e6b787197",
          "type": "enhancement"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-05 14:12:25 +0000",
          "deletions_count": 5,
          "description": "Make the headers and query tables optional.",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 11,
          "message": "fix(elasticsearch sink): Make the headers and query tables optional. (#831)",
          "pr_number": 831,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "c83e7e0c7c3a994c817c4a8ae0ac41c3a6c1818d",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-05 14:27:46 +0000",
          "deletions_count": 17,
          "description": "Fix docker nightly builds",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 11,
          "message": "fix(operations): Fix docker nightly builds (#830)",
          "pr_number": 830,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c8736ea623df8ed17cd04478785522459bd4c105",
          "type": "fix"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-04 17:16:50 +0000",
          "deletions_count": 16,
          "description": "allow aggregating multiple lines into one event",
          "files_count": 5,
          "group": "enhancement",
          "insertions_count": 285,
          "message": "enhancement(file source): allow aggregating multiple lines into one event (#809)",
          "pr_number": 809,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "e9b5988bd26c550c2308ba65798872634fe6a4f8",
          "type": "enhancement"
        },
        {
          "author": "Bittrance",
          "breaking_change": false,
          "date": "2019-09-04 21:46:24 +0000",
          "deletions_count": 830,
          "description": "Topology test refactoring",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 677,
          "message": "chore(testing): Topology test refactoring (#748)",
          "pr_number": 748,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "1b8f2bb9f2b2ec60ba02a0be6f449be19950f8eb",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-03 15:37:02 +0000",
          "deletions_count": 28,
          "description": "Add support for unverified HTTPS",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 124,
          "message": "enhancement(http sink): Add support for unverified HTTPS (#815)",
          "pr_number": 815,
          "scope": {
            "category": "sink",
            "component_name": "http",
            "component_type": "sink",
            "name": "http sink"
          },
          "sha": "1dac7d8c3e399d750891bbe74fb0580c179e4138",
          "type": "enhancement"
        },
        {
          "author": "Markus Holtermann",
          "breaking_change": false,
          "date": "2019-09-03 23:27:30 +0000",
          "deletions_count": 0,
          "description": "Add missing clickhouse integration test feature",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(testing): Add missing clickhouse integration test feature (#818)",
          "pr_number": 818,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "928e37f4de188134565e05e04943e04dcc95e6a0",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-03 14:12:43 +0000",
          "deletions_count": 53,
          "description": "Update to `tokio-udp` v0.1.5",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 21,
          "message": "chore(udp source): Update to `tokio-udp` v0.1.5 (#817)",
          "pr_number": 817,
          "scope": {
            "category": "source",
            "component_name": "udp",
            "component_type": "source",
            "name": "udp source"
          },
          "sha": "712a7219aeb2e8f4fe87efdbcf11493dc0cb9d97",
          "type": "chore"
        },
        {
          "author": "ktff",
          "breaking_change": false,
          "date": "2019-08-29 21:18:36 +0000",
          "deletions_count": 50,
          "description": "Use new UdpFramed",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 55,
          "message": "chore(udp source): Use new UdpFramed (#808)",
          "pr_number": 808,
          "scope": {
            "category": "source",
            "component_name": "udp",
            "component_type": "source",
            "name": "udp source"
          },
          "sha": "1c6dd7b0b07be08f3c8b794d58d9c0f32c07454f",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-27 21:13:06 +0000",
          "deletions_count": 121,
          "description": "make fingerprinting strategy configurable",
          "files_count": 7,
          "group": "enhancement",
          "insertions_count": 330,
          "message": "enhancement(file source): make fingerprinting strategy configurable (#780)",
          "pr_number": 780,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "c0f8e78195e88457589d95eaa731a3ab699132d2",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-27 19:37:18 +0000",
          "deletions_count": 41,
          "description": "fix tcp sink docs formatting issues",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 33,
          "message": "docs: fix tcp sink docs formatting issues",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2ee1c39c251344bca78caa29824927b2c967ca84",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-27 17:29:43 +0000",
          "deletions_count": 1,
          "description": "Initial `journald` source implementation",
          "files_count": 20,
          "group": "feat",
          "insertions_count": 1366,
          "message": "feat(new source): Initial `journald` source implementation (#702)",
          "pr_number": 702,
          "scope": {
            "category": "source",
            "component_name": null,
            "component_type": "source",
            "name": "new source"
          },
          "sha": "0f72a2b1669a97e4838d3ca852d2f68a878915f4",
          "type": "feat"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-27 15:25:00 +0000",
          "deletions_count": 19,
          "description": "Add support for TLS",
          "files_count": 10,
          "group": "enhancement",
          "insertions_count": 460,
          "message": "enhancement(tcp sink): Add support for TLS (#765)",
          "pr_number": 765,
          "scope": {
            "category": "sink",
            "component_name": "tcp",
            "component_type": "sink",
            "name": "tcp sink"
          },
          "sha": "73a092647ef36db3b489a760b75da81cc27ef608",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-27 15:29:58 +0000",
          "deletions_count": 0,
          "description": "add test for tokenizer handling multiple spaces",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(testing): add test for tokenizer handling multiple spaces",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "4d3d5d5a79ef5124ec8a96acec558b4e63026bcb",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-08-27 14:24:59 +0000",
          "deletions_count": 0,
          "description": "add build steps as part of overall testing",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore(testing): add build steps as part of overall testing (#788)",
          "pr_number": 788,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "90bded60b2ba5618dbfbed35c7f6ac000ca5a40b",
          "type": "chore"
        },
        {
          "author": "Bittrance",
          "breaking_change": false,
          "date": "2019-08-27 17:42:00 +0000",
          "deletions_count": 13,
          "description": "`encoding = \"text\"` overrides",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 48,
          "message": "fix(aws_cloudwatch_logs sink): `encoding = \"text\"` overrides (#803)",
          "pr_number": 803,
          "scope": {
            "category": "sink",
            "component_name": "aws_cloudwatch_logs",
            "component_type": "sink",
            "name": "aws_cloudwatch_logs sink"
          },
          "sha": "19aef1601e7c2a03b340d2af0b1d4849d9a48862",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-26 17:38:18 +0000",
          "deletions_count": 22,
          "description": "Docker build image tweaks",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 28,
          "message": "chore(operations): Docker build image tweaks (#802)",
          "pr_number": 802,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "ed7605a0aeb07e16385907cc56b190345f088752",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-08-26 22:11:32 +0000",
          "deletions_count": 0,
          "description": "Add new `kafka` source",
          "files_count": 16,
          "group": "feat",
          "insertions_count": 786,
          "message": "feat(new source): Add new `kafka` source (#774)",
          "pr_number": 774,
          "scope": {
            "category": "source",
            "component_name": null,
            "component_type": "source",
            "name": "new source"
          },
          "sha": "15cd77ee9f65bc749ed17cf3673e06ca02d25a2b",
          "type": "feat"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-08-25 19:50:55 +0000",
          "deletions_count": 64,
          "description": "Use GNU ld instead of LLVM lld for x86_64-unknown-linux-musl",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 26,
          "message": "fix(operations): Use GNU ld instead of LLVM lld for x86_64-unknown-linux-musl (#794)",
          "pr_number": 794,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "3a57fe52addb3c7f0760437f31518fb9ed8f1bf0",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-24 14:21:10 +0000",
          "deletions_count": 174,
          "description": "update github label links to use new lowercase format",
          "files_count": 32,
          "group": "docs",
          "insertions_count": 174,
          "message": "docs: update github label links to use new lowercase format",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5f88b0aa44e1909736a842f9311ae3c54f0d99c2",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-24 11:35:50 +0000",
          "deletions_count": 56,
          "description": "remove sinks guidelines from docs and put them in contributing.md",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 0,
          "message": "docs: remove sinks guidelines from docs and put them in contributing.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b504f8542a57991b59a7fbd233712afdff172383",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-24 11:30:43 +0000",
          "deletions_count": 307,
          "description": "merge DEVELOPING.md into CONTRIBUTING.md",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 286,
          "message": "docs: merge DEVELOPING.md into CONTRIBUTING.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0296c9e0553b63de1d2e8fe616da12c9233b67db",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-24 17:32:34 +0000",
          "deletions_count": 48,
          "description": "Add tags support to log_to_metric transform",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 127,
          "message": "enhancement(lua transform): Add tags support to log_to_metric transform (#786)",
          "pr_number": 786,
          "scope": {
            "category": "transform",
            "component_name": "lua",
            "component_type": "transform",
            "name": "lua transform"
          },
          "sha": "e74e4694f5358154b51cbb96475972498f01d426",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-24 10:25:52 +0000",
          "deletions_count": 12,
          "description": "fix relative linking on root docs pages, ref: https://github.com/timberio/vector/pull/793",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 16,
          "message": "docs: fix relative linking on root docs pages, ref: https://github.com/timberio/vector/pull/793",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "af1a700c1b79c542b41a677b14356a1c4c8291fa",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-24 10:17:16 +0000",
          "deletions_count": 33,
          "description": "update data model docs with relevant changes",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 73,
          "message": "docs: update data model docs with relevant changes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a02ea63fc70d6b1b2c736e48fc203a09e439305b",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-08-24 06:15:03 +0000",
          "deletions_count": 0,
          "description": "Restore rust-toolchain after building",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Restore rust-toolchain after building (#792)",
          "pr_number": 792,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "990714c5b9fce922f05720ab3b84e1aec8b39826",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-23 23:12:49 +0000",
          "deletions_count": 15,
          "description": "fix source output types",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 20,
          "message": "docs: fix source output types",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "758d646be0c9f8dd1dca5c997dc57a3541eafcec",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-23 14:44:18 +0000",
          "deletions_count": 0,
          "description": "update add companies link",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: update add companies link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5e7132806adfad922a165a5da0f6c0ac3a5d0854",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-08-23 12:20:41 +0000",
          "deletions_count": 4,
          "description": "add companies list",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 38,
          "message": "docs: add companies list (#789)",
          "pr_number": 789,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "65bb96690c29e474d63ab1850ce2904c466a97e5",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-23 12:04:07 +0000",
          "deletions_count": 6,
          "description": "add log/metrics correlation feature",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 10,
          "message": "docs: add log/metrics correlation feature",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b6fcdc1176f3586f47f9593294c3fc81c6b08492",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-23 09:42:21 +0000",
          "deletions_count": 22,
          "description": "add namespace config",
          "files_count": 7,
          "group": "enhancement",
          "insertions_count": 67,
          "message": "enhancement(prometheus sink): add namespace config (#782)",
          "pr_number": 782,
          "scope": {
            "category": "sink",
            "component_name": "prometheus",
            "component_type": "sink",
            "name": "prometheus sink"
          },
          "sha": "761993432a817176ba89ead07b681c36e3b3a1f7",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-21 15:40:44 +0000",
          "deletions_count": 17,
          "description": "update cloudwatch examples",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 22,
          "message": "docs: update cloudwatch examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7e8e7a2417244e58082c855576898d9b5edb1971",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-21 15:00:40 +0000",
          "deletions_count": 4,
          "description": "fix authentication list, attempt 2",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: fix authentication list, attempt 2",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "38ba67ede2adfb8aba59c161dd47b05a519c1426",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-21 14:59:19 +0000",
          "deletions_count": 4,
          "description": "fix authentication list",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 20,
          "message": "docs: fix authentication list",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "28d62b40b19e56dfaeb851f5313ef937af9d9c79",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-21 14:29:02 +0000",
          "deletions_count": 32,
          "description": "fix partitioning language",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 11,
          "message": "docs: fix partitioning language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6ff75029425ab791f3b63818c74b55481be45139",
          "type": "docs"
        },
        {
          "author": "Jesse Szwedko",
          "breaking_change": false,
          "date": "2019-08-20 16:26:31 +0000",
          "deletions_count": 0,
          "description": "Only notify on failed/fixed master builds",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(operations): Only notify on failed/fixed master builds (#779)",
          "pr_number": 779,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "48778f848a8b3ee934a28c796d69589eab9b9242",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-20 13:52:23 +0000",
          "deletions_count": 6,
          "description": "fix UDP docs typo",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: fix UDP docs typo",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6bbd5c429706b0ca898e9cefb3d44c767adfac61",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-20 13:50:57 +0000",
          "deletions_count": 33,
          "description": "fix errors in udp source docs",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 24,
          "message": "docs: fix errors in udp source docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "546ba47f692d3deea48a067d419be3c5bda42121",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-20 13:46:53 +0000",
          "deletions_count": 7,
          "description": "fix from archive installation typos",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 7,
          "message": "docs: fix from archive installation typos",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "760d21bb84bddb692e7ac31ac8a8a2e0a86784ab",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-08-20 13:43:35 +0000",
          "deletions_count": 108,
          "description": "keep nightly builds",
          "files_count": 10,
          "group": "chore",
          "insertions_count": 154,
          "message": "chore: keep nightly builds (#772)",
          "pr_number": 772,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ff0d46f2236de6dc1bb81c1a9f898a9bf378c484",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-20 19:22:02 +0000",
          "deletions_count": 57,
          "description": "add labels support",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 131,
          "message": "enhancement(prometheus sink): add labels support (#773)",
          "pr_number": 773,
          "scope": {
            "category": "sink",
            "component_name": "prometheus",
            "component_type": "sink",
            "name": "prometheus sink"
          },
          "sha": "ab9aff1340786e8bac0ce4b7eeff31ff90e746d7",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-19 21:07:51 +0000",
          "deletions_count": 0,
          "description": "add udp source",
          "files_count": 12,
          "group": "docs",
          "insertions_count": 485,
          "message": "docs: add udp source",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8a29c615e59e5e8728d08b09bfadc92739aa75ec",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-19 21:02:35 +0000",
          "deletions_count": 0,
          "description": "add clickhouse sink documentation",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 478,
          "message": "docs: add clickhouse sink documentation",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c71c421d013d4bd68223982a36d73c3805bb4886",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-19 15:56:29 +0000",
          "deletions_count": 5,
          "description": "Add support for custom query parameters",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 64,
          "message": "enhancement(elasticsearch sink): Add support for custom query parameters (#766)",
          "pr_number": 766,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "d723a95ce9ff8689635c6bed9b4ec78a1daea81b",
          "type": "enhancement"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-16 14:55:42 +0000",
          "deletions_count": 42,
          "description": "Error type for types conversion",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 101,
          "message": "chore: Error type for types conversion (#735)",
          "pr_number": 735,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8361f6a36ce604e39ea124b2864060c5cfa680ae",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-16 13:58:08 +0000",
          "deletions_count": 29,
          "description": "Initial `clickhouse` sink implementation",
          "files_count": 18,
          "group": "feat",
          "insertions_count": 698,
          "message": "feat(new sink): Initial `clickhouse` sink implementation (#693)",
          "pr_number": 693,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "bed79bbaf9ed5ac566b1765ff989a4cbdd5aefcc",
          "type": "feat"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-15 18:47:27 +0000",
          "deletions_count": 68,
          "description": "Add rust-toolchain file and bump to 1.37",
          "files_count": 27,
          "group": "chore",
          "insertions_count": 55,
          "message": "chore: Add rust-toolchain file and bump to 1.37 (#761)",
          "pr_number": 761,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a55ed98f2aecce097f7a4e31b424e6ad47a4703e",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-15 19:33:42 +0000",
          "deletions_count": 33,
          "description": "add tags into metrics model",
          "files_count": 8,
          "group": "enhancement",
          "insertions_count": 243,
          "message": "enhancement(metric data model): add tags into metrics model (#754)",
          "pr_number": 754,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "252d145caa473a97b93051178b00ddfd7436cc46",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-15 10:59:10 +0000",
          "deletions_count": 4,
          "description": "Add guidance for writing healthchecks",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 116,
          "message": "docs: Add guidance for writing healthchecks (#755)",
          "pr_number": 755,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b0d58784a917931f8bdc0e16981bd2ff62108472",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-15 11:50:40 +0000",
          "deletions_count": 27,
          "description": "Add dynamic group creation",
          "files_count": 8,
          "group": "enhancement",
          "insertions_count": 236,
          "message": "enhancement(aws_cloudwatch_logs sink): Add dynamic group creation (#759)",
          "pr_number": 759,
          "scope": {
            "category": "sink",
            "component_name": "aws_cloudwatch_logs",
            "component_type": "sink",
            "name": "aws_cloudwatch_logs sink"
          },
          "sha": "7d2427ff1afa2addf29d96b5508133628b1e4e50",
          "type": "enhancement"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-15 11:43:59 +0000",
          "deletions_count": 5,
          "description": "Initial `aws_cloudwatch_metrics` sink implementation ",
          "files_count": 9,
          "group": "feat",
          "insertions_count": 588,
          "message": "feat(new sink): Initial `aws_cloudwatch_metrics` sink implementation  (#707)",
          "pr_number": 707,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "18abb24e03f1e5ec1613ed44ad1674ba8765361f",
          "type": "feat"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-14 17:05:21 +0000",
          "deletions_count": 4,
          "description": "fix docs generator file ext",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(docs): fix docs generator file ext",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "docs"
          },
          "sha": "60fe033ae52bf2fd7558b17037d37c9e236a02d1",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-14 15:40:06 +0000",
          "deletions_count": 1,
          "description": "Add support for additional headers to the Elasticsearch sink",
          "files_count": 5,
          "group": "enhancement",
          "insertions_count": 62,
          "message": "enhancement(elasticsearch sink): Add support for additional headers to the Elasticsearch sink (#758)",
          "pr_number": 758,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "0d0fcdfb226394ca6f26f55cd24785cc948f49d7",
          "type": "enhancement"
        },
        {
          "author": "ktff",
          "breaking_change": false,
          "date": "2019-08-14 22:47:44 +0000",
          "deletions_count": 1,
          "description": "Update Metric::Set usage",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 5,
          "message": "fix(prometheus sink): Update Metric::Set usage (#756)",
          "pr_number": 756,
          "scope": {
            "category": "sink",
            "component_name": "prometheus",
            "component_type": "sink",
            "name": "prometheus sink"
          },
          "sha": "37c998922a2a8ae96d17e82e6fd56c41679c66f8",
          "type": "fix"
        },
        {
          "author": "ktff",
          "breaking_change": false,
          "date": "2019-08-14 22:19:27 +0000",
          "deletions_count": 1,
          "description": "Initial `udp` source implementation",
          "files_count": 3,
          "group": "feat",
          "insertions_count": 230,
          "message": "feat(new source): Initial `udp` source implementation (#738)",
          "pr_number": 738,
          "scope": {
            "category": "source",
            "component_name": null,
            "component_type": "source",
            "name": "new source"
          },
          "sha": "756b115fe4db5e81358c61f88444c87010ec9268",
          "type": "feat"
        },
        {
          "author": "ktff",
          "breaking_change": false,
          "date": "2019-08-14 22:16:35 +0000",
          "deletions_count": 6,
          "description": "Support sets",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 178,
          "message": "enhancement(prometheus sink): Support sets (#733)",
          "pr_number": 733,
          "scope": {
            "category": "sink",
            "component_name": "prometheus",
            "component_type": "sink",
            "name": "prometheus sink"
          },
          "sha": "014d6f63044476c541f9f3f0f9f1092e2446ca05",
          "type": "enhancement"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-08-14 16:50:47 +0000",
          "deletions_count": 8,
          "description": "reload with unparseable config",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 14,
          "message": "fix(config): reload with unparseable config (#752)",
          "pr_number": 752,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "584196c14caa150bc97edc39e339976c2927cd1e",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-08-13 16:02:21 +0000",
          "deletions_count": 5,
          "description": "Add HTTP Basic authorization",
          "files_count": 7,
          "group": "enhancement",
          "insertions_count": 105,
          "message": "enhancement(elasticsearch sink): Add HTTP Basic authorization (#749)",
          "pr_number": 749,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "a2196b89075bbd71c82340bcab607a8eca72d1dc",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-13 11:19:15 +0000",
          "deletions_count": 0,
          "description": "Ignore topology replace source and transform",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore: Ignore topology replace source and transform (#740)",
          "pr_number": 740,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fdc863ce7f757c75a277818195fdbfe170963765",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-12 20:15:05 +0000",
          "deletions_count": 28,
          "description": "fix typo",
          "files_count": 28,
          "group": "docs",
          "insertions_count": 28,
          "message": "docs: fix typo",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "dd99c6cdf430f86285856ead0d75c2e1dab4f104",
          "type": "docs"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-08-12 15:27:11 +0000",
          "deletions_count": 1,
          "description": "Hot fix (cargo-fmt)",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore: Hot fix (cargo-fmt)",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "59f3a185cdc9038b2cdc78c027239f6f241e03e9",
          "type": "chore"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-08-12 17:12:06 +0000",
          "deletions_count": 2,
          "description": "Validation of sinks and sources for non-emptiness.",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 14,
          "message": "enhancement(config): Validation of sinks and sources for non-emptiness. (#739)",
          "pr_number": 739,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "2b8c1cdcaa5fd577770a8a5cf63fb60d4c7b50d7",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-12 11:08:34 +0000",
          "deletions_count": 8,
          "description": "fix typos",
          "files_count": 8,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: fix typos",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f59065106149213c6227b7542c7b9e46f9caf119",
          "type": "docs"
        },
        {
          "author": "Matthias Endler",
          "breaking_change": false,
          "date": "2019-08-12 16:48:17 +0000",
          "deletions_count": 134,
          "description": "Fix typo in vector image",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Fix typo in vector image (#744)",
          "pr_number": 744,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f23553a792214649acf091d2b71a23c837acee9f",
          "type": "docs"
        },
        {
          "author": "Matthias Endler",
          "breaking_change": false,
          "date": "2019-08-12 15:17:18 +0000",
          "deletions_count": 29,
          "description": "Fix typos",
          "files_count": 16,
          "group": "docs",
          "insertions_count": 30,
          "message": "docs: Fix typos (#743)",
          "pr_number": 743,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9a2f2b1e25699b9083990cf32d1e13582de6455b",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-08-10 18:34:16 +0000",
          "deletions_count": 2,
          "description": "Improve x86_64-unknown-linux-musl build",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(operations): Improve x86_64-unknown-linux-musl build (#722)",
          "pr_number": 722,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "95a19e1f9c28fdcb4ba1c337d935cabb5a29b176",
          "type": "chore"
        },
        {
          "author": "Bittrance",
          "breaking_change": false,
          "date": "2019-08-09 21:04:56 +0000",
          "deletions_count": 3,
          "description": "It is now possible to reload a with a non-overlap…",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 63,
          "message": "fix(topology): It is now possible to reload a with a non-overlap… (#681)",
          "pr_number": 681,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "topology"
          },
          "sha": "adf0f1f5cc1828fd2be012d2487bc64caa748de3",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-09 13:56:39 +0000",
          "deletions_count": 125,
          "description": "Add sink healthcheck disable",
          "files_count": 22,
          "group": "enhancement",
          "insertions_count": 531,
          "message": "enhancement(topology): Add sink healthcheck disable (#731)",
          "pr_number": 731,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "topology"
          },
          "sha": "febdde0419fd7665916ea76bfb310ec1ad805c41",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-09 11:25:50 +0000",
          "deletions_count": 27,
          "description": "update sink flow diagrams",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 25,
          "message": "docs: update sink flow diagrams",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3a2990c4da5aef70caa106f4d7382dcf3fc1ec1e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-08 16:35:00 +0000",
          "deletions_count": 2,
          "description": "fix release-s3 error",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: fix release-s3 error",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "db6829d7da7e7a3ffdf6086cadc1beb3455c79ce",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-08 23:25:46 +0000",
          "deletions_count": 195,
          "description": "add timestamps into metrics",
          "files_count": 11,
          "group": "enhancement",
          "insertions_count": 335,
          "message": "enhancement(metric data model): add timestamps into metrics (#726)",
          "pr_number": 726,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "96b1e89bf47929edd361baf4f4da34ff40a5c8a8",
          "type": "enhancement"
        },
        {
          "author": "Markus Holtermann",
          "breaking_change": false,
          "date": "2019-08-09 05:27:17 +0000",
          "deletions_count": 4,
          "description": "don't serialize MapValue::Null as a string",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 30,
          "message": "fix(log data model): don't serialize MapValue::Null as a string (#725)",
          "pr_number": 725,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "log data model"
          },
          "sha": "f22e3af44256d2c07b9f6fcc5369f94f7c405dd4",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-08 11:41:13 +0000",
          "deletions_count": 5,
          "description": "RUSTSEC-2019-0011 by updating crossbeam-epoch",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 20,
          "message": "fix(security): RUSTSEC-2019-0011 by updating crossbeam-epoch (#723)",
          "pr_number": 723,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "security"
          },
          "sha": "5a7d1516c5c08cee44cc84043db10a8253380407",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-07 22:33:00 +0000",
          "deletions_count": 1,
          "description": "remove filter on nightly builds",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore(operations): remove filter on nightly builds",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "84f87eaf788b61d07bb989410e7e74948f75ee12",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-07 19:25:24 +0000",
          "deletions_count": 22,
          "description": "add prometheus histograms test",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 39,
          "message": "chore(testing): add prometheus histograms test (#719)",
          "pr_number": 719,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "e267f72beda5092984b0f6b4c92fb785037419b9",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-07 12:06:55 +0000",
          "deletions_count": 2,
          "description": "Use a locked down version of localstack",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(testing): Use a locked down version of localstack (#720)",
          "pr_number": 720,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "1bf385dea6f4d7a02aebc9c3cc010defe5d56277",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-07 18:02:14 +0000",
          "deletions_count": 23,
          "description": "use double for storing metric values",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 18,
          "message": "chore(metric data model): use double for storing metric values (#717)",
          "pr_number": 717,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "e4108bc1b067ac83aa0dc85fcab9564af75367ef",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-06 14:19:26 +0000",
          "deletions_count": 47,
          "description": "use shorter component ids",
          "files_count": 30,
          "group": "docs",
          "insertions_count": 47,
          "message": "docs: use shorter component ids",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3c36de2691c263dfbbda747d3d328b445ec174ff",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-08-06 19:43:06 +0000",
          "deletions_count": 596,
          "description": "Support histograms",
          "files_count": 18,
          "group": "enhancement",
          "insertions_count": 742,
          "message": "enhancement(prometheus sink): Support histograms (#675)",
          "pr_number": 675,
          "scope": {
            "category": "sink",
            "component_name": "prometheus",
            "component_type": "sink",
            "name": "prometheus sink"
          },
          "sha": "855b00793cd4b2cee35788a020d1e729a02b5005",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-06 11:42:04 +0000",
          "deletions_count": 0,
          "description": "all new * as a commit title category",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 12,
          "message": "chore: all new * as a commit title category",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "bedccf409b61c7eeaa9d96126fed184ff0df27fe",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 23:41:36 +0000",
          "deletions_count": 18,
          "description": "fix duplicate section references",
          "files_count": 13,
          "group": "docs",
          "insertions_count": 145,
          "message": "docs: fix duplicate section references",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "495d8be6299de55d5e31a84cfe467f263582d9df",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 23:21:07 +0000",
          "deletions_count": 189,
          "description": "aws_s3_sink encoding is not required",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 184,
          "message": "docs: aws_s3_sink encoding is not required",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "929026eb7bdeea9459ab81324124f46b85674c78",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 23:06:48 +0000",
          "deletions_count": 2,
          "description": "add valid scopes",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 63,
          "message": "docs: add valid scopes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a9993b8e1aa6557be5ddef47cc3d305fe0a50a56",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 22:37:34 +0000",
          "deletions_count": 40,
          "description": "fix typo",
          "files_count": 11,
          "group": "docs",
          "insertions_count": 40,
          "message": "docs: fix typo",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9b7b786d74e1ad57322ae7f5e3ec5bcd2073d9cf",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 22:36:17 +0000",
          "deletions_count": 96,
          "description": "remove false default values that communicate dynamic behavior",
          "files_count": 16,
          "group": "docs",
          "insertions_count": 136,
          "message": "docs: remove false default values that communicate dynamic behavior",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4c0046c54cd7bef189a7a9422f3b7608ecb17ebd",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 22:18:02 +0000",
          "deletions_count": 248,
          "description": "fix html escaping issues",
          "files_count": 33,
          "group": "docs",
          "insertions_count": 248,
          "message": "docs: fix html escaping issues",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "67bbaa52e35e1736f383f995a0399c9960d34a24",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-05 22:14:20 +0000",
          "deletions_count": 281,
          "description": "add html escaping",
          "files_count": 37,
          "group": "docs",
          "insertions_count": 326,
          "message": "docs: add html escaping",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "906bb21fe220d827b0fbc018d223bda0792d6006",
          "type": "docs"
        },
        {
          "author": "Denis Andrejew",
          "breaking_change": false,
          "date": "2019-08-06 03:48:52 +0000",
          "deletions_count": 50,
          "description": "fall back to global data_dir option (#644)",
          "files_count": 14,
          "group": "enhancement",
          "insertions_count": 191,
          "message": "enhancement(file source): fall back to global data_dir option (#644) (#673)",
          "pr_number": 673,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "e190e96e925d819d7460fab64f37fdb4241b88ad",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-04 11:36:35 +0000",
          "deletions_count": 10,
          "description": "fix lua drop event example",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: fix lua drop event example",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ce3bc8dd988539643df3f5a6447696a7ebac108f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-04 11:32:10 +0000",
          "deletions_count": 59,
          "description": "fix alternative suggestions",
          "files_count": 32,
          "group": "docs",
          "insertions_count": 70,
          "message": "docs: fix alternative suggestions",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "bb4e220318db02c9d52703d7a603f70b9731473d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-04 11:25:06 +0000",
          "deletions_count": 58,
          "description": "update log_to_metric docs to reflect all metric types",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 284,
          "message": "docs: update log_to_metric docs to reflect all metric types",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7f9a86f5de31a3d2b17dc8a22b7ae27420eceed6",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-04 10:15:41 +0000",
          "deletions_count": 230,
          "description": "update enum language",
          "files_count": 37,
          "group": "docs",
          "insertions_count": 239,
          "message": "docs: update enum language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b317396794f97d66208379f2cfffe007ac1a51fa",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 15:59:09 +0000",
          "deletions_count": 2,
          "description": "add summary for Vector config syntax",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 6,
          "message": "docs: add summary for Vector config syntax",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "86fef80098e7e438852917dbcd58eec0e8e8ac44",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 15:57:52 +0000",
          "deletions_count": 16,
          "description": "fix template syntax broken link",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 16,
          "message": "docs: fix template syntax broken link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "d391ad95a259cab216f4848f7c938a612749e043",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 15:56:28 +0000",
          "deletions_count": 5,
          "description": "fix doc typo",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: fix doc typo",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e37229641eac91eee4c9699b34ae821ad8548ada",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 15:55:56 +0000",
          "deletions_count": 5,
          "description": "remote strftime comment in s3 key_prefix description",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: remote strftime comment in s3 key_prefix description",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5406afbf6d5632be9202d83a4d1055c91d022549",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 15:52:47 +0000",
          "deletions_count": 137,
          "description": "add documentation on Vectors template syntax",
          "files_count": 43,
          "group": "docs",
          "insertions_count": 2601,
          "message": "docs: add documentation on Vectors template syntax",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7fea8ad0876a90a7b6bdc3e14f686978b5d109f3",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 13:15:08 +0000",
          "deletions_count": 4,
          "description": "fix build syntax error",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(operations): fix build syntax error",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "57d57db514a0de09dcbd4f98405c9b9a26b1c027",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 13:13:29 +0000",
          "deletions_count": 71,
          "description": "fix nightly builds, release to docker and s3",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 128,
          "message": "chore(operations): fix nightly builds, release to docker and s3",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5eb88c35797cebf49e1ede178b94598a0afdd5eb",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 11:15:11 +0000",
          "deletions_count": 19,
          "description": "cleanup docker language",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 28,
          "message": "docs: cleanup docker language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4b066e6d7f40fd3cc6d967bfc527c0c8aa8c3718",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 11:06:08 +0000",
          "deletions_count": 1,
          "description": "update installer script to use musl statically linked archive",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(chore): update installer script to use musl statically linked archive",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "chore"
          },
          "sha": "b8ee40ead6b03b23f11940f2038dd0c10580e48b",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 11:02:01 +0000",
          "deletions_count": 31,
          "description": "update chat to chat/forum since it servers both purposes now",
          "files_count": 30,
          "group": "docs",
          "insertions_count": 31,
          "message": "docs: update chat to chat/forum since it servers both purposes now",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7e161e6aa92f20e3b758fb0a87889fa05346ab18",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-03 10:58:27 +0000",
          "deletions_count": 7,
          "description": "add data model diagram",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 216,
          "message": "docs: add data model diagram",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "02cfaa1e6a78f08d0eba93cdb15a6049940f7d8a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 18:31:21 +0000",
          "deletions_count": 6,
          "description": "fix docker html entity escaping",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: fix docker html entity escaping",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ecdf4ed715901b8a9c132b57df120b5bdf1a2f63",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-08-02 18:29:23 +0000",
          "deletions_count": 32,
          "description": "update vector docker images to reflect their base image",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 34,
          "message": "chore(operations): update vector docker images to reflect their base image (#705)",
          "pr_number": 705,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "b511fd9362421e6ac3b73187a8ac1f61ea309501",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-08-02 16:40:06 +0000",
          "deletions_count": 151,
          "description": "use templates for ES index and S3 key prefix",
          "files_count": 4,
          "group": "enhancement",
          "insertions_count": 275,
          "message": "enhancement(elasticsearch sink): use templates for ES index and S3 key prefix (#686)",
          "pr_number": 686,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "8fe6b2252bfe7bf20f17327a46771742eb80396c",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-02 17:24:18 +0000",
          "deletions_count": 40,
          "description": "unflatten event before outputting",
          "files_count": 17,
          "group": "fix",
          "insertions_count": 552,
          "message": "fix(log data model): unflatten event before outputting (#678)",
          "pr_number": 678,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "log data model"
          },
          "sha": "fbed6bdddddc627f6400bf36a075fcd897a8b09a",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 17:16:13 +0000",
          "deletions_count": 4,
          "description": "recommend alpine docker image",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: recommend alpine docker image",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "12ce3a069bbc809a08d5561ddbd4593c318b9960",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 16:32:51 +0000",
          "deletions_count": 3,
          "description": "attempt to fix data model type links",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 3,
          "message": "docs: attempt to fix data model type links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fb9595e5abc2072ec454c2e84e59a056cab5d65b",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 16:01:55 +0000",
          "deletions_count": 4,
          "description": "singularize log event types",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: singularize log event types",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f07e3ce41cea2492114e8b20d35f703843a825ac",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:58:47 +0000",
          "deletions_count": 32,
          "description": "expand on log event types",
          "files_count": 21,
          "group": "docs",
          "insertions_count": 66,
          "message": "docs: expand on log event types",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0d19588b28bab2f2c44760e610516c5ce17ad6b4",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:42:02 +0000",
          "deletions_count": 2,
          "description": "fix subnav item names for log and event",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: fix subnav item names for log and event",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8758a7c978fa62fe88960f4dd10ebc17a604a743",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:41:20 +0000",
          "deletions_count": 4,
          "description": "fix path typo in subnav",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: fix path typo in subnav",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "093bf3f5e7054cbfd58a4443a454e7532a8f844e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:40:31 +0000",
          "deletions_count": 2,
          "description": "rename log and metric subnav items because Gitbook...",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: rename log and metric subnav items because Gitbook...",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e57120ab88d37b817a207588bca03d377e9c94b0",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:39:48 +0000",
          "deletions_count": 2,
          "description": "rename log and metric event titles because gitbook...",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: rename log and metric event titles because gitbook...",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9f2e7dda97814710e62fe8d1cf07ff2ee7d4ccf4",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 15:37:59 +0000",
          "deletions_count": 0,
          "description": "add log and metrics subnav items for the data model section",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: add log and metrics subnav items for the data model section",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "69a1c145621cc0a922f059f340d3fcd28938631b",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-02 15:34:46 +0000",
          "deletions_count": 522,
          "description": "Add configurable partition keys",
          "files_count": 54,
          "group": "enhancement",
          "insertions_count": 1075,
          "message": "enhancement(aws_kinesis_streams sink): Add configurable partition keys (#692)",
          "pr_number": 692,
          "scope": {
            "category": "sink",
            "component_name": "aws_kinesis_streams",
            "component_type": "sink",
            "name": "aws_kinesis_streams sink"
          },
          "sha": "05a2aecb33dd95e1b1e99f923767b2e40b082339",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 10:44:50 +0000",
          "deletions_count": 10,
          "description": "cleanup musl archive language",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: cleanup musl archive language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e70d1834e34c58ae0e31b630f3a148d5ed3c64d4",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-08-02 10:35:15 +0000",
          "deletions_count": 37,
          "description": "release nightly instead of on each commit",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 47,
          "message": "chore(operations): release nightly instead of on each commit (#703)",
          "pr_number": 703,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a9ec4a75d753f6e939df4b31b9d3ba8f700ff890",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-02 09:24:55 +0000",
          "deletions_count": 3,
          "description": "remove musl warnings since it includes all features now",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 3,
          "message": "docs: remove musl warnings since it includes all features now",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "bce95689d801b35a635328f9524613da3b137b39",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 18:54:58 +0000",
          "deletions_count": 5,
          "description": "fix broken links",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: fix broken links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c8a304590fdced8b867a8b3d1d44b86c67dd0bfb",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 18:34:07 +0000",
          "deletions_count": 0,
          "description": "fix docker.md parsing error",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: fix docker.md parsing error",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4b0735f5b64da0c7d6aba1a15d803d1767048fe4",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 18:14:05 +0000",
          "deletions_count": 11,
          "description": "Add rate limit notice when it starts",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 40,
          "message": "enhancement(observability): Add rate limit notice when it starts (#696)",
          "pr_number": 696,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "c3345f5da237fcfb94caccdd88ab0adfb7e333eb",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 18:00:03 +0000",
          "deletions_count": 1,
          "description": "make binary stripping an option during the release process, fixes an issue stripping armv7 binaries",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore(operations): make binary stripping an option during the release process, fixes an issue stripping armv7 binaries",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "df3df71d2b9c1b2f53f2590bc5bb0c1a639ff1c4",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 17:24:05 +0000",
          "deletions_count": 1,
          "description": "add TARGET env var to musl build archive step",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(operations): add TARGET env var to musl build archive step",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "22f8454d4b70496262f57e3f4e4232768fc30ebd",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 13:35:34 +0000",
          "deletions_count": 3,
          "description": "Remove extra debug flags",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore: Remove extra debug flags",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "88726cb21b0c4284373cfd12ce1b230d307e8a07",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 13:35:09 +0000",
          "deletions_count": 10,
          "description": "Fix build-archive script to support multiple features",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 23,
          "message": "chore(operations): Fix build-archive script to support multiple features",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c47d9cd610befbede9846c61437be748884f4c46",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 11:57:01 +0000",
          "deletions_count": 19,
          "description": "Disable armv7 musleabihf build",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 19,
          "message": "chore(operations): Disable armv7 musleabihf build (#698)",
          "pr_number": 698,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "feca20d2ba5cba4c88bef431a1ec4988ba26f6c9",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-08-01 18:52:16 +0000",
          "deletions_count": 20,
          "description": "Build for x86_64-unknown-linux-musl with all features and optimized binary size",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 365,
          "message": "enhancement(operations): Build for x86_64-unknown-linux-musl with all features and optimized binary size (#689)",
          "pr_number": 689,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "d2df9ba321990a0bf5996f18135351fa8bbf296c",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 11:18:23 +0000",
          "deletions_count": 4,
          "description": "remove Slack since we no longer use Slack",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore: remove Slack since we no longer use Slack",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e37995eec33941545694d8c9d8b784f081c4c785",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-08-01 11:17:20 +0000",
          "deletions_count": 82,
          "description": "update documentation to reflect new help resources",
          "files_count": 37,
          "group": "docs",
          "insertions_count": 208,
          "message": "docs: update documentation to reflect new help resources",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0ae355f76c66147032b6ef5e4bdab141bfd2eeef",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 11:07:09 +0000",
          "deletions_count": 20,
          "description": "Retry requests on timeouts",
          "files_count": 5,
          "group": "fix",
          "insertions_count": 146,
          "message": "fix(networking): Retry requests on timeouts (#691)",
          "pr_number": 691,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "networking"
          },
          "sha": "57bc070a11ef3141ee5829d043f3720e359da726",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-08-01 11:06:41 +0000",
          "deletions_count": 4,
          "description": "Default `doc_type` to `_doc` and make it op…",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 6,
          "message": "enhancement(elasticsearch sink): Default `doc_type` to `_doc` and make it op… (#695)",
          "pr_number": 695,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "9675b5197d60d3ff6a3ddd81cd9b4ec08bc92576",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-31 16:12:16 +0000",
          "deletions_count": 97,
          "description": "remove forum references, we recommend filing a help issue or joining our Slack channel instead",
          "files_count": 34,
          "group": "chore",
          "insertions_count": 33,
          "message": "chore: remove forum references, we recommend filing a help issue or joining our Slack channel instead",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9ec1c644e82b029b943a1017f8176e77b1e494bd",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-26 12:01:25 +0000",
          "deletions_count": 355,
          "description": "Add retry ability to cloudwatch",
          "files_count": 24,
          "group": "enhancement",
          "insertions_count": 605,
          "message": "enhancement(aws_cloudwatch_logs sink): Add retry ability to cloudwatch (#663)",
          "pr_number": 663,
          "scope": {
            "category": "sink",
            "component_name": "aws_cloudwatch_logs",
            "component_type": "sink",
            "name": "aws_cloudwatch_logs sink"
          },
          "sha": "05032c6803bf1d45eaf2372a58d46fadaa9646bb",
          "type": "enhancement"
        },
        {
          "author": "Denis Andrejew",
          "breaking_change": false,
          "date": "2019-07-26 16:53:55 +0000",
          "deletions_count": 2,
          "description": "replace some references to \"sink\" with `component.type`",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: replace some references to \"sink\" with `component.type` (#685)",
          "pr_number": 685,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "df6816f2432039236ba14361262012380b8f5c82",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-25 15:53:32 +0000",
          "deletions_count": 7,
          "description": "Update nom from 0.5.0-beta2 to 0.5",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 21,
          "message": "chore: Update nom from 0.5.0-beta2 to 0.5 (#679)",
          "pr_number": 679,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "89a32737baa90f36de69da19fe95ba6734283368",
          "type": "chore"
        },
        {
          "author": "Cédric Da Fonseca",
          "breaking_change": false,
          "date": "2019-07-25 16:41:31 +0000",
          "deletions_count": 4,
          "description": "minor fixes in getting-started page",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: minor fixes in getting-started page (#682)",
          "pr_number": 682,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6670fc00c576788fecb9e7f8321f76f2dc08eb6f",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-24 09:56:17 +0000",
          "deletions_count": 58,
          "description": "use templates for metric names in log_to_metric",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 141,
          "message": "enhancement(log_to_metric transform): use templates for metric names in log_to_metric (#668)",
          "pr_number": 668,
          "scope": {
            "category": "transform",
            "component_name": "log_to_metric",
            "component_type": "transform",
            "name": "log_to_metric transform"
          },
          "sha": "1fbd6a4eead61518d8678ca39b6baadbbec30314",
          "type": "enhancement"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-07-23 13:24:31 +0000",
          "deletions_count": 20,
          "description": "add coercer transform",
          "files_count": 17,
          "group": "feat",
          "insertions_count": 689,
          "message": "feat(new transform): add coercer transform (#666)",
          "pr_number": 666,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "f1dfaf90512f3ea8a8a0bee743bfb297b08657df",
          "type": "feat"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-22 13:49:30 +0000",
          "deletions_count": 2,
          "description": "Use multi-stage builds for vector-slim Docker image",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 11,
          "message": "chore(operations): Use multi-stage builds for vector-slim Docker image (#672)",
          "pr_number": 672,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2ecb9897b3d469a0eb0c180db9ba371cde87443b",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-22 13:44:46 +0000",
          "deletions_count": 4,
          "description": "fix broken build process",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(operations): fix broken build process",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "bd22713c4b544b82b56b079bd2ef7411af951226",
          "type": "chore"
        },
        {
          "author": "Brian Kabiro",
          "breaking_change": false,
          "date": "2019-07-22 20:21:26 +0000",
          "deletions_count": 2,
          "description": "fix spelling in READMEs",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: fix spelling in READMEs (#671)",
          "pr_number": 671,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "01458f4e5764e6d06ca04b3a569eeb767ac58eee",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-22 13:13:53 +0000",
          "deletions_count": 27,
          "description": "build x86_64-unknown-linux-musl with all features",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 54,
          "message": "chore(operations): build x86_64-unknown-linux-musl with all features (#669)",
          "pr_number": 669,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "6c47a6716206d066191d4e67d810df0f7f761c96",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-21 10:05:28 +0000",
          "deletions_count": 24,
          "description": "update batch_timeuot unit to seconds across all docs",
          "files_count": 9,
          "group": "docs",
          "insertions_count": 28,
          "message": "docs: update batch_timeuot unit to seconds across all docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0a4ef9774092eef2d9d48ec7167b73d46caf464a",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-21 09:16:01 +0000",
          "deletions_count": 4,
          "description": "add support for armv7 releases, both gnueabihf and musleabihf",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 84,
          "message": "chore(operations): add support for armv7 releases, both gnueabihf and musleabihf (#662)",
          "pr_number": 662,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a69668faab8c759e40377e696e5750f6bc58f244",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-19 11:10:53 +0000",
          "deletions_count": 1,
          "description": "switch batch_timeout from bytes to seconds",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: switch batch_timeout from bytes to seconds",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "d6f3a1a4c2f8da71b950725f7bb164f526c12386",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-07-19 18:10:09 +0000",
          "deletions_count": 12,
          "description": "Use correct units in example batch timeouts",
          "files_count": 12,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: Use correct units in example batch timeouts (#664)",
          "pr_number": 664,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6812ca83f8de0a5c2bd6d131f3c7026b2a223d57",
          "type": "docs"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-18 14:23:44 +0000",
          "deletions_count": 64,
          "description": "reusable templating system for event values",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 162,
          "message": "enhancement(config): reusable templating system for event values (#656)",
          "pr_number": 656,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "b4575e662c5d06eb52d43678c6031d095bfa06de",
          "type": "enhancement"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-18 14:34:36 +0000",
          "deletions_count": 5,
          "description": "add timberio/vector-alpine docker image",
          "files_count": 8,
          "group": "chore",
          "insertions_count": 86,
          "message": "chore(operations): add timberio/vector-alpine docker image (#659)",
          "pr_number": 659,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "075e1cca2744e3fb868e852236345c484ae4973e",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-18 10:18:54 +0000",
          "deletions_count": 36,
          "description": "remove labels support from log_to_metric",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 20,
          "message": "chore(operations): remove labels support from log_to_metric (#657)",
          "pr_number": 657,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "e3e31d04d87513f21083a094d90b79b358ed4cd8",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-18 09:52:43 +0000",
          "deletions_count": 68,
          "description": "push Histogram and Set metrics from logs",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 155,
          "message": "enhancement(log_to_metric transform): push Histogram and Set metrics from logs (#650)",
          "pr_number": 650,
          "scope": {
            "category": "transform",
            "component_name": "log_to_metric",
            "component_type": "transform",
            "name": "log_to_metric transform"
          },
          "sha": "32d2f6ba6d47f5c7f4c031dc25a7026edf4f869d",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-17 17:03:11 +0000",
          "deletions_count": 0,
          "description": "retry HttpDispatch errors for s3 and kinesis",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 2,
          "message": "fix(aws_s3 sink): retry HttpDispatch errors for s3 and kinesis (#651)",
          "pr_number": 651,
          "scope": {
            "category": "sink",
            "component_name": "aws_s3",
            "component_type": "sink",
            "name": "aws_s3 sink"
          },
          "sha": "75f05f4626323cb47cdfbf6caf6ca0030f500f15",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-17 16:26:30 +0000",
          "deletions_count": 2,
          "description": "rename call when releasing to latest and edge channels in s3",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): rename call when releasing to latest and edge channels in s3",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c7654ce407fc525a22f0fa4b5a5fa949bb4247de",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-17 16:03:29 +0000",
          "deletions_count": 41,
          "description": "add support for x86_64-unknown-linux-musl releases",
          "files_count": 10,
          "group": "chore",
          "insertions_count": 51,
          "message": "chore(operations): add support for x86_64-unknown-linux-musl releases (#654)",
          "pr_number": 654,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5099d14e6f809235e87f0ee95737ea7e67a5a8b6",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-17 15:18:07 +0000",
          "deletions_count": 6,
          "description": "Update smallvec to `v0.6.10`",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore(tech debt): Update smallvec to `v0.6.10` (#652)",
          "pr_number": 652,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "tech debt"
          },
          "sha": "4e1e9e21b71a9ccdc38a38d51b9727f332721f05",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-17 15:04:24 +0000",
          "deletions_count": 2,
          "description": "Add `jemallocator` feature flag",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 3,
          "message": "enhancement(operations): Add `jemallocator` feature flag (#653)",
          "pr_number": 653,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "1f2319f9b49260951824bce7c3d75548347f1d2a",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-17 10:36:59 +0000",
          "deletions_count": 1,
          "description": "add test around min file size for fingerprinting",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 41,
          "message": "chore: add test around min file size for fingerprinting",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1ea7e30d460f7f00be6d138f0d875ed8efbb0904",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-16 21:49:20 +0000",
          "deletions_count": 318,
          "description": "accept both logs and metrics",
          "files_count": 31,
          "group": "enhancement",
          "insertions_count": 600,
          "message": "enhancement(console sink): accept both logs and metrics (#631)",
          "pr_number": 631,
          "scope": {
            "category": "sink",
            "component_name": "console",
            "component_type": "sink",
            "name": "console sink"
          },
          "sha": "fc93a801ba5ae8ae90132727f3ad194691b6bfb0",
          "type": "enhancement"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-16 22:02:24 +0000",
          "deletions_count": 81,
          "description": "Refactor metrics sampling, rename Timer to Histogram",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 96,
          "message": "chore(metric data model): Refactor metrics sampling, rename Timer to Histogram (#648)",
          "pr_number": 648,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "33489984d28285740d26dcd2bc3183dfafb9711f",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-07-16 13:01:23 +0000",
          "deletions_count": 78,
          "description": "add type coercion",
          "files_count": 12,
          "group": "enhancement",
          "insertions_count": 322,
          "message": "enhancement(grok_parser transform): add type coercion (#632)",
          "pr_number": 632,
          "scope": {
            "category": "transform",
            "component_name": "grok_parser",
            "component_type": "transform",
            "name": "grok_parser transform"
          },
          "sha": "fddfbe83ee89a890662872a6a614c8213da8d37b",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-15 22:59:20 +0000",
          "deletions_count": 36,
          "description": "test thread usage to ensure tests pass on all machines",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 40,
          "message": "chore(testing): test thread usage to ensure tests pass on all machines (#646)",
          "pr_number": 646,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "a308ed2744bddf9f2b4b2607fec40800c622bd7b",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 17:54:27 +0000",
          "deletions_count": 1,
          "description": "add convetional commits to contributing",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: add convetional commits to contributing",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "59592d3a1e62169ffe934c7773c4ebc3d6392630",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 17:51:36 +0000",
          "deletions_count": 7,
          "description": "add AWS env vars",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: add AWS env vars",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2a07d727b2acc57dd72746356dd5ad0284b23208",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 17:47:38 +0000",
          "deletions_count": 0,
          "description": "add exit codes",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 17,
          "message": "docs: add exit codes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "92dfdca8f99986961d4eb66ce480770700ee1994",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 17:40:12 +0000",
          "deletions_count": 2,
          "description": "Add validating page for administration docs",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 49,
          "message": "docs: Add validating page for administration docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0cd9c302dfbd37f320e56ac385801af6bdf18404",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 16:46:33 +0000",
          "deletions_count": 8,
          "description": "Add docs about file checkpointing",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 31,
          "message": "docs: Add docs about file checkpointing",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "81b9e4f06a8fe6b0ce5f3592921d6bebea7aa85f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 16:33:18 +0000",
          "deletions_count": 3,
          "description": "Add reference to glob_minimum_cooldown option",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: Add reference to glob_minimum_cooldown option",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b3d1a767b46302dc8d698812b960afce23c511b2",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 16:23:48 +0000",
          "deletions_count": 13,
          "description": "Fix Github labels query param",
          "files_count": 14,
          "group": "docs",
          "insertions_count": 17,
          "message": "docs: Fix Github labels query param",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b420159287f19a2aa4405da6f90fcea733d9de28",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 16:18:43 +0000",
          "deletions_count": 5,
          "description": "Fix sampler rate example",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: Fix sampler rate example",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7fa2515374f55848e128deb50e153488c9fe330f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 16:17:41 +0000",
          "deletions_count": 85,
          "description": "Add component context section",
          "files_count": 22,
          "group": "docs",
          "insertions_count": 135,
          "message": "docs: Add component context section",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "de2f4b3a9f845f57b6e5d40342e8f4a64639f91d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 15:43:12 +0000",
          "deletions_count": 31,
          "description": "Add fingerprint options for file source to docs",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 95,
          "message": "docs: Add fingerprint options for file source to docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "dd48943579fe07525aa2f93a7ecf357617d54194",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 15:34:50 +0000",
          "deletions_count": 0,
          "description": "Add sampler transform to summary.md",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Add sampler transform to summary.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f8aeff54adf9aa46175a98b5211705393d0c4c20",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 15:33:43 +0000",
          "deletions_count": 0,
          "description": "Add glob_minimum_cooldown option to file source docs",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 29,
          "message": "docs: Add glob_minimum_cooldown option to file source docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fe54f1e9d28ea18c94063170819c2fced8397a26",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 15:04:19 +0000",
          "deletions_count": 67,
          "description": "Use one consistent env var syntax",
          "files_count": 31,
          "group": "docs",
          "insertions_count": 67,
          "message": "docs: Use one consistent env var syntax",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1a4229093e15452f2c378a81e448ce85167709f3",
          "type": "docs"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-07-15 20:41:33 +0000",
          "deletions_count": 156,
          "description": "Improve configuration validation and make it more strict",
          "files_count": 17,
          "group": "enhancement",
          "insertions_count": 236,
          "message": "enhancement(config): Improve configuration validation and make it more strict (#552)",
          "pr_number": 552,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "0d0c9d62f2f737359331cc2a52d988850552f0fc",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 14:37:21 +0000",
          "deletions_count": 0,
          "description": "Add semtantic.yml to only check PR titles",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: Add semtantic.yml to only check PR titles",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "524355cde9009936fe5eeae0a85315bd3405dc94",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-15 11:53:41 +0000",
          "deletions_count": 1,
          "description": "Use the proper type in the blackhole example",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Use the proper type in the blackhole example",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1176821bd86431ef8cf0b9db763a85828c3116c7",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-14 17:56:04 +0000",
          "deletions_count": 215,
          "description": "Add doc sections for all sink egress methods",
          "files_count": 19,
          "group": "docs",
          "insertions_count": 259,
          "message": "docs: Add doc sections for all sink egress methods",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "20f678c0d96ce9ad282abc34d30a23ce13f63a97",
          "type": "docs"
        },
        {
          "author": "Ayhan",
          "breaking_change": false,
          "date": "2019-07-14 20:42:28 +0000",
          "deletions_count": 1,
          "description": "Fix argument type",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore: Fix argument type (#639)",
          "pr_number": 639,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f1e0938c5ef508dda26005e567d6aaab6eabe0ab",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 08:51:14 +0000",
          "deletions_count": 2,
          "description": "Batch diagram language",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Batch diagram language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0db4e693ec618ea21f5273c85c0810f15973353d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 08:49:50 +0000",
          "deletions_count": 24,
          "description": "Fix authentication formatting",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: Fix authentication formatting",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "472bd3574089994d464e1b91746bfc35a382e934",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 08:43:59 +0000",
          "deletions_count": 108,
          "description": "Fix config example headers for transforms and sources",
          "files_count": 34,
          "group": "docs",
          "insertions_count": 188,
          "message": "docs: Fix config example headers for transforms and sources",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5746533135b33aae4b35aee5feb169ade0284810",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 08:30:34 +0000",
          "deletions_count": 134,
          "description": "Add relevance text to options table",
          "files_count": 19,
          "group": "docs",
          "insertions_count": 99,
          "message": "docs: Add relevance text to options table",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4b76ae8e2dba91dd0943aa7947325c8ed2b7cdf4",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 08:23:08 +0000",
          "deletions_count": 25,
          "description": "Add relevant when... tag for options that depend on other options in docs",
          "files_count": 17,
          "group": "docs",
          "insertions_count": 40,
          "message": "docs: Add relevant when... tag for options that depend on other options in docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "150994527180a69eb848fffa9a810d7fe376d2d1",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-13 07:42:55 +0000",
          "deletions_count": 92,
          "description": "Fix environment variable language in docs",
          "files_count": 28,
          "group": "docs",
          "insertions_count": 95,
          "message": "docs: Fix environment variable language in docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fcbf1aef0eee29bc3a36f2cce7e5ab2387a0acb7",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-12 16:51:55 +0000",
          "deletions_count": 18,
          "description": "Update grok_parser language",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 16,
          "message": "docs: Update grok_parser language",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "419b2f0f73c89d81eb636cb8af43a52489fca3cb",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-12 13:12:16 +0000",
          "deletions_count": 22,
          "description": "Add examples to the add_fields docs",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 31,
          "message": "docs: Add examples to the add_fields docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "94d6838e901d745ada07cc62649dbbc3cef52bcb",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-12 10:34:05 +0000",
          "deletions_count": 7,
          "description": "Fix section references for fields that include Regex special characters",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Fix section references for fields that include Regex special characters",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c4976fd7d54d23cff1595d1de183ce04ba81153a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-12 09:44:16 +0000",
          "deletions_count": 2,
          "description": "Link to log data model in add fields docs",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: Link to log data model in add fields docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c98f455cfc7dda43b2f09d5804134b3832ae3153",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-12 09:42:16 +0000",
          "deletions_count": 73,
          "description": "Add default envirnoment variables section",
          "files_count": 33,
          "group": "docs",
          "insertions_count": 317,
          "message": "docs: Add default envirnoment variables section",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a95201a3c1fe73f7d250f313fe786458bc9aa880",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-12 12:09:33 +0000",
          "deletions_count": 2,
          "description": "Fix cloudwatch test by dropping sink",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore: Fix cloudwatch test by dropping sink (#626)",
          "pr_number": 626,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "934011d78f8fc92bfff922a61bb0bf0269ad0ac7",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-10 08:15:13 +0000",
          "deletions_count": 136,
          "description": "Fix add_fields transform docs",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 64,
          "message": "docs: Fix add_fields transform docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0fb311fbce5d4a304d82e50e186fd03636bf1c44",
          "type": "docs"
        },
        {
          "author": "apjones-proton",
          "breaking_change": false,
          "date": "2019-07-12 10:08:40 +0000",
          "deletions_count": 325,
          "description": "Add File checkpoint feature.",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 539,
          "message": "enhancement(file source): Add File checkpoint feature. (#609)",
          "pr_number": 609,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "0820c1087f9c524d55a96f726a56afd09c2f0069",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-11 17:24:12 +0000",
          "deletions_count": 7,
          "description": "Back out change to dash handling",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore: Back out change to dash handling",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4a88262f95ace846b60d4ebe2857d1c1d3170bbe",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-11 17:36:24 +0000",
          "deletions_count": 211,
          "description": "Add cloudwatch partitioning and refactor partition buffer",
          "files_count": 7,
          "group": "enhancement",
          "insertions_count": 656,
          "message": "enhancement(aws_cloudwatch_logs sink): Add cloudwatch partitioning and refactor partition buffer (#519)",
          "pr_number": 519,
          "scope": {
            "category": "sink",
            "component_name": "aws_cloudwatch_logs",
            "component_type": "sink",
            "name": "aws_cloudwatch_logs sink"
          },
          "sha": "d8a8e961a35d2eb7dadf183a69f214a4637a47b0",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-11 17:32:52 +0000",
          "deletions_count": 37,
          "description": "Add `--color` option and tty check for ansi colors",
          "files_count": 5,
          "group": "enhancement",
          "insertions_count": 64,
          "message": "enhancement(cli): Add `--color` option and tty check for ansi colors (#623)",
          "pr_number": 623,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "e93621195a390383ae5fec131f2e01874ea842d8",
          "type": "enhancement"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-07-10 19:50:42 +0000",
          "deletions_count": 1,
          "description": "Log when regex does not match",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 13,
          "message": "enhancement(regex_parser transform): Log when regex does not match (#618)",
          "pr_number": 618,
          "scope": {
            "category": "transform",
            "component_name": "regex_parser",
            "component_type": "transform",
            "name": "regex_parser transform"
          },
          "sha": "009803467f4513827abbe4a28d8170a5593ea2c5",
          "type": "enhancement"
        },
        {
          "author": "apjones-proton",
          "breaking_change": false,
          "date": "2019-07-10 18:03:27 +0000",
          "deletions_count": 25,
          "description": "File tests timeout instead of hang if channel is stuck open.",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 63,
          "message": "chore: File tests timeout instead of hang if channel is stuck open. (#612)",
          "pr_number": 612,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "96fadd8decbae32b6ce55063566ba683e27cdc96",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-10 09:01:17 +0000",
          "deletions_count": 4,
          "description": "Debian 10 verification step",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 30,
          "message": "chore(operations): Debian 10 verification step (#615)",
          "pr_number": 615,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "80347525540296db8e9a06140e9359093d9144a6",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-10 07:47:27 +0000",
          "deletions_count": 2,
          "description": "Fix debian-slim install line in docs",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Fix debian-slim install line in docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "734aa228d859357c671c3e61732fdd49b1d9295b",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-09 22:11:45 +0000",
          "deletions_count": 30,
          "description": "Dont use HTML characters in default value for docs",
          "files_count": 11,
          "group": "docs",
          "insertions_count": 30,
          "message": "docs: Dont use HTML characters in default value for docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6eaa2912a8f2440fc968c87e0f6287da0f752291",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-09 22:09:43 +0000",
          "deletions_count": 2,
          "description": "Restore docker installation instructions",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 84,
          "message": "docs: Restore docker installation instructions",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2d1c24a8ced93db9496248a52271f5a0d0f6b534",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-09 13:54:16 +0000",
          "deletions_count": 2658,
          "description": "Add section references to each option within the docs",
          "files_count": 56,
          "group": "docs",
          "insertions_count": 453,
          "message": "docs: Add section references to each option within the docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "82274cca2047432ecc378f8343703dc5d96ab801",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-09 01:16:54 +0000",
          "deletions_count": 18,
          "description": "Fix lock file",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Fix lock file",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "099f062c35c5888a79422d4ee1abca1e200d6a4b",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 17:18:26 +0000",
          "deletions_count": 4,
          "description": "Restore \"send your first event\" guide",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 96,
          "message": "docs: Restore \"send your first event\" guide",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "27fce01ed595969e716bac9c0f688b5813e81e4d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 17:00:46 +0000",
          "deletions_count": 248,
          "description": "Fix docs/README.md",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 20,
          "message": "docs: Fix docs/README.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b44cc232bc9dd9cee1acac9726b18a02fff0ab7d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 16:50:21 +0000",
          "deletions_count": 0,
          "description": "Fix log_to_metrics examples",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix log_to_metrics examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "67a0031a34ba9e94bb772c9fcc0c7d9e2f052507",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 16:45:45 +0000",
          "deletions_count": 114,
          "description": "Ensure \"How It Works\" sections are alphabetically sorted",
          "files_count": 33,
          "group": "docs",
          "insertions_count": 247,
          "message": "docs: Ensure \"How It Works\" sections are alphabetically sorted",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7f54fcd82f45adcf2b5fa29cc1e68b7b5b8fd292",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 16:41:36 +0000",
          "deletions_count": 389,
          "description": "Ensure docs links are relative",
          "files_count": 36,
          "group": "docs",
          "insertions_count": 618,
          "message": "docs: Ensure docs links are relative",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "bd54765b1d394bb072b42a2239673dc263f05ddc",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-08 12:35:12 +0000",
          "deletions_count": 1472,
          "description": "Add log_to_metric documentation",
          "files_count": 54,
          "group": "docs",
          "insertions_count": 2030,
          "message": "docs: Add log_to_metric documentation",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7c5743a9cc2913b337bfbe96f8b0767d49d8ade2",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-08 17:13:49 +0000",
          "deletions_count": 18,
          "description": "Add filename extension option and fix trailing slash",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 70,
          "message": "enhancement(aws_s3 sink): Add filename extension option and fix trailing slash (#596)",
          "pr_number": 596,
          "scope": {
            "category": "sink",
            "component_name": "aws_s3",
            "component_type": "sink",
            "name": "aws_s3 sink"
          },
          "sha": "8646a0104998dae7e341fe0a389ebdaaa181e6f1",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-08 16:47:14 +0000",
          "deletions_count": 75,
          "description": "Rename tracing crates",
          "files_count": 12,
          "group": "chore",
          "insertions_count": 94,
          "message": "chore: Rename tracing crates (#608)",
          "pr_number": 608,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "368b73a22db806b750dff44ed3e7aaac1859d467",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-08 14:37:49 +0000",
          "deletions_count": 0,
          "description": "Fix README",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 81,
          "message": "docs: Fix README (#610)",
          "pr_number": 610,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5021837ba934214b6f7ffa3720c7553c1b17179f",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-08 13:12:32 +0000",
          "deletions_count": 0,
          "description": "Initial rate limit subscriber",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 378,
          "message": "enhancement(observability): Initial rate limit subscriber (#494)",
          "pr_number": 494,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "6a9707d3c419aaa88d3f11a46acbf0e21c0c7bf6",
          "type": "enhancement"
        },
        {
          "author": "Andy Georges",
          "breaking_change": false,
          "date": "2019-07-08 18:41:38 +0000",
          "deletions_count": 1,
          "description": "Convert \"-\" into \"nil\"",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 24,
          "message": "enhancement(tokenizer transform): Convert \"-\" into \"nil\" (#580)",
          "pr_number": 580,
          "scope": {
            "category": "transform",
            "component_name": "tokenizer",
            "component_type": "transform",
            "name": "tokenizer transform"
          },
          "sha": "ac1f714f0ab8bcd2449cf763da66341f141a3b8e",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-07 21:22:29 +0000",
          "deletions_count": 309,
          "description": "Cleanup documentation headers",
          "files_count": 37,
          "group": "docs",
          "insertions_count": 726,
          "message": "docs: Cleanup documentation headers",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "971640c239451aea5d217e72d84a0221dc4b7117",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-07 22:39:47 +0000",
          "deletions_count": 7109,
          "description": "Move dynamically generated docs to ERB templates",
          "files_count": 149,
          "group": "docs",
          "insertions_count": 9434,
          "message": "docs: Move dynamically generated docs to ERB templates (#601)",
          "pr_number": 601,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6975b45c05db10550e7432a138dfe9144fd6f4b2",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-07-07 20:46:36 +0000",
          "deletions_count": 0,
          "description": "Add Ruby and Bundler 2 to development requirements",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: Add Ruby and Bundler 2 to development requirements (#600)",
          "pr_number": 600,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1d98e789c8db3cee3f45303ff73b102290ddbb97",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-04 18:51:05 +0000",
          "deletions_count": 9,
          "description": "Fix gauge misspelling",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Fix gauge misspelling (#594)",
          "pr_number": 594,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7e3cb94bacdbf26a7c0487f57696a46e420d8d2f",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-04 18:47:15 +0000",
          "deletions_count": 20,
          "description": "Fix include exclude",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 20,
          "message": "docs: Fix include exclude (#593)",
          "pr_number": 593,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "25ece4711cf918f321fc00e7d91efc5f582a69ef",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-04 18:37:54 +0000",
          "deletions_count": 6,
          "description": "Add env var example to add_fields documentation",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 18,
          "message": "docs: Add env var example to add_fields documentation",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8fac6fe083e4fdfee270cbf1be18ed7cd4eee9e9",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-04 17:15:45 +0000",
          "deletions_count": 136,
          "description": "Fix documentation array syntax",
          "files_count": 45,
          "group": "docs",
          "insertions_count": 136,
          "message": "docs: Fix documentation array syntax",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "894c9df97e881483ee48b4319813c9132344e46c",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-04 17:14:01 +0000",
          "deletions_count": 290,
          "description": "Resolve documentation typos and formatting issues",
          "files_count": 61,
          "group": "docs",
          "insertions_count": 875,
          "message": "docs: Resolve documentation typos and formatting issues",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e05314708498fa5d97054ff15510478f8aa66893",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-04 16:47:19 +0000",
          "deletions_count": 1132,
          "description": "Add check for pending documentation changes",
          "files_count": 83,
          "group": "docs",
          "insertions_count": 750,
          "message": "docs: Add check for pending documentation changes (#592)",
          "pr_number": 592,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b5c1cd7bad03ec37166d924b29dea17acc22b85a",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-04 12:47:37 +0000",
          "deletions_count": 27,
          "description": "Fix configuration documentation headings",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 27,
          "message": "docs: Fix configuration documentation headings (#591)",
          "pr_number": 591,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fbbf5d1d6a8dbd03208faa4fc5b3af577a97ac91",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-04 12:42:49 +0000",
          "deletions_count": 321,
          "description": "Cleanup documentation conventions",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 132,
          "message": "docs: Cleanup documentation conventions (#590)",
          "pr_number": 590,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e8682cc307ce3a74b719e809a388a20860aee658",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-04 11:05:10 +0000",
          "deletions_count": 1,
          "description": "Reduce test threads from 8 to 4",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(testing): Reduce test threads from 8 to 4 (#587)",
          "pr_number": 587,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "5f3a00216fecf17f44f3a5a6be032fe9e362bb3d",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-03 22:48:31 +0000",
          "deletions_count": 194,
          "description": "Rename tokio-trace to tracing",
          "files_count": 30,
          "group": "chore",
          "insertions_count": 197,
          "message": "chore: Rename tokio-trace to tracing (#578)",
          "pr_number": 578,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "92277fbfae7a1873a35ea75a725e9b71e963a0d5",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-03 18:13:59 +0000",
          "deletions_count": 3,
          "description": "Add make signoff command in pull request template",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore: Add make signoff command in pull request template",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4074d8430a183d3eaccca311044c3ad733785f57",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-03 15:43:04 +0000",
          "deletions_count": 65,
          "description": "Update Makefile and DEVELOPING.md",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 82,
          "message": "docs: Update Makefile and DEVELOPING.md (#570)",
          "pr_number": 570,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "35afcc8ee85d2d826bf4feb348bb1b5c5e15b781",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-03 15:28:24 +0000",
          "deletions_count": 10,
          "description": "Use MiB not mib in docs",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 10,
          "message": "docs: Use MiB not mib in docs (#577)",
          "pr_number": 577,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b57af065e88ff915ef9b8450114394063615a5f5",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-03 15:06:06 +0000",
          "deletions_count": 5,
          "description": "Link to License",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 5,
          "message": "docs: Link to License",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4fce85e98dac0d15edddc25adebe0db13b4c072f",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-07-03 14:54:46 +0000",
          "deletions_count": 39,
          "description": "Add DCO and update CONTRIBUTING.md",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 117,
          "message": "docs: Add DCO and update CONTRIBUTING.md (#571)",
          "pr_number": 571,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8de9ad3a22f0c4789a760b4f0e57a84163edddec",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-03 09:31:16 +0000",
          "deletions_count": 2,
          "description": "Fix tests",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(testing): Fix tests",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "b6316953a480a5ee161c6a61b33b4d33de23434d",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-07-03 09:22:28 +0000",
          "deletions_count": 19,
          "description": "Use floats for metrics values",
          "files_count": 4,
          "group": "enhancement",
          "insertions_count": 19,
          "message": "enhancement(metric data model): Use floats for metrics values (#553)",
          "pr_number": 553,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "16da8e55e7408473a15adf045de6bf9ebf6517af",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-02 07:04:39 +0000",
          "deletions_count": 7,
          "description": "output multiple metrics from a single log",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 57,
          "message": "enhancement(log_to_metric transform): output multiple metrics from a single log",
          "pr_number": null,
          "scope": {
            "category": "transform",
            "component_name": "log_to_metric",
            "component_type": "transform",
            "name": "log_to_metric transform"
          },
          "sha": "d8eadb08f469e7e411138ed9ff9e318bd4f9954c",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-06-27 17:07:11 +0000",
          "deletions_count": 5,
          "description": "adjust transform trait for multiple output events",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 14,
          "message": "enhancement(topology): adjust transform trait for multiple output events",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "topology"
          },
          "sha": "fe7f2b503443199a65a79dad129ed89ace3e287a",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-02 16:40:04 +0000",
          "deletions_count": 1,
          "description": "Remove makefile from list of languages",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: Remove makefile from list of languages",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5b58adb048b5740e5420255141f33a58e280852f",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-02 15:33:19 +0000",
          "deletions_count": 15,
          "description": "Use printf in the install.sh script",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 15,
          "message": "chore(operations): Use printf in the install.sh script",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "7c4b6488841b86c64ce41aadf7c1552a87b27d0a",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-07-02 12:07:53 +0000",
          "deletions_count": 1,
          "description": "Bump check-stable box size",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Bump check-stable box size (#555)",
          "pr_number": 555,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "7538d6eaae49666e4fc320a0f44425a69f789c38",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-07-02 08:46:47 +0000",
          "deletions_count": 6,
          "description": "make sure Cargo.lock gets updated on version bump",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 8,
          "message": "chore: make sure Cargo.lock gets updated on version bump",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a703de875fa7181c78d080509bbfed427a63fd11",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-02 02:04:50 +0000",
          "deletions_count": 2,
          "description": "Ensure new bumped version uses -dev",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: Ensure new bumped version uses -dev",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "15d6b26409761aa5eb15c70082fc02f83d1e949c",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-07-02 02:03:58 +0000",
          "deletions_count": 14,
          "description": "Start v0.4.0-dev",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 14,
          "message": "chore: Start v0.4.0-dev",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e53c86c0895ef0dfa48dbe8a4c572ea1c9d87a84",
          "type": "chore"
        },
        {
          "author": "Kirill Taran",
          "breaking_change": false,
          "date": "2019-09-13 00:44:30 +0000",
          "deletions_count": 123,
          "description": "add all parsed syslog fields to event",
          "files_count": 4,
          "group": "feat",
          "insertions_count": 322,
          "message": "feat(syslog source): add all parsed syslog fields to event (#836)",
          "pr_number": 836,
          "scope": {
            "category": "source",
            "component_name": "syslog",
            "component_type": "source",
            "name": "syslog source"
          },
          "sha": "27f79e2f8d5d99685bae8549d697355b77a0ad12",
          "type": "feat"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-13 09:50:18 +0000",
          "deletions_count": 9,
          "description": "log a single warning when ignoring small files",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 33,
          "message": "enhancement(file source): log a single warning when ignoring small files (#863)",
          "pr_number": 863,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "b9a7812e2e4cd7c7a7c87d77a84a3488b82b8f64",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-13 09:54:05 +0000",
          "deletions_count": 8,
          "description": "add logging when we can't tail file",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 11,
          "message": "chore: add logging when we can't tail file",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "65c189a6200f670c7faf1f6137e1e6ec77193bc5",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-13 15:05:10 +0000",
          "deletions_count": 43,
          "description": "Support AWS authentication",
          "files_count": 8,
          "group": "feat",
          "insertions_count": 270,
          "message": "feat(elasticsearch sink): Support AWS authentication (#864)",
          "pr_number": 864,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "95f7d345687737ba61ded2202196f4a40e3f8b85",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-14 10:47:10 +0000",
          "deletions_count": 50,
          "description": "add check_urls make argument",
          "files_count": 8,
          "group": "docs",
          "insertions_count": 50,
          "message": "docs: add check_urls make argument",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7cb7cf3efc5f64d926458fcacc8228ee543e203d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-14 10:50:03 +0000",
          "deletions_count": 6,
          "description": "create component md file if it does not yet exist, closes #849",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: create component md file if it does not yet exist, closes #849",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a4f963f3f7362c34335880659ea6d4a8c49d412f",
          "type": "docs"
        },
        {
          "author": "Matthias Endler",
          "breaking_change": false,
          "date": "2019-09-16 17:08:58 +0000",
          "deletions_count": 7,
          "description": "add split transform",
          "files_count": 17,
          "group": "feat",
          "insertions_count": 964,
          "message": "feat(new transform): add split transform (#850)",
          "pr_number": 850,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "35247a654181d1b3ace0309508707c6300b03561",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-16 11:12:47 +0000",
          "deletions_count": 1,
          "description": "ignore .tmp files",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: ignore .tmp files",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "662d74cce6fe8dbbbe4ff00e4cf61ef2d484676a",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-16 10:51:18 +0000",
          "deletions_count": 302,
          "description": "Error types",
          "files_count": 51,
          "group": "chore",
          "insertions_count": 627,
          "message": "chore: Error types (#811)",
          "pr_number": 811,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "aa74f1ec31764278a4dc53e9abdc53f52a742a89",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-16 13:52:51 +0000",
          "deletions_count": 2035,
          "description": "Move .metadata.toml to /.meta/*",
          "files_count": 53,
          "group": "docs",
          "insertions_count": 1873,
          "message": "docs: Move .metadata.toml to /.meta/* (#872)",
          "pr_number": 872,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1a90ce7182388de44bc5079cc1168842b5490168",
          "type": "docs"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-16 14:49:13 +0000",
          "deletions_count": 4,
          "description": "switch to more modern kafka image",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 10,
          "message": "chore: switch to more modern kafka image (#875)",
          "pr_number": 875,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "dadb904fda1681eec6d9063406fa2e43cfc7ba64",
          "type": "chore"
        },
        {
          "author": "Matthias Endler",
          "breaking_change": false,
          "date": "2019-09-16 22:18:33 +0000",
          "deletions_count": 6,
          "description": "Fix some typos in file-source crate",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore: Fix some typos in file-source crate (#871)",
          "pr_number": 871,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "503bbc0494eca9b2d62267b4a29adc3c2ce27ff4",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-17 11:08:09 +0000",
          "deletions_count": 11,
          "description": "Fix String error return in elasticsearch config parser",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 23,
          "message": "chore: Fix String error return in elasticsearch config parser (#883)",
          "pr_number": 883,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a97f2984778c4ffdf0412b16e27e43e9a32b2884",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-17 18:06:34 +0000",
          "deletions_count": 2042,
          "description": "Simpler, less noisy component options",
          "files_count": 88,
          "group": "docs",
          "insertions_count": 199,
          "message": "docs: Simpler, less noisy component options (#888)",
          "pr_number": 888,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f18796a35b9d61d3747386a0290c5ae50bc57310",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-18 10:24:07 +0000",
          "deletions_count": 94,
          "description": "Introduce crate-level `Result` type",
          "files_count": 41,
          "group": "chore",
          "insertions_count": 88,
          "message": "chore: Introduce crate-level `Result` type (#884)",
          "pr_number": 884,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ec73082da655d5e17c7023fef3b5c1893a4d7bf4",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-19 12:15:26 +0000",
          "deletions_count": 0,
          "description": "add commit types for semantic prs",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 8,
          "message": "chore: add commit types for semantic prs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "269c6054f7d74c11cf5a933f79f8966befa2c579",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-20 13:32:29 +0000",
          "deletions_count": 7,
          "description": "Add relese-meta make target for preparing release metadata",
          "files_count": 8,
          "group": "chore",
          "insertions_count": 355,
          "message": "chore: Add relese-meta make target for preparing release metadata (#898)",
          "pr_number": 898,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f9bf4bc05a1afd6d3861c96ba107e02120d447fa",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-20 14:45:09 +0000",
          "deletions_count": 297,
          "description": "automatically create missing component templates",
          "files_count": 81,
          "group": "docs",
          "insertions_count": 342,
          "message": "docs: automatically create missing component templates (#899)",
          "pr_number": 899,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6caa0f9fcc72c9becf2588b0839e2849c1d9b28e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-20 16:35:32 +0000",
          "deletions_count": 10,
          "description": "update checker docker image too include activesupport",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 13,
          "message": "chore(operations): update checker docker image too include activesupport",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2e5c0e0998d14f4e95397c92ffd92f85b54ff682",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-21 18:42:48 +0000",
          "deletions_count": 2694,
          "description": "Simplify link system and resolution",
          "files_count": 138,
          "group": "docs",
          "insertions_count": 2700,
          "message": "docs: Simplify link system and resolution (#901)",
          "pr_number": 901,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8c48932bb9cfd7267bf72bf260684d5fa93e8150",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-22 12:24:11 +0000",
          "deletions_count": 82,
          "description": "Generate CHANGELOG.md",
          "files_count": 20,
          "group": "chore",
          "insertions_count": 699,
          "message": "chore(operations): Generate CHANGELOG.md (#903)",
          "pr_number": 903,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c38f85c570194a5eb3e689c73550305e02a5bf1d",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-22 12:29:08 +0000",
          "deletions_count": 27,
          "description": "simplify readme installation links",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: simplify readme installation links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2776d7556176299e9090f319b6eca4bfcaa03b79",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-22 12:39:38 +0000",
          "deletions_count": 1,
          "description": "fix archive name for nightly builds",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore: fix archive name for nightly builds",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1dc0f93b0771cda8b075f0501151ab7d62247e29",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-22 15:31:27 +0000",
          "deletions_count": 7,
          "description": "dont upload version triple archives to s3",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): dont upload version triple archives to s3",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c2792e1c543e9a67782b5dd43d3c9ec6f0ac82db",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-22 15:44:19 +0000",
          "deletions_count": 27,
          "description": "use consistent archive names across all release channels",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 33,
          "message": "chore(operations): use consistent archive names across all release channels",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "dc2582b31eb1a7722c50d6eb7a6799ae04ec7f66",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-22 15:53:46 +0000",
          "deletions_count": 2,
          "description": "cleanup unused variables in release-s3.sh script",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: cleanup unused variables in release-s3.sh script",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2921e9a88e07e5a84294fdd36300c0cbf8bb294d",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-09-23 16:50:14 +0000",
          "deletions_count": 1,
          "description": "rename config tag",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(add_fields transform): rename config tag (#902)",
          "pr_number": 902,
          "scope": {
            "category": "transform",
            "component_name": "add_fields",
            "component_type": "transform",
            "name": "add_fields transform"
          },
          "sha": "a83a75003b41a881f87b7f2a053a9c43e040e1bc",
          "type": "fix"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-09-23 18:39:02 +0000",
          "deletions_count": 1,
          "description": "default config path \"/etc/vector/vector.toml\"",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 7,
          "message": "enhancement(config): default config path \"/etc/vector/vector.toml\" (#900)",
          "pr_number": 900,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "c830b956409b5f64d83c2ddd5056a5deaec1e609",
          "type": "enhancement"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-23 15:39:08 +0000",
          "deletions_count": 97,
          "description": "Add release-commit make target",
          "files_count": 31,
          "group": "chore",
          "insertions_count": 387,
          "message": "chore(operations): Add release-commit make target (#911)",
          "pr_number": 911,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2f187234ee024398997a6c4defac0ad38a234ac3",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-23 15:43:15 +0000",
          "deletions_count": 1,
          "description": "Remove $VERSION from package-deb",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(testing): Remove $VERSION from package-deb (#910)",
          "pr_number": 910,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "f942dfaca06a3de66ca593d99b5f04ccd4638e95",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-09-23 23:44:23 +0000",
          "deletions_count": 34,
          "description": "Use OpenSSL instead of LibreSSL for x86_64-unknown-linux-musl",
          "files_count": 4,
          "group": "fix",
          "insertions_count": 6,
          "message": "fix(operations): Use OpenSSL instead of LibreSSL for x86_64-unknown-linux-musl (#904)",
          "pr_number": 904,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8b2a11ee9ba0c3204deefa3d0435120873808089",
          "type": "fix"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-23 17:04:49 +0000",
          "deletions_count": 274,
          "description": "Remove ARMv7 support -- for now",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore(operations): Remove ARMv7 support -- for now (#913)",
          "pr_number": 913,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "d35ddfff2edbc4f776a75cc420f834a6f4d2aec4",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-23 21:15:14 +0000",
          "deletions_count": 63,
          "description": "Add libssl-dev to musl builder",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 63,
          "message": "chore(operations): Add libssl-dev to musl builder (#917)",
          "pr_number": 917,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "88769c9049da01560866a17f806403df46ca43fe",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-23 22:35:16 +0000",
          "deletions_count": 8,
          "description": "Remove $VERSION when building archives",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Remove $VERSION when building archives (#918)",
          "pr_number": 918,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "b0089e2509a5dc05155f4a11ed99439055b43eea",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-09-24 16:21:20 +0000",
          "deletions_count": 0,
          "description": "Use vendored OpenSSL for x86_64-unknown-linux-musl CI build",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Use vendored OpenSSL for x86_64-unknown-linux-musl CI build (#919)",
          "pr_number": 919,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "027836100a44874fc1989296f49777203f0a722a",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 09:59:45 +0000",
          "deletions_count": 6,
          "description": "add types to semantic.yml",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 20,
          "message": "docs: add types to semantic.yml",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4e256e2d3e9bd6aa91484f093b5b7fae894b9bf5",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-24 10:05:23 +0000",
          "deletions_count": 4,
          "description": "verify builds by default",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 32,
          "message": "chore(operations): verify builds by default (#914)",
          "pr_number": 914,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "230f3250cb1e109446ef017f82794466e3e070c2",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 10:11:23 +0000",
          "deletions_count": 3,
          "description": "use enhancement not improvement",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 11,
          "message": "docs: use enhancement not improvement",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f6b0739ebcabce1c768a2e3a97f2e6ee30119e4c",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 11:03:56 +0000",
          "deletions_count": 2,
          "description": "Prepare v0.4.0 release",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 671,
          "message": "chore: Prepare v0.4.0 release",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3450767465c7a58bb46631a8b922bb33d0b585c2",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-24 10:39:24 +0000",
          "deletions_count": 69,
          "description": "fix s3 compression and endpoint options",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 73,
          "message": "docs: fix s3 compression and endpoint options (#921)",
          "pr_number": 921,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c512e286e6a864911683bde5cdec4744f154966d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 12:21:29 +0000",
          "deletions_count": 0,
          "description": "update release-github to include release notes",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 106,
          "message": "chore(operations): update release-github to include release notes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "ced248773ab9a04d862a22dd4b80dfde5c9e8de3",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 12:24:53 +0000",
          "deletions_count": 116,
          "description": "use common setup.rb script for boiler plate setup",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 7,
          "message": "chore(operations): use common setup.rb script for boiler plate setup",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "3ea589a0d8ddc58d9b4caa167b0ef84fab99c84e",
          "type": "chore"
        }
      ],
      "compare_url": "https://github.com/timberio/vector/compare/v...v0.4.0",
      "date": "2019-09-24",
      "deletions_count": 8605,
      "insertions_count": 27640,
      "last_version": null,
      "posts": [

      ],
      "type": "initial dev",
      "type_url": "https://semver.org/#spec-item-4",
      "upgrade_guides": [

      ],
      "version": "0.4.0"
    },
    "0.5.0": {
      "commits": [
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 12:28:01 +0000",
          "deletions_count": 2,
          "description": "Update releaser Dockerfile to include Ruby and the necessary gems",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 10,
          "message": "chore(operations): Update releaser Dockerfile to include Ruby and the necessary gems",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "89c303748f100c881e6e1cb921e3d64870d89ca3",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-24 14:58:36 +0000",
          "deletions_count": 1,
          "description": "Add git to musl builder",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Add git to musl builder (#923)",
          "pr_number": 923,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5f251260ede2331a19e20d1319e9484bebd6f890",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 16:28:40 +0000",
          "deletions_count": 21,
          "description": "Fix github release notes",
          "files_count": 9,
          "group": "chore",
          "insertions_count": 51,
          "message": "chore(operations): Fix github release notes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "eab5a1a6c20ea7ec30b2e7f17c622d61e5f74613",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 16:46:51 +0000",
          "deletions_count": 12,
          "description": "Update release download URLs",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 22,
          "message": "docs: Update release download URLs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b88e0563acf439f1503c0380f2612fdf398ff134",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-24 15:03:33 +0000",
          "deletions_count": 23,
          "description": "Show information about why a retry needs to happen",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 36,
          "message": "enhancement(observability): Show information about why a retry needs to happen (#835)",
          "pr_number": 835,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "b2e4ccc78d8e8df3507abf3a3e2a9c44b3a37e7e",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-24 17:12:30 +0000",
          "deletions_count": 707,
          "description": "Make encoding non-optional",
          "files_count": 23,
          "group": "chore",
          "insertions_count": 542,
          "message": "chore(config): Make encoding non-optional (#894)",
          "pr_number": 894,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "b25a22e71417df6bb3889f6ff1208cbf6f73232f",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 17:58:04 +0000",
          "deletions_count": 82,
          "description": "add version to readme",
          "files_count": 10,
          "group": "docs",
          "insertions_count": 146,
          "message": "docs: add version to readme",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "222fe08358566f677e342e9553ce5421597cdfaa",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 18:08:03 +0000",
          "deletions_count": 3,
          "description": "Update installation readme link",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: Update installation readme link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7c10d204cd0cf821a38f3ae6f903f346d94a1d87",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 20:39:03 +0000",
          "deletions_count": 46,
          "description": "Recommend a new version based on pending commits",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 81,
          "message": "chore(operations): Recommend a new version based on pending commits",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "f8ba06b75daf3d7d3be9c47d9762b8ec8dae7c55",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-24 23:02:18 +0000",
          "deletions_count": 25,
          "description": "Use proper category in changelog for new components",
          "files_count": 19,
          "group": "docs",
          "insertions_count": 461,
          "message": "docs: Use proper category in changelog for new components",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "597f989dc0900c08b099f62107ce53a5508e9933",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-09-25 09:57:05 +0000",
          "deletions_count": 1,
          "description": "Initial `statsd` implementation",
          "files_count": 16,
          "group": "feat",
          "insertions_count": 776,
          "message": "feat(new sink): Initial `statsd` implementation (#821)",
          "pr_number": 821,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "55582e52e1e8856b75702ffce6b56218ac82ddaf",
          "type": "feat"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-25 09:54:36 +0000",
          "deletions_count": 18,
          "description": "Fix incorrect description of kafka option",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 14,
          "message": "docs: Fix incorrect description of kafka option (#926)",
          "pr_number": 926,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "eff3bf23a9dbdbf1c01b2744ad0a489542533841",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-09-26 00:08:25 +0000",
          "deletions_count": 4,
          "description": "Add OpenSSL to x86_64-unknown-linux-musl buil…",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 53,
          "message": "chore(operations): Add OpenSSL to x86_64-unknown-linux-musl buil… (#927)",
          "pr_number": 927,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "dd74e64f1d00c1032a7a470f40f4b7aea57b1d86",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-25 16:01:03 +0000",
          "deletions_count": 10,
          "description": "Add support for TLS (SSL)",
          "files_count": 14,
          "group": "feat",
          "insertions_count": 409,
          "message": "feat(kafka sink): Add support for TLS (SSL) (#912)",
          "pr_number": 912,
          "scope": {
            "category": "sink",
            "component_name": "kafka",
            "component_type": "sink",
            "name": "kafka sink"
          },
          "sha": "630d841a4dce90df195abfab53722f61b8b192a2",
          "type": "feat"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-09-25 18:34:20 +0000",
          "deletions_count": 23,
          "description": "Use PKCS#12 keys instead of JKS",
          "files_count": 6,
          "group": "feat",
          "insertions_count": 64,
          "message": "feat(kafka sink): Use PKCS#12 keys instead of JKS (#934)",
          "pr_number": 934,
          "scope": {
            "category": "sink",
            "component_name": "kafka",
            "component_type": "sink",
            "name": "kafka sink"
          },
          "sha": "43d04fc4b5a9855c936b5c63e470c3b78206b227",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-25 20:45:45 +0000",
          "deletions_count": 2,
          "description": "Fix nightly builds link",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix nightly builds link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5fa0161e537f33010e8116cb5c6782c721701c29",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-26 08:52:41 +0000",
          "deletions_count": 0,
          "description": "Create SECURITY.md",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 7,
          "message": "docs: Create SECURITY.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "880d6401ac9705760e140dfb2537646078fb3eb0",
          "type": "docs"
        },
        {
          "author": "Cédric Da Fonseca",
          "breaking_change": false,
          "date": "2019-09-26 14:53:56 +0000",
          "deletions_count": 2,
          "description": "Fix install script path export",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 9,
          "message": "chore(operations): Fix install script path export (#891)",
          "pr_number": 891,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "af77005bc7cbf908c271a826e4cd5caee7b45072",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 09:02:45 +0000",
          "deletions_count": 50,
          "description": "Simplify changelog TOC",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 15,
          "message": "docs: Simplify changelog TOC",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6353e49126dc5f575194783870ab06f1e9e3354a",
          "type": "docs"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-26 09:57:05 +0000",
          "deletions_count": 1,
          "description": "Update to rust 1.38.0",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore: Update to rust 1.38.0",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ec7e488213fc8e9c04798174a00318aa3d9b84b8",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-26 10:15:13 +0000",
          "deletions_count": 3,
          "description": "Fix fmt errors for 1.38.0",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore: Fix fmt errors for 1.38.0",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "dda0309ced633cfd0a7b810c19733e02e8f09fbe",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-09-26 17:30:51 +0000",
          "deletions_count": 200,
          "description": "Improve installation docs",
          "files_count": 23,
          "group": "docs",
          "insertions_count": 419,
          "message": "docs: Improve installation docs (#942)",
          "pr_number": 942,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "da8802836f5b9085c776eeb80d12d2c9fa1ab266",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 17:40:46 +0000",
          "deletions_count": 54,
          "description": "Link to README.md file in SUMMARY.md",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 59,
          "message": "docs: Link to README.md file in SUMMARY.md",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "05395b070b3eb3cf4f32f61423aae99ad26dc773",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 17:57:02 +0000",
          "deletions_count": 241,
          "description": "Fix broken docs links",
          "files_count": 27,
          "group": "docs",
          "insertions_count": 63,
          "message": "docs: Fix broken docs links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9f1c2b78847d0f0122ea1f8e6c9e2f93db0053f8",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 18:00:13 +0000",
          "deletions_count": 5,
          "description": "Ensure .rpm packages are built in nightly builds",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 21,
          "message": "chore(operations): Ensure .rpm packages are built in nightly builds",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "0f8aaecea209105a58693f0360c43d08fd594263",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 18:04:34 +0000",
          "deletions_count": 2,
          "description": "fix broken tabs on yum and apt pages",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: fix broken tabs on yum and apt pages",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "daa7ce711f6b3d50a4e1a75eda15ba0d8bd95973",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 18:08:59 +0000",
          "deletions_count": 11,
          "description": "fix download links for deb and rpm packages",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 218,
          "message": "docs: fix download links for deb and rpm packages",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "87737e23261437d7f1bdb0bba0662cfd3884098e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-26 18:18:56 +0000",
          "deletions_count": 3,
          "description": "Update SECURITY.md with better info",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 25,
          "message": "docs: Update SECURITY.md with better info",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "296411c14eceb799c26ac478aee9f6d302bea515",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-26 23:02:02 +0000",
          "deletions_count": 80,
          "description": "Docker images use binaries",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 29,
          "message": "chore(operations): Docker images use binaries (#940)",
          "pr_number": 940,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "61520b2cfabb8c3345dcf896df620906ceb55d4c",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-27 11:28:24 +0000",
          "deletions_count": 2,
          "description": "Remove setting VERSION for `make generate`",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Remove setting VERSION for `make generate`",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f6e1050d8a7fdd41a844ae9ba496ad1cd2bb10ce",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-09-27 16:07:58 +0000",
          "deletions_count": 0,
          "description": "Add `fix` as a valid PR type",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore: Add `fix` as a valid PR type",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "089bb5a2a4fbc8fa1522781b0982a9a9ca58e479",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-09-28 19:08:07 +0000",
          "deletions_count": 3,
          "description": "Clean up debian user creation and unit file",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore(operations): Clean up debian user creation and unit file (#947)",
          "pr_number": 947,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "4c9917754edd71a4ef53b9778d4540e3736d0abb",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-09-30 18:08:46 +0000",
          "deletions_count": 34,
          "description": "Update tokio versions",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 70,
          "message": "chore(operations): Update tokio versions (#949)",
          "pr_number": 949,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "7653c6bbd61f3859d651d6cab21e43d5612cf6c7",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-01 18:20:10 +0000",
          "deletions_count": 7,
          "description": "Use stable Rust 1.38.0 and update Linux headers for x86_6…",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore: Use stable Rust 1.38.0 and update Linux headers for x86_6… (#945)",
          "pr_number": 945,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "07339e111633e783c71484c83f8f0193a9167716",
          "type": "chore"
        },
        {
          "author": "Fernando Schuindt",
          "breaking_change": false,
          "date": "2019-10-02 00:35:52 +0000",
          "deletions_count": 2,
          "description": "Tarball URL address for the Linux installation script",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Tarball URL address for the Linux installation script (#957)",
          "pr_number": 957,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "00818013b6d8a9acfa578ce80a2ef5fa5cf9505d",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-02 11:53:44 +0000",
          "deletions_count": 213,
          "description": "Add support for TLS options",
          "files_count": 14,
          "group": "feat",
          "insertions_count": 575,
          "message": "feat(elasticsearch sink): Add support for TLS options (#953)",
          "pr_number": 953,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "8f185695e084b1be5da753e9fca2c831cace3bac",
          "type": "feat"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-02 13:59:12 +0000",
          "deletions_count": 4,
          "description": "Ensure released s3 files are public-read",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 12,
          "message": "chore(operations): Ensure released s3 files are public-read (#959)",
          "pr_number": 959,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2d5736e8f1e57ffe573c07cfcdc77e0c67dc84e9",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-02 17:19:40 +0000",
          "deletions_count": 140,
          "description": "Sync and verify install.sh",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 428,
          "message": "chore(operations): Sync and verify install.sh (#958)",
          "pr_number": 958,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "e1608701e298baf2d452689ca9fec9f1f0fb4c02",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-02 17:42:08 +0000",
          "deletions_count": 265,
          "description": "Remove APT, YUM, and PackageCloud",
          "files_count": 21,
          "group": "docs",
          "insertions_count": 61,
          "message": "docs: Remove APT, YUM, and PackageCloud (#961)",
          "pr_number": 961,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f20d68a6ba153df599e525f15e18baebe624585f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-02 22:35:35 +0000",
          "deletions_count": 1,
          "description": "Add SSE and public-read ACL to install.sh",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Add SSE and public-read ACL to install.sh",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "ce2174996a583ae27b8c04b998f59abc47f5634a",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-03 11:09:26 +0000",
          "deletions_count": 0,
          "description": "Verify installation script on mac",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 15,
          "message": "chore(operations): Verify installation script on mac (#965)",
          "pr_number": 965,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8dae324901f7fb4913ca68e723d6aeea814e76f3",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-03 11:57:52 +0000",
          "deletions_count": 1,
          "description": "Verify that sh.vector.dev works",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 22,
          "message": "chore(operations): Verify that sh.vector.dev works (#964)",
          "pr_number": 964,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "eebcbd4c1fa1296a0bfe152a2141c253cbb76d88",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-03 12:11:17 +0000",
          "deletions_count": 0,
          "description": "Create missing .md file for new components",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 7,
          "message": "docs: Create missing .md file for new components",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "eedbb2c650f75406b86a9da9f1d7de48550dcf7e",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-04 11:18:38 +0000",
          "deletions_count": 7,
          "description": "Verify and check Homebrew install",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 26,
          "message": "chore(operations): Verify and check Homebrew install (#969)",
          "pr_number": 969,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5c5ad89d74a7ec0069e0b41ba8adccc20b5ecf55",
          "type": "chore"
        },
        {
          "author": "albert",
          "breaking_change": false,
          "date": "2019-10-04 23:30:25 +0000",
          "deletions_count": 4,
          "description": "Add support for basic auth",
          "files_count": 5,
          "group": "feat",
          "insertions_count": 100,
          "message": "feat(clickhouse sink): Add support for basic auth (#937)",
          "pr_number": 937,
          "scope": {
            "category": "sink",
            "component_name": "clickhouse",
            "component_type": "sink",
            "name": "clickhouse sink"
          },
          "sha": "d5974dc4198abd22bf6b920fc380a087cc150137",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-05 13:10:20 +0000",
          "deletions_count": 1,
          "description": "Use sudo when checking internet install",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Use sudo when checking internet install",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "c216022f600dbdf7aec8f8bb2fd7e9320584ed16",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-10-07 16:42:28 +0000",
          "deletions_count": 6,
          "description": "Update cloudwatch metrics docs",
          "files_count": 8,
          "group": "docs",
          "insertions_count": 99,
          "message": "docs: Update cloudwatch metrics docs (#968)",
          "pr_number": 968,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "7449992216e3c8812f2ed24d4ddda11c799e50e9",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-07 20:22:14 +0000",
          "deletions_count": 19,
          "description": "Properly verify that the Vector Systemd service started",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 25,
          "message": "chore(operations): Properly verify that the Vector Systemd service started (#982)",
          "pr_number": 982,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2e6516f844247a18a7885dfeafc5f4d118687845",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-07 22:33:01 +0000",
          "deletions_count": 1,
          "description": "Dont auto-update when testing Homebrew install",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Dont auto-update when testing Homebrew install",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8c2ef3e5412159289be79a5521ffd43c65be812b",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-08 00:31:46 +0000",
          "deletions_count": 169,
          "description": "Fix Docker builds",
          "files_count": 20,
          "group": "chore",
          "insertions_count": 430,
          "message": "chore(operations): Fix Docker builds (#985)",
          "pr_number": 985,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8a6705aefbf8e00c99107ba5038cb1022d85cd7e",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 00:37:07 +0000",
          "deletions_count": 1,
          "description": "Fix failing verify-install-on-internet check",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Fix failing verify-install-on-internet check",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8cc531da55aa1d948e785af0dec1ba74bef165e0",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 00:38:40 +0000",
          "deletions_count": 2,
          "description": "Fix vector docker image name reference",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix vector docker image name reference",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "76e396aeb5d89f227b25467fdb86293e5e5c1a95",
          "type": "docs"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-10-08 17:18:19 +0000",
          "deletions_count": 9,
          "description": "Initial `docker` source implementation",
          "files_count": 23,
          "group": "feat",
          "insertions_count": 1537,
          "message": "feat(new source): Initial `docker` source implementation (#787)",
          "pr_number": 787,
          "scope": {
            "category": "source",
            "component_name": null,
            "component_type": "source",
            "name": "new source"
          },
          "sha": "ddc27bb670e86713c03554ffe081dd1e873a7de9",
          "type": "feat"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-08 11:33:32 +0000",
          "deletions_count": 540,
          "description": "Unify the different TLS options",
          "files_count": 25,
          "group": "enhancement",
          "insertions_count": 905,
          "message": "enhancement(security): Unify the different TLS options (#972)",
          "pr_number": 972,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "security"
          },
          "sha": "74b654606b39a7554c53c07b585d0cd9be3b76f7",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-10-08 16:08:22 +0000",
          "deletions_count": 1,
          "description": "Default data_dir to /var/lib/vector",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 8,
          "message": "fix(config): Default data_dir to /var/lib/vector (#995)",
          "pr_number": 995,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "476fb7e436f1b285ccff3dc52e21a8b1f36ab458",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-10-08 18:21:36 +0000",
          "deletions_count": 630,
          "description": "Add rate limited debug messages",
          "files_count": 33,
          "group": "feat",
          "insertions_count": 422,
          "message": "feat(observability): Add rate limited debug messages (#971)",
          "pr_number": 971,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "b541bb1a4097d22f3efa9d74ccaf28cabcbe6466",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 18:33:39 +0000",
          "deletions_count": 2,
          "description": "Fix release script bug",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Fix release script bug",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "389a65072cea2b7d3bafe70a52597d83925251e6",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 18:48:29 +0000",
          "deletions_count": 204,
          "description": "Prepare v0.5.0 release",
          "files_count": 10,
          "group": "chore",
          "insertions_count": 55,
          "message": "chore: Prepare v0.5.0 release",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3a86fdae3f5d72d001ba16b9683514e571a7c105",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 19:06:31 +0000",
          "deletions_count": 0,
          "description": "Add 0.5.0 release metadata",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 61,
          "message": "chore: Add 0.5.0 release metadata",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "df6018be6c1c964692d3ea071f4d95fb21f1cb14",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-08 20:27:20 +0000",
          "deletions_count": 6,
          "description": "Remove unsupported bash flags",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore: Remove unsupported bash flags",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5a4d50b022db116a0155efafb6aaaa34e4882600",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-09 11:36:55 +0000",
          "deletions_count": 2,
          "description": "Add sudo when installing via dpkg or rpm",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 10,
          "message": "chore(operations): Add sudo when installing via dpkg or rpm (#999)",
          "pr_number": 999,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "86d1d01bed23aa1496dcdab9c627d90c6c07e294",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-09 11:38:01 +0000",
          "deletions_count": 1,
          "description": "Add git to musl build image",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Add git to musl build image (#997)",
          "pr_number": 997,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a70603d11764ca49c4aa62bf3e50f7cf712c0018",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-09 15:52:26 +0000",
          "deletions_count": 107,
          "description": "Fix centos verifier",
          "files_count": 15,
          "group": "chore",
          "insertions_count": 37,
          "message": "chore(operations): Fix centos verifier (#1001)",
          "pr_number": 1001,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2cb0e44c69c776679dc19d41af8aecee42169e75",
          "type": "chore"
        }
      ],
      "compare_url": "https://github.com/timberio/vector/compare/v0.4.0...v0.5.0",
      "date": "2019-10-09",
      "deletions_count": 3038,
      "insertions_count": 6839,
      "last_version": "0.4.0",
      "posts": [

      ],
      "type": "initial dev",
      "type_url": "https://semver.org/#spec-item-4",
      "upgrade_guides": [

      ],
      "version": "0.5.0"
    },
    "0.6.0": {
      "commits": [
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-10 15:01:52 +0000",
          "deletions_count": 1,
          "description": "Push docker images so that `latest` tags are last",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Push docker images so that `latest` tags are last",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "15b44d04a06c91d5e0d1017b251c32ac165f2bd6",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-10 15:19:21 +0000",
          "deletions_count": 1,
          "description": "Print grease command output",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Print grease command output",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "4bc7696077e691f59811e8b1e078f1b029fe63a6",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-11 09:58:21 +0000",
          "deletions_count": 7,
          "description": "Do not release Github or Homebrew on nightly",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore(operations): Do not release Github or Homebrew on nightly",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "0f5266193c6ae8d7d47907c906e34598e36f2057",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-10-11 09:08:43 +0000",
          "deletions_count": 40,
          "description": "Make global options actually use default",
          "files_count": 6,
          "group": "fix",
          "insertions_count": 56,
          "message": "fix(cli): Make global options actually use default (#1013)",
          "pr_number": 1013,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "1e1d66e04722841e3e0dc9b6d7d85c75379d1caf",
          "type": "fix"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-11 10:23:18 +0000",
          "deletions_count": 2,
          "description": "Add relevant when details to config spec",
          "files_count": 17,
          "group": "docs",
          "insertions_count": 74,
          "message": "docs: Add relevant when details to config spec (#1016)",
          "pr_number": 1016,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "a7f7ffa879cd310beca498a600537707b7aee896",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-11 12:26:22 +0000",
          "deletions_count": 3683,
          "description": "List out component options as linkable sections",
          "files_count": 95,
          "group": "docs",
          "insertions_count": 3115,
          "message": "docs: List out component options as linkable sections (#1019)",
          "pr_number": 1019,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1f0c52bcb931bd2e10fa09557e343af50513e166",
          "type": "docs"
        },
        {
          "author": "Lincoln Lee",
          "breaking_change": false,
          "date": "2019-10-14 02:13:53 +0000",
          "deletions_count": 0,
          "description": "Add ca certificates for docker image",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 2,
          "message": "fix(docker platform): Add ca certificates for docker image (#1014)",
          "pr_number": 1014,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "docker platform"
          },
          "sha": "5510b176ce0645d9893ea0e92ac2f73d58515e38",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-13 18:50:02 +0000",
          "deletions_count": 3593,
          "description": "Further improve options documentation for each component",
          "files_count": 122,
          "group": "docs",
          "insertions_count": 3957,
          "message": "docs: Further improve options documentation for each component",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "d4aac2e13c8c3f285cfeb95a6c22695fe07cb18e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-13 18:53:10 +0000",
          "deletions_count": 456,
          "description": "Remove superflous tags in config examples",
          "files_count": 42,
          "group": "docs",
          "insertions_count": 458,
          "message": "docs: Remove superflous tags in config examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "21506409f8bf1311dfb4cd7ce8539d049dd4a5cd",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-13 19:47:18 +0000",
          "deletions_count": 480,
          "description": "Dont repeat default value in configuration examples",
          "files_count": 45,
          "group": "docs",
          "insertions_count": 468,
          "message": "docs: Dont repeat default value in configuration examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "aa02c432cca22a9fd8f7425c839156f2613e3e7b",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-10-14 15:10:55 +0000",
          "deletions_count": 1,
          "description": "Initial `datadog_metrics` implementation",
          "files_count": 16,
          "group": "feat",
          "insertions_count": 1085,
          "message": "feat(new sink): Initial `datadog_metrics` implementation (#967)",
          "pr_number": 967,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "d04a3034e3a6ea233be44ddaf59e07c6340d5824",
          "type": "feat"
        },
        {
          "author": "Lincoln Lee",
          "breaking_change": false,
          "date": "2019-10-15 01:43:09 +0000",
          "deletions_count": 1,
          "description": "Remove debian cache to reduce image size",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Remove debian cache to reduce image size (#1028)",
          "pr_number": 1028,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "1378575334e0032de645c8277683f73cf640eb97",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-13 19:49:38 +0000",
          "deletions_count": 76,
          "description": "Dont label unit in config examples",
          "files_count": 20,
          "group": "docs",
          "insertions_count": 80,
          "message": "docs: Dont label unit in config examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c1b36be946a2103a6c5eff77e288f32898a3bbe3",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-14 19:25:25 +0000",
          "deletions_count": 334,
          "description": "Add back section references to option descriptions",
          "files_count": 45,
          "group": "docs",
          "insertions_count": 348,
          "message": "docs: Add back section references to option descriptions",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "571e1390bd4a5455a5b1305ace8fd1724a761ddd",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-15 12:31:14 +0000",
          "deletions_count": 5,
          "description": "Ensure log_to_metric tags option shows in example",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Ensure log_to_metric tags option shows in example",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "22efd48c90d91c9fa9a4d102e54ffb3d869945f3",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-15 12:32:52 +0000",
          "deletions_count": 1,
          "description": "Fix metrics examples syntax error",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Fix metrics examples syntax error",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5dd167a462930da589f842a366334d65be17d185",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-15 12:36:11 +0000",
          "deletions_count": 1,
          "description": "Fix log data model",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix log data model",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f804cebad4ed97f0da105effbe72b593a846ff9d",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-16 15:30:34 +0000",
          "deletions_count": 5,
          "description": "Add `commit_interval_ms` option",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 17,
          "message": "enhancement(kafka source): Add `commit_interval_ms` option (#944)",
          "pr_number": 944,
          "scope": {
            "category": "source",
            "component_name": "kafka",
            "component_type": "source",
            "name": "kafka source"
          },
          "sha": "a3c7c752e3fec7d3c5d84d4452e1243b263a3ae8",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-10-16 19:19:15 +0000",
          "deletions_count": 8,
          "description": "Fix typos",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Fix typos",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8aaa22524c13a184a8ce0c8eeaa744d556ed4841",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-10-17 14:38:27 +0000",
          "deletions_count": 0,
          "description": "Put buffering tests behind `leveldb` feature",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(testing): Put buffering tests behind `leveldb` feature (#1046)",
          "pr_number": 1046,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "20bc1a29af0ad4cab9f86482873e942627d366c2",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-10-17 15:45:52 +0000",
          "deletions_count": 3,
          "description": "Update `tower-limit` to `v0.1.1`",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(operations): Update `tower-limit` to `v0.1.1` (#1018)",
          "pr_number": 1018,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "22fd9ef6f07b4372512185270b729ad0fd21b49c",
          "type": "chore"
        },
        {
          "author": "AlyHKafoury",
          "breaking_change": false,
          "date": "2019-10-17 22:47:58 +0000",
          "deletions_count": 17,
          "description": "Resolve inability to shutdown Vector when std…",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 39,
          "message": "fix(stdin source): Resolve inability to shutdown Vector when std… (#960)",
          "pr_number": 960,
          "scope": {
            "category": "source",
            "component_name": "stdin",
            "component_type": "source",
            "name": "stdin source"
          },
          "sha": "32ed04fb529fcb6a10dfed101dff04447357cf13",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-10-17 18:41:54 +0000",
          "deletions_count": 0,
          "description": "Add address and path to the syslog source example config",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Add address and path to the syslog source example config",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3e8c906e791505732cea3608fbac9c1878a141bd",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-10-18 13:04:52 +0000",
          "deletions_count": 0,
          "description": "Bump version in Cargo.toml before releasing",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 23,
          "message": "chore(operations): Bump version in Cargo.toml before releasing (#1048)",
          "pr_number": 1048,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "fe26627b13797465d7a94a7ea1e63a7266bf7d42",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-18 22:15:06 +0000",
          "deletions_count": 3,
          "description": "Update leveldb-sys up to 2.0.5",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 3,
          "message": "enhancement(platforms): Update leveldb-sys up to 2.0.5 (#1055)",
          "pr_number": 1055,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "platforms"
          },
          "sha": "875de183748ba7939f53d1c712f1ea1aff7017a8",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-10-21 14:19:44 +0000",
          "deletions_count": 204,
          "description": "Apply some fixes for clippy lints",
          "files_count": 36,
          "group": "chore",
          "insertions_count": 188,
          "message": "chore: Apply some fixes for clippy lints (#1034)",
          "pr_number": 1034,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b2a3c25bbf9e33a9d167eef1ca28d606f405b670",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": true,
          "date": "2019-10-21 16:54:41 +0000",
          "deletions_count": 61,
          "description": "Require `encoding` option for console and file sinks",
          "files_count": 17,
          "group": "breaking change",
          "insertions_count": 116,
          "message": "fix(config)!: Require `encoding` option for console and file sinks (#1033)",
          "pr_number": 1033,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "616d14abf59ac6e29c356fbf43e108dd7a438d35",
          "type": "fix"
        },
        {
          "author": "Yeonghoon Park",
          "breaking_change": false,
          "date": "2019-10-23 06:22:55 +0000",
          "deletions_count": 5,
          "description": "Bundle install should print output on error",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(operations): Bundle install should print output on error (#1068)",
          "pr_number": 1068,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "b6a8778949d9fbb36637bec13bf9a9b03762663b",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-22 16:32:08 +0000",
          "deletions_count": 70,
          "description": "Add support for systemd socket activation",
          "files_count": 23,
          "group": "enhancement",
          "insertions_count": 199,
          "message": "enhancement(networking): Add support for systemd socket activation (#1045)",
          "pr_number": 1045,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "networking"
          },
          "sha": "f90f50abec9f5848b12c216e2962ad45f1a87652",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-23 15:08:45 +0000",
          "deletions_count": 2,
          "description": "Add OpenSSL and pkg-config to development requirements",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Add OpenSSL and pkg-config to development requirements (#1066)",
          "pr_number": 1066,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "afc1edab8b726291850674d6fbbf7c66af2ba6aa",
          "type": "docs"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-10-23 18:27:01 +0000",
          "deletions_count": 1,
          "description": "Set default `drop_field` to true",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 13,
          "message": "enhancement(regex_parser transform): Set default `drop_field` to true",
          "pr_number": null,
          "scope": {
            "category": "transform",
            "component_name": "regex_parser",
            "component_type": "transform",
            "name": "regex_parser transform"
          },
          "sha": "e56f9503f09a7f97d96093775856a019d738d402",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-10-24 09:02:53 +0000",
          "deletions_count": 83,
          "description": "Add `validate` sub command",
          "files_count": 5,
          "group": "feat",
          "insertions_count": 269,
          "message": "feat(cli): Add `validate` sub command (#1064)",
          "pr_number": 1064,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "018db5f4c65662367cc749f3e4458271a2003e75",
          "type": "feat"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-10-24 12:11:00 +0000",
          "deletions_count": 136,
          "description": "Metrics buffer & aggregation",
          "files_count": 7,
          "group": "enhancement",
          "insertions_count": 875,
          "message": "enhancement(metric data model): Metrics buffer & aggregation (#930)",
          "pr_number": 930,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "c112c4ac7f45e69fea312e7691566a3f9e8e3066",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-24 14:57:57 +0000",
          "deletions_count": 127,
          "description": "Use rdkafka crate from the upstream Git repository",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 118,
          "message": "chore(operations): Use rdkafka crate from the upstream Git repository (#1063)",
          "pr_number": 1063,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "137e9ea7495eabca272207a904b9dd4c2f82d6af",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-10-24 15:13:08 +0000",
          "deletions_count": 635,
          "description": "Check config examples",
          "files_count": 37,
          "group": "chore",
          "insertions_count": 18,
          "message": "chore(config): Check config examples (#1082)",
          "pr_number": 1082,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "4cde6dc5021d06e07393af135d0625178385802a",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-24 11:52:44 +0000",
          "deletions_count": 12,
          "description": "Fix a couple minor issues with checkpointing",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 17,
          "message": "fix(journald source): Fix a couple minor issues with checkpointing (#1086)",
          "pr_number": 1086,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "ef5ec5732fd4f677f0b25e3f6e470c37d0f73855",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-24 13:17:07 +0000",
          "deletions_count": 1,
          "description": "Fix merge problem in Cargo.lock",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Fix merge problem in Cargo.lock (#1087)",
          "pr_number": 1087,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8fef7056a1d1c515014e721a2940d04ff269a704",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-10-25 09:40:42 +0000",
          "deletions_count": 17,
          "description": "Use metric buffer in Datadog sink",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 17,
          "message": "enhancement(datadog_metrics sink): Use metric buffer in Datadog sink (#1080)",
          "pr_number": 1080,
          "scope": {
            "category": "sink",
            "component_name": "datadog_metrics",
            "component_type": "sink",
            "name": "datadog_metrics sink"
          },
          "sha": "c97173fb472ffeb11902e3385dc212fdef8a0ffa",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-28 14:20:14 +0000",
          "deletions_count": 6,
          "description": "Update `ctor` dependency",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 6,
          "message": "chore(operations): Update `ctor` dependency (#1095)",
          "pr_number": 1095,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "25813de321b097677e7c23069082b8e3597928e8",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-28 14:50:20 +0000",
          "deletions_count": 1,
          "description": "Avoid dependency on platform-specific machine word size",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Avoid dependency on platform-specific machine word size (#1096)",
          "pr_number": 1096,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "ccae97b37b04b590ddf64284fd593afdfb024b22",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-28 15:02:09 +0000",
          "deletions_count": 13,
          "description": "Rework option to limit records to current boot in journald source",
          "files_count": 7,
          "group": "fix",
          "insertions_count": 36,
          "message": "fix(journald source): Rework option to limit records to current boot in journald source (#1105)",
          "pr_number": 1105,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "7ca6dc31a3af3e6e08ef89a469923fa385e5df30",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-10-28 18:34:13 +0000",
          "deletions_count": 7,
          "description": "Wrap provider call with a tokio runtime",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 11,
          "message": "enhancement(elasticsearch sink): Wrap provider call with a tokio runtime (#1104)",
          "pr_number": 1104,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "f9a6776a4467cd8a5c4ffdaa44a8a5593f6471ac",
          "type": "enhancement"
        },
        {
          "author": "David O'Rourke",
          "breaking_change": false,
          "date": "2019-10-29 17:26:32 +0000",
          "deletions_count": 77,
          "description": "Update Rusoto to 0.38.0",
          "files_count": 8,
          "group": "chore",
          "insertions_count": 80,
          "message": "chore(operations): Update Rusoto to 0.38.0 (#1112)",
          "pr_number": 1112,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "603f1e3331e44c2b486cb8f5570109987b0a261e",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-29 20:30:57 +0000",
          "deletions_count": 1,
          "description": "Increase sleep interval in the tests for file source",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(file source): Increase sleep interval in the tests for file source (#1113)",
          "pr_number": 1113,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "9e2f98e780fdca4380f701508eb6f35e924d8d8b",
          "type": "chore"
        },
        {
          "author": "David O'Rourke",
          "breaking_change": false,
          "date": "2019-10-29 18:01:52 +0000",
          "deletions_count": 116,
          "description": "Update Rusoto to 0.41.x",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 79,
          "message": "chore(operations): Update Rusoto to 0.41.x (#1114)",
          "pr_number": 1114,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "539f7086459692fe8b52493cdf053220af687d92",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-29 14:42:21 +0000",
          "deletions_count": 12,
          "description": "Cursor/checkpoint fixes",
          "files_count": 5,
          "group": "fix",
          "insertions_count": 77,
          "message": "fix(journald source): Cursor/checkpoint fixes (#1106)",
          "pr_number": 1106,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "ddffd3b91588da87b3c3a1623ac1f7be842f2392",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-30 20:12:56 +0000",
          "deletions_count": 6,
          "description": "Use `rlua` crate from a fork with Pairs implementation",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 16,
          "message": "chore(operations): Use `rlua` crate from a fork with Pairs implementation (#1119)",
          "pr_number": 1119,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a5d442c9d311fb100d1912d5a0c422a847dbbdc3",
          "type": "chore"
        },
        {
          "author": "Steven Aerts",
          "breaking_change": false,
          "date": "2019-10-30 18:13:29 +0000",
          "deletions_count": 0,
          "description": "Allow iteration over fields",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 61,
          "message": "enhancement(lua transform): Allow iteration over fields (#1111)",
          "pr_number": 1111,
          "scope": {
            "category": "transform",
            "component_name": "lua",
            "component_type": "transform",
            "name": "lua transform"
          },
          "sha": "219b9259bad71e36a7e1863c8add85a902bc057f",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-10-30 20:48:54 +0000",
          "deletions_count": 13,
          "description": "Move example of iterating over all fields out of the autogenerated file",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 26,
          "message": "docs(lua transform): Move example of iterating over all fields out of the autogenerated file (#1120)",
          "pr_number": 1120,
          "scope": {
            "category": "transform",
            "component_name": "lua",
            "component_type": "transform",
            "name": "lua transform"
          },
          "sha": "ec2c9970ed16c3b06f5dc328b7edd6460db4f310",
          "type": "docs"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-30 14:16:04 +0000",
          "deletions_count": 0,
          "description": "Flatten out region configuration in elasticsearch sink",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(elasticsearch sink): Flatten out region configuration in elasticsearch sink (#1116)",
          "pr_number": 1116,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "608e21abe8198a90b1100868b46550d63ab95c8c",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-10-31 12:07:34 +0000",
          "deletions_count": 22,
          "description": "Improve topology tracing spans",
          "files_count": 47,
          "group": "fix",
          "insertions_count": 276,
          "message": "fix(observability): Improve topology tracing spans (#1123)",
          "pr_number": 1123,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "55766802be0a6c35eb6e1f8d35be9081401b27de",
          "type": "fix"
        },
        {
          "author": "Michael Nitschinger",
          "breaking_change": false,
          "date": "2019-10-31 20:03:31 +0000",
          "deletions_count": 4,
          "description": "Update grok to version 1.0.1",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 4,
          "message": "enhancement(grok_parser transform): Update grok to version 1.0.1 (#1124)",
          "pr_number": 1124,
          "scope": {
            "category": "transform",
            "component_name": "grok_parser",
            "component_type": "transform",
            "name": "grok_parser transform"
          },
          "sha": "641bc4242c7e86cde031a51e4228edb0a66bec27",
          "type": "enhancement"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-10-31 14:56:23 +0000",
          "deletions_count": 11,
          "description": "Limit journald records to the current boot",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 34,
          "message": "fix(journald source): Limit journald records to the current boot (#1122)",
          "pr_number": 1122,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "67ee5cc3055da22e5f9eb4861f8be383c2f72f1c",
          "type": "fix"
        },
        {
          "author": "Michael-J-Ward",
          "breaking_change": false,
          "date": "2019-11-01 08:44:37 +0000",
          "deletions_count": 98,
          "description": "Abstracts runtime into runtime.rs",
          "files_count": 23,
          "group": "chore",
          "insertions_count": 170,
          "message": "chore(operations): Abstracts runtime into runtime.rs (#1098)",
          "pr_number": 1098,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "70482ab33c44226f392877461cb8be833f8bbdd6",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-04 14:29:31 +0000",
          "deletions_count": 10,
          "description": "Add Cargo.toml version check to CI",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 84,
          "message": "chore(operations): Add Cargo.toml version check to CI (#1102)",
          "pr_number": 1102,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "e13b2131dbe297be8ce53f627affe52a9a26ca5d",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-04 15:23:32 +0000",
          "deletions_count": 2,
          "description": "Handle edge cases in the Cargo.toml version check",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Handle edge cases in the Cargo.toml version check (#1138)",
          "pr_number": 1138,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "933fd510ba4e8ae7a6184515371d7a3c0d97dc75",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-04 15:29:42 +0000",
          "deletions_count": 1,
          "description": "Bump version in Cargo.toml to 0.6.0",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Bump version in Cargo.toml to 0.6.0 (#1139)",
          "pr_number": 1139,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "6f236505b5808e0da01cd08df20334ced2f48edf",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-11-04 10:13:29 +0000",
          "deletions_count": 28,
          "description": "Automatically create missing directories",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 121,
          "message": "enhancement(file sink): Automatically create missing directories (#1094)",
          "pr_number": 1094,
          "scope": {
            "category": "sink",
            "component_name": "file",
            "component_type": "sink",
            "name": "file sink"
          },
          "sha": "3b3c824e98c8ae120f32ffb3603077792c165141",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-11-04 11:35:33 +0000",
          "deletions_count": 1,
          "description": "Update lock file for 0.6",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Update lock file for 0.6 (#1140)",
          "pr_number": 1140,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "d9550711ebcc3bd1033b4985efb3af469e8a4384",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-04 23:33:29 +0000",
          "deletions_count": 17,
          "description": "Show Git version and target triple in `vector --version` output",
          "files_count": 5,
          "group": "enhancement",
          "insertions_count": 40,
          "message": "enhancement(cli): Show Git version and target triple in `vector --version` output (#1044)",
          "pr_number": 1044,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "a0a5bee914ea94353d545e2d772978ba7963b20f",
          "type": "enhancement"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-11-04 15:51:53 +0000",
          "deletions_count": 1380,
          "description": "Update lock file",
          "files_count": 10,
          "group": "chore",
          "insertions_count": 880,
          "message": "chore: Update lock file (#1133)",
          "pr_number": 1133,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8be060fc48eb504c30f874fead15f144570cbeb3",
          "type": "chore"
        },
        {
          "author": "David Howell",
          "breaking_change": false,
          "date": "2019-11-05 09:15:57 +0000",
          "deletions_count": 1,
          "description": "Flush and reset any current filter before applying new filter",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(journald source): Flush and reset any current filter before applying new filter (#1135)",
          "pr_number": 1135,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "96bd716fc1c022831eb04afd633ede3efe809d28",
          "type": "fix"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-11-06 09:10:51 +0000",
          "deletions_count": 0,
          "description": "Ensure internal rate limiting is logged",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 1,
          "message": "enhancement(observability): Ensure internal rate limiting is logged (#1151)",
          "pr_number": 1151,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "observability"
          },
          "sha": "c7ad707ed296a93e3d82bff2b3d7793178d50bcc",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-06 22:17:55 +0000",
          "deletions_count": 40,
          "description": "Use inventory for plugins",
          "files_count": 42,
          "group": "chore",
          "insertions_count": 280,
          "message": "chore(config): Use inventory for plugins (#1115)",
          "pr_number": 1115,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "eb0566313849002fa820d57cc15d8a9ec957b9d3",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-11-07 10:22:10 +0000",
          "deletions_count": 17,
          "description": "Fix metrics batch strategy in sinks",
          "files_count": 6,
          "group": "fix",
          "insertions_count": 7,
          "message": "fix(aws_cloudwatch_metrics sink): Fix metrics batch strategy in sinks (#1141)",
          "pr_number": 1141,
          "scope": {
            "category": "sink",
            "component_name": "aws_cloudwatch_metrics",
            "component_type": "sink",
            "name": "aws_cloudwatch_metrics sink"
          },
          "sha": "fefe9ef4c8f1f20513bc31545d36ab00ed09c4a7",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-08 14:30:47 +0000",
          "deletions_count": 130,
          "description": "Refactor the batching configuration",
          "files_count": 12,
          "group": "enhancement",
          "insertions_count": 132,
          "message": "enhancement(config): Refactor the batching configuration (#1154)",
          "pr_number": 1154,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "f4adfd716034141f367e93bebf283d703c09dfaa",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-08 20:35:06 +0000",
          "deletions_count": 1,
          "description": "Add `list` subcommand",
          "files_count": 4,
          "group": "feat",
          "insertions_count": 98,
          "message": "feat(cli): Add `list` subcommand (#1156)",
          "pr_number": 1156,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "cfab2339b9b3f8117d816015d6523976b38190cc",
          "type": "feat"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-11-08 15:58:14 +0000",
          "deletions_count": 6,
          "description": "Stop accidentally requiring region for ES",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 24,
          "message": "fix(elasticsearch sink): Stop accidentally requiring region for ES (#1161)",
          "pr_number": 1161,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "200dccccc58cf5f7fec86b3124ed00e9ad0d5366",
          "type": "fix"
        },
        {
          "author": "dependabot[bot]",
          "breaking_change": false,
          "date": "2019-11-09 18:36:10 +0000",
          "deletions_count": 3,
          "description": "Bump loofah from 2.2.3 to 2.3.1 in /scripts",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore(operatons): Bump loofah from 2.2.3 to 2.3.1 in /scripts (#1163)",
          "pr_number": 1163,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operatons"
          },
          "sha": "4b831475ed4cb6a016b18b4fa4f2457f0591ce21",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-11 17:30:27 +0000",
          "deletions_count": 17,
          "description": "Use vendored OpenSSL",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 20,
          "message": "enhancement(platforms): Use vendored OpenSSL (#1170)",
          "pr_number": 1170,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "platforms"
          },
          "sha": "32cfe37c87a01ae08b61627d31be73ecf840d375",
          "type": "enhancement"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-11-11 09:37:36 +0000",
          "deletions_count": 1,
          "description": "upgrade to rust 1.39.0",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): upgrade to rust 1.39.0 (#1159)",
          "pr_number": 1159,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "fb9c17a26959e8276770a86307807721cd2ded25",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-11 20:34:23 +0000",
          "deletions_count": 0,
          "description": "Add `clean` target to Makefile",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 3,
          "message": "enhancement(operations): Add `clean` target to Makefile (#1171)",
          "pr_number": 1171,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8de50f4603b3e7626af27b24d9a350eaadb9b4e7",
          "type": "enhancement"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-11-12 00:09:45 +0000",
          "deletions_count": 4,
          "description": "Fixes a bug droping parsed field",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 24,
          "message": "fix(json_parser transform): Fixes a bug droping parsed field (#1167)",
          "pr_number": 1167,
          "scope": {
            "category": "transform",
            "component_name": "json_parser",
            "component_type": "transform",
            "name": "json_parser transform"
          },
          "sha": "f9d3111015352910e71dab210c376b09cdd26333",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-13 13:16:25 +0000",
          "deletions_count": 60,
          "description": "`host` is not required when provider is AWS",
          "files_count": 5,
          "group": "fix",
          "insertions_count": 112,
          "message": "fix(elasticsearch sink): `host` is not required when provider is AWS (#1164)",
          "pr_number": 1164,
          "scope": {
            "category": "sink",
            "component_name": "elasticsearch",
            "component_type": "sink",
            "name": "elasticsearch sink"
          },
          "sha": "a272f633464ce06ab28e5d9a7c1e7d6b595c61ec",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-13 15:34:38 +0000",
          "deletions_count": 1,
          "description": " Limit the number of CircleCI build jobs to 8",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations):  Limit the number of CircleCI build jobs to 8 (#1176)",
          "pr_number": 1176,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2100100b5cda0f57292a17bbf4473ed543811f39",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-13 15:34:59 +0000",
          "deletions_count": 1,
          "description": "Fix missed `cargo fmt` run on elasticsearch sink",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore: Fix missed `cargo fmt` run on elasticsearch sink (#1175)",
          "pr_number": 1175,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2e2af43786ff0dbc292f98cedc830791d1e20937",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-11-13 17:21:05 +0000",
          "deletions_count": 1,
          "description": "Don't drop parsed field",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 20,
          "message": "fix(grok_parser transform): Don't drop parsed field (#1172)",
          "pr_number": 1172,
          "scope": {
            "category": "transform",
            "component_name": "grok_parser",
            "component_type": "transform",
            "name": "grok_parser transform"
          },
          "sha": "cfb66e5b90007d9a5dc461afa80e6d3e190febcf",
          "type": "fix"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-13 21:23:21 +0000",
          "deletions_count": 3,
          "description": "Add support for target field configuration",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 152,
          "message": "enhancement(json_parser transform): Add support for target field configuration (#1165)",
          "pr_number": 1165,
          "scope": {
            "category": "transform",
            "component_name": "json_parser",
            "component_type": "transform",
            "name": "json_parser transform"
          },
          "sha": "e0433fd1ada425c1f5c9505426fa362aae14249e",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-14 10:49:59 +0000",
          "deletions_count": 6,
          "description": "Add `generate` subcommand",
          "files_count": 6,
          "group": "feat",
          "insertions_count": 272,
          "message": "feat(cli): Add `generate` subcommand (#1168)",
          "pr_number": 1168,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "e503057ff3616569521a208abbbed8c3e8fbc848",
          "type": "feat"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-14 21:24:43 +0000",
          "deletions_count": 28,
          "description": "Use `strptime` instead of `strftime` in docs where appropriate",
          "files_count": 13,
          "group": "docs",
          "insertions_count": 28,
          "message": "docs: Use `strptime` instead of `strftime` in docs where appropriate (#1183)",
          "pr_number": 1183,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "de0a6734710a6c63c969048a06d3b55ae1637c87",
          "type": "docs"
        },
        {
          "author": "Jean Mertz",
          "breaking_change": false,
          "date": "2019-11-14 20:23:38 +0000",
          "deletions_count": 4,
          "description": "Support default environment variable values",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 11,
          "message": "enhancement(config): Support default environment variable values (#1185)",
          "pr_number": 1185,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "fc2c1db5824f8499190efa078c993f3f52737043",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-14 23:49:51 +0000",
          "deletions_count": 2,
          "description": "Update rdkafka to fix rdkafka/cmake feature",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Update rdkafka to fix rdkafka/cmake feature (#1186)",
          "pr_number": 1186,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "20ba2575f40944b36c7bbd9e4d821452626f288b",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-14 23:50:35 +0000",
          "deletions_count": 4,
          "description": "Use leveldb from fork with improved portability",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(operations): Use leveldb from fork with improved portability (#1184)",
          "pr_number": 1184,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "84d830b57de1798b2aac61279f7a0ae99f854241",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-14 23:50:59 +0000",
          "deletions_count": 8,
          "description": "Increase wait timeouts in tests which otherwise fail on slow CPUs",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 8,
          "message": "fix(testing): Increase wait timeouts in tests which otherwise fail on slow CPUs (#1181)",
          "pr_number": 1181,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "3ce0b4ed645d2844f1f6c5308409e2e9466c0799",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-19 17:35:50 +0000",
          "deletions_count": 3,
          "description": "Control which version of leveldb-sys to use with features",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore(operations): Control which version of leveldb-sys to use with features (#1191)",
          "pr_number": 1191,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "0884f5d90ca2162aaa0ea6b9ab5d2e10a026a286",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-19 17:39:05 +0000",
          "deletions_count": 0,
          "description": "Support `armv7-unknown-linux` (Raspberry Pi, etc) platforms",
          "files_count": 4,
          "group": "feat",
          "insertions_count": 366,
          "message": "feat(new platform): Support `armv7-unknown-linux` (Raspberry Pi, etc) platforms (#1054)",
          "pr_number": 1054,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "new platform"
          },
          "sha": "90388ed57afea24d569b2317d97df7035211b252",
          "type": "feat"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-19 17:41:09 +0000",
          "deletions_count": 10,
          "description": "Support `aarch64-unknown-linux` (ARM64, Raspberry Pi, etc) platforms",
          "files_count": 4,
          "group": "feat",
          "insertions_count": 347,
          "message": "feat(new platform): Support `aarch64-unknown-linux` (ARM64, Raspberry Pi, etc) platforms (#1193)",
          "pr_number": 1193,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "new platform"
          },
          "sha": "d58139caf6cdb15b4622360d7c9a04a8c86724d6",
          "type": "feat"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-11-19 15:24:03 +0000",
          "deletions_count": 37,
          "description": "Re-fix journald cursor handling and libsystemd name",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 34,
          "message": "fix(journald source): Re-fix journald cursor handling and libsystemd name (#1202)",
          "pr_number": 1202,
          "scope": {
            "category": "source",
            "component_name": "journald",
            "component_type": "source",
            "name": "journald source"
          },
          "sha": "1b833eb6d693d4c281aa51c332202eb2796ba4db",
          "type": "fix"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-19 16:51:07 +0000",
          "deletions_count": 23643,
          "description": "New website and documentation",
          "files_count": 496,
          "group": "docs",
          "insertions_count": 39821,
          "message": "docs: New website and documentation (#1207)",
          "pr_number": 1207,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "2d2fadb2599d99ded3d73286fe17a67d20d23805",
          "type": "docs"
        },
        {
          "author": "Jean Mertz",
          "breaking_change": false,
          "date": "2019-11-20 00:27:10 +0000",
          "deletions_count": 0,
          "description": "Initial `ansi_stripper` transform implementation",
          "files_count": 5,
          "group": "feat",
          "insertions_count": 158,
          "message": "feat(new transform): Initial `ansi_stripper` transform implementation (#1188)",
          "pr_number": 1188,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "2d419d57d5ab6072bc1058126bc3be50fa57c835",
          "type": "feat"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-20 14:37:14 +0000",
          "deletions_count": 2,
          "description": "Fix README banner",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 146,
          "message": "docs: Fix README banner",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "55b68910ee3d80fdf302faf5a5bc9aa1f68e8dce",
          "type": "docs"
        },
        {
          "author": "Amit Saha",
          "breaking_change": false,
          "date": "2019-11-21 08:36:02 +0000",
          "deletions_count": 0,
          "description": "Initial `geoip` transform implementation",
          "files_count": 6,
          "group": "feat",
          "insertions_count": 286,
          "message": "feat(new transform): Initial `geoip` transform implementation (#1015)",
          "pr_number": 1015,
          "scope": {
            "category": "transform",
            "component_name": null,
            "component_type": "transform",
            "name": "new transform"
          },
          "sha": "458f6cc0e3fbc6fded1fdf8d47dedb2d0be3bb2d",
          "type": "feat"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-20 21:31:34 +0000",
          "deletions_count": 307,
          "description": "Small website and documentation improvements",
          "files_count": 28,
          "group": "docs",
          "insertions_count": 880,
          "message": "docs: Small website and documentation improvements (#1215)",
          "pr_number": 1215,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "803c7f98349a4d07bfc68bc7f10a80c165698f1a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-21 00:14:23 +0000",
          "deletions_count": 5,
          "description": "Small changes to website homepage styles",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Small changes to website homepage styles",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fb6a1dc7d41a73869b36d20863f410a3f3d9a844",
          "type": "docs"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-21 15:28:49 +0000",
          "deletions_count": 11,
          "description": "Fix some URLs",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 7,
          "message": "docs: Fix some URLs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "42ca451408b42db43ea2597509e0ce85b44059a9",
          "type": "docs"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-21 15:39:33 +0000",
          "deletions_count": 91,
          "description": "Allow >1 config targets for validate command",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 82,
          "message": "enhancement(cli): Allow >1 config targets for validate command (#1218)",
          "pr_number": 1218,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "9fe1eeb4786b27843673c05ff012f6b5cf5c3e45",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-21 23:53:20 +0000",
          "deletions_count": 2,
          "description": "Fix components link in README",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix components link in README (#1222)",
          "pr_number": 1222,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3af177516728cc4a78a198f69d1cb6b0f0b093fc",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-21 16:13:16 +0000",
          "deletions_count": 4232,
          "description": "Rename components section to reference in docs",
          "files_count": 134,
          "group": "docs",
          "insertions_count": 740,
          "message": "docs: Rename components section to reference in docs (#1223)",
          "pr_number": 1223,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "58246b306f0e927cfc2ffcfb6f023c146846db0e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-21 16:30:11 +0000",
          "deletions_count": 4,
          "description": "Styling fixes",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 13,
          "message": "docs: Styling fixes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "89c50b177689cbacf4dc3f930ebbe2b264046b8a",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 00:49:04 +0000",
          "deletions_count": 3,
          "description": "Fix restoring of `rust-toolchain` file",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(operations): Fix restoring of `rust-toolchain` file (#1224)",
          "pr_number": 1224,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5b38129d0de1185235e630a571e31c3e9f5ab85c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 01:25:18 +0000",
          "deletions_count": 1,
          "description": "Produce archives for `armv7-unknown-linux-musleabihf`",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 29,
          "message": "chore(operations): Produce archives for `armv7-unknown-linux-musleabihf` (#1225)",
          "pr_number": 1225,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5f39c2f3515d958d40c9a6187c59806c4731c91c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 02:01:41 +0000",
          "deletions_count": 72,
          "description": "Support `x86_64-pc-windows-msvc` (Windows 7+) platform",
          "files_count": 15,
          "group": "feat",
          "insertions_count": 337,
          "message": "feat(new platform): Support `x86_64-pc-windows-msvc` (Windows 7+) platform (#1205)",
          "pr_number": 1205,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "new platform"
          },
          "sha": "a1410f69382bd8036a7046a156c64f56e8f9ef33",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-21 23:06:41 +0000",
          "deletions_count": 53,
          "description": "Update downloads links",
          "files_count": 11,
          "group": "docs",
          "insertions_count": 144,
          "message": "docs: Update downloads links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "bf9402b2151d976edd42b35d08c1722de7ec2b9b",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 12:58:49 +0000",
          "deletions_count": 374,
          "description": "Fix `check-generate` check in CI",
          "files_count": 8,
          "group": "chore",
          "insertions_count": 398,
          "message": "chore(operations): Fix `check-generate` check in CI (#1226)",
          "pr_number": 1226,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5062b39a82949c86fdc80658085a88b78a24a27c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 14:15:54 +0000",
          "deletions_count": 5,
          "description": "Use bash from Docker containers as a shell in Circle CI",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 13,
          "message": "chore(operations): Use bash from Docker containers as a shell in Circle CI (#1227)",
          "pr_number": 1227,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "befb29916c2d19827303109769ca824fbd167870",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 14:51:24 +0000",
          "deletions_count": 12,
          "description": "Fix invocation of check jobs",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 12,
          "message": "chore(operations): Fix invocation of check jobs (#1229)",
          "pr_number": 1229,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "6052cbc9a00eac0b2db96651730bd730c39ca83e",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 16:04:48 +0000",
          "deletions_count": 10,
          "description": "Verify `zip` archives for `x86_64-pc-windows-msvc` in `wine`",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 17,
          "message": "chore(operations): Verify `zip` archives for `x86_64-pc-windows-msvc` in `wine` (#1228)",
          "pr_number": 1228,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "d7a0fd1362f7b99a3bac344434d2a50305f1fa2e",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 10:25:57 +0000",
          "deletions_count": 90,
          "description": "Update to docusaurus alpha.36",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 82,
          "message": "chore(website): Update to docusaurus alpha.36",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "7906dcae3c0a43c99880f2cea9aeb01de629157c",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 11:22:16 +0000",
          "deletions_count": 3,
          "description": "Fix curl commands mentioned in #1234",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: Fix curl commands mentioned in #1234",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "49a861ab3045570f1e173c56fa23291e014856a2",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-22 16:49:49 +0000",
          "deletions_count": 1,
          "description": "Run nightly builds at 5pm UTC",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Run nightly builds at 5pm UTC",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "39bd126fe67b048003532c178c64be90ef4cec62",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 13:19:53 +0000",
          "deletions_count": 6,
          "description": "Redraw diagram to fix an initial load issue in Chrome",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 4,
          "message": "docs: Redraw diagram to fix an initial load issue in Chrome",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fe32fdc5d222182f18e4118af28d72d4b06dca0d",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 15:45:12 +0000",
          "deletions_count": 7,
          "description": "Rerender diagram to fix Chrome update issue",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 10,
          "message": "docs: Rerender diagram to fix Chrome update issue",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6de3e4f3a725c978ccaa95c5a9180df202c5a074",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 16:43:22 +0000",
          "deletions_count": 4,
          "description": "More Chrome fixes",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(website): More Chrome fixes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "12d36bbe2eb223ab89335b61dfbb7e18c4649981",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 17:00:30 +0000",
          "deletions_count": 8,
          "description": "Fix Chrome sorting issue",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 8,
          "message": "chore(website): Fix Chrome sorting issue",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "f9396da79b49f617ce93d6be233f9592831fab2d",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 19:32:52 +0000",
          "deletions_count": 182,
          "description": "Fix readme",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 47,
          "message": "docs: Fix readme",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "662c5d1346ea2b01c0bc3c11c648cbdf92035fe2",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-22 19:36:11 +0000",
          "deletions_count": 11,
          "description": "Fix readme component counts",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 11,
          "message": "docs: Fix readme component counts",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "cb6571798af5b80c123905b4cac3a56a67fc3181",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-23 11:50:14 +0000",
          "deletions_count": 7,
          "description": "Make `openssl/vendored` feature optional",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 7,
          "message": "enhancement(platforms): Make `openssl/vendored` feature optional (#1239)",
          "pr_number": 1239,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "platforms"
          },
          "sha": "1f401a68bdb5c0bcfc9d0385f49a70f22fbce5d9",
          "type": "enhancement"
        },
        {
          "author": "Austin Seipp",
          "breaking_change": false,
          "date": "2019-11-23 04:21:20 +0000",
          "deletions_count": 6,
          "description": "Accept metric events, too",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 8,
          "message": "enhancement(blackhole sink): Accept metric events, too (#1237)",
          "pr_number": 1237,
          "scope": {
            "category": "sink",
            "component_name": "blackhole",
            "component_type": "sink",
            "name": "blackhole sink"
          },
          "sha": "52a49d5a32f091eec7c174b02803f7fc3ca5af34",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-23 13:27:51 +0000",
          "deletions_count": 14,
          "description": "Update `openssl` dependency",
          "files_count": 2,
          "group": "enhancement",
          "insertions_count": 14,
          "message": "enhancement(platforms): Update `openssl` dependency (#1240)",
          "pr_number": 1240,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "platforms"
          },
          "sha": "457f964bde42fce3b92e5bd1a65ef6192c404a16",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-23 15:49:09 +0000",
          "deletions_count": 0,
          "description": "Don't put *.erb files to configs directory",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 2,
          "message": "fix(platforms): Don't put *.erb files to configs directory (#1241)",
          "pr_number": 1241,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "platforms"
          },
          "sha": "cdee561f8c1a023b77c5db712cc081b90570eb55",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-23 22:51:25 +0000",
          "deletions_count": 351,
          "description": "Document installation on Windows",
          "files_count": 37,
          "group": "docs",
          "insertions_count": 1064,
          "message": "docs: Document installation on Windows (#1235)",
          "pr_number": 1235,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b449b2b67f077760215294c418688c27f3f629a0",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-23 15:01:47 +0000",
          "deletions_count": 1,
          "description": "Add docker to homepage",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 151,
          "message": "docs: Add docker to homepage",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "663be72997339cb9c30f935d9ef4c8e7732bc56c",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-23 15:13:26 +0000",
          "deletions_count": 1,
          "description": "Update docker image",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Update docker image",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "732265e9be0ae4c5add4679ef11fe808032c8f78",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-23 15:40:52 +0000",
          "deletions_count": 1,
          "description": "Fix administrating doc",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 31,
          "message": "docs: Fix administrating doc",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5c15a3c6c7811315ff980e57f685d7fd3616ca7e",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-23 15:41:36 +0000",
          "deletions_count": 0,
          "description": "Add administration to docs sidebar",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Add administration to docs sidebar",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "068ae60a963523e540f2f404545e287a8b161037",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-23 20:46:47 +0000",
          "deletions_count": 5,
          "description": "Add C++ toolchain installation step",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 9,
          "message": "docs: Add C++ toolchain installation step",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "cdcd624da93fd36676e84426b8ec93917a90c8e1",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 01:14:17 +0000",
          "deletions_count": 20,
          "description": "Attempt to fix website theme flickering",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 25,
          "message": "chore(website): Attempt to fix website theme flickering",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "d7b7735ae57e362e8255a59a578ac12f4b438119",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-24 10:26:30 +0000",
          "deletions_count": 25,
          "description": "Describe build features",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 82,
          "message": "docs: Describe build features (#1243)",
          "pr_number": 1243,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "1ec95b9df9a1f0456c02dcfd9824024ed7516fcc",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 12:03:02 +0000",
          "deletions_count": 3,
          "description": "Add ARMv7 to installation docs",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 84,
          "message": "docs: Add ARMv7 to installation docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "37e60137b4fab70dc97cc177ecd6f1c81b1c86b0",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 12:24:10 +0000",
          "deletions_count": 15,
          "description": "Various installation docs corrections, closes #1234",
          "files_count": 8,
          "group": "docs",
          "insertions_count": 27,
          "message": "docs: Various installation docs corrections, closes #1234",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8698eb922c5e1a1a0906fe25e2e9f2a39acb9c06",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 12:26:07 +0000",
          "deletions_count": 5,
          "description": "Remove Alogia search until it has indexed everything",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(website): Remove Alogia search until it has indexed everything",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "818c28228965d9d0b691e18298127eb5666d7865",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-24 21:56:40 +0000",
          "deletions_count": 7,
          "description": "Fix passing environment variables inside the CI Docker containers",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 16,
          "message": "chore(operations): Fix passing environment variables inside the CI Docker containers (#1233)",
          "pr_number": 1233,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "b00996fc6949d6d34fcd13f685b5b91d116f4e8c",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-24 15:06:09 +0000",
          "deletions_count": 141,
          "description": "Add operating system as a compenent attribute and filter",
          "files_count": 59,
          "group": "chore",
          "insertions_count": 619,
          "message": "chore(website): Add operating system as a compenent attribute and filter (#1244)",
          "pr_number": 1244,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "604b40d15bcbfb62eae0ca314ffad06a365ccc85",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 15:56:01 +0000",
          "deletions_count": 2,
          "description": "Fix operating system filter",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(website): Fix operating system filter",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "dde45458aa375d5c9e1eb7beb4bf9fe102ccb0db",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 16:02:39 +0000",
          "deletions_count": 33,
          "description": "Dont show operating systems for transforms",
          "files_count": 16,
          "group": "chore",
          "insertions_count": 33,
          "message": "chore(website): Dont show operating systems for transforms",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "0cad20f837f1f682f9a5b976e150417484e4839f",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 17:14:28 +0000",
          "deletions_count": 1,
          "description": "Fix broken link on homepage",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Fix broken link on homepage",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "cad2349778d5d42e71ed12c7cf974e6f9ef731d5",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-24 21:43:05 +0000",
          "deletions_count": 1,
          "description": "Add sidebar background and ga id",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(website): Add sidebar background and ga id",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "9bdaf14ee089da0ab6dff3b464a3086fc709cec6",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-25 11:12:50 +0000",
          "deletions_count": 2,
          "description": "Fix link",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "545ea5b0c1f88fc8ee42c9bce13358155bbf34fe",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-25 15:25:18 +0000",
          "deletions_count": 2,
          "description": "Fix name of `shiplift/unix-socket` feature",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix name of `shiplift/unix-socket` feature (#1251)",
          "pr_number": 1251,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f9c486ce4abcd77cf61ddc7fe2fadb4aeae3b806",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 00:08:26 +0000",
          "deletions_count": 641,
          "description": "Update dependencies",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 644,
          "message": "chore(website): Update dependencies",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "0e26cfd64a421b3b8296697e5dfca8d8ab35df6c",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 00:15:02 +0000",
          "deletions_count": 13,
          "description": "Fix Github issues links",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 13,
          "message": "docs: Fix Github issues links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9863f819c001827c400803b9fc0b1b71ea862244",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 10:42:39 +0000",
          "deletions_count": 7,
          "description": "Use the proper font in the configuration digram, ref #1234",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 7,
          "message": "chore(website): Use the proper font in the configuration digram, ref #1234",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "42eabf66dc5138f43c7310b067064beaf3f8c29d",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 11:10:47 +0000",
          "deletions_count": 5,
          "description": "Enable Algolia search",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(website): Enable Algolia search",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "9358c7a2d51ca259e38e49de5c2a46049146fead",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 11:30:11 +0000",
          "deletions_count": 5,
          "description": "Remove paginator from main doc content so that it is not included in search results",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 11,
          "message": "chore(website): Remove paginator from main doc content so that it is not included in search results",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "8f18ad80302bf5975ad704271eb2c8d986b1c7d0",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 12:20:05 +0000",
          "deletions_count": 9,
          "description": "Fix search field styling",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 42,
          "message": "chore(website): Fix search field styling",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "d8fef3c66ce2072c003ba30704276e51c5267dc4",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 12:25:24 +0000",
          "deletions_count": 4,
          "description": "Move main links in header to the left",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 4,
          "message": "chore(website): Move main links in header to the left",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "5492ae39c48d67e22fb983b9e55fa1cf5ee09dae",
          "type": "chore"
        },
        {
          "author": "James Sewell",
          "breaking_change": false,
          "date": "2019-11-26 05:38:57 +0000",
          "deletions_count": 17,
          "description": "Add JSON encoding option",
          "files_count": 6,
          "group": "enhancement",
          "insertions_count": 102,
          "message": "enhancement(http sink): Add JSON encoding option (#1174)",
          "pr_number": 1174,
          "scope": {
            "category": "sink",
            "component_name": "http",
            "component_type": "sink",
            "name": "http sink"
          },
          "sha": "357bdbbe9bf142eaf028a46e016e7b37e73a6e88",
          "type": "enhancement"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-25 14:38:10 +0000",
          "deletions_count": 61,
          "description": "Reference exact latest version instead of \"latest\" in download URLs",
          "files_count": 7,
          "group": "docs",
          "insertions_count": 153,
          "message": "docs: Reference exact latest version instead of \"latest\" in download URLs (#1254)",
          "pr_number": 1254,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "969a426e0f9826e5bebf45ffb87fe7b2f785e7e7",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 14:38:34 +0000",
          "deletions_count": 11,
          "description": "Fix search bar styling on mobile",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 24,
          "message": "chore(website): Fix search bar styling on mobile",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "b29e4e309b9a13eff12f46cf00e21a76090e46fd",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 14:52:15 +0000",
          "deletions_count": 101,
          "description": "Add auto-generated comments to files that are auto-generated, closes #1256",
          "files_count": 114,
          "group": "docs",
          "insertions_count": 655,
          "message": "docs: Add auto-generated comments to files that are auto-generated, closes #1256",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ea81323033974a347bca458e5ab7e446b24228a3",
          "type": "docs"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-11-25 14:27:35 +0000",
          "deletions_count": 6,
          "description": "Sleep to avoid split reads",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 16,
          "message": "fix(file source): Sleep to avoid split reads (#1236)",
          "pr_number": 1236,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "26333d9cf00bb5e44ae73aa17a7cab5583dc7d22",
          "type": "fix"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-25 15:49:57 +0000",
          "deletions_count": 0,
          "description": "Add CODEOWNERS file",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 71,
          "message": "chore(operations): Add CODEOWNERS file (#1248)",
          "pr_number": 1248,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "9b7fdca9f9f0d5818afbd821210f9f2c17ccc564",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-25 21:56:15 +0000",
          "deletions_count": 79,
          "description": "Add `test` sub-command",
          "files_count": 38,
          "group": "feat",
          "insertions_count": 2446,
          "message": "feat(cli): Add `test` sub-command (#1220)",
          "pr_number": 1220,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "a9fbcb3ddbb3303f981257be064a995db59b7dbb",
          "type": "feat"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-25 22:43:40 +0000",
          "deletions_count": 0,
          "description": "Re-generate unit test spec",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Re-generate unit test spec",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "3e92c1eac7a44b0661f25b452a112e5024edf7b3",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-25 19:44:24 +0000",
          "deletions_count": 8,
          "description": "Add hash links to all headings",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 20,
          "message": "chore(website): Add hash links to all headings",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "a282db6df013b89d84694e68ecde38c4d544c1ba",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": true,
          "date": "2019-11-26 12:24:33 +0000",
          "deletions_count": 1036,
          "description": "Reorganise metric model",
          "files_count": 16,
          "group": "breaking change",
          "insertions_count": 1389,
          "message": "enhancement(metric data model)!: Reorganise metric model (#1217)",
          "pr_number": 1217,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "aed6f1bf1cb0d3d10b360e16bd118665a49c4ea5",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-26 15:24:00 +0000",
          "deletions_count": 0,
          "description": "Turn \"executable\" bit off for some of docs files",
          "files_count": 21,
          "group": "docs",
          "insertions_count": 0,
          "message": "docs: Turn \"executable\" bit off for some of docs files",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "df3e70980bfc9f6cde60516df482949fd0bc592b",
          "type": "docs"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-11-26 16:35:36 +0000",
          "deletions_count": 298,
          "description": "Enrich events with metadata",
          "files_count": 39,
          "group": "enhancement",
          "insertions_count": 505,
          "message": "enhancement(docker source): Enrich events with metadata (#1149)",
          "pr_number": 1149,
          "scope": {
            "category": "source",
            "component_name": "docker",
            "component_type": "source",
            "name": "docker source"
          },
          "sha": "f20fc4ad3ea88d112d84be58eb51b4a5e85df21f",
          "type": "enhancement"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-26 11:01:48 +0000",
          "deletions_count": 2,
          "description": "Testing documentation touchups",
          "files_count": 4,
          "group": "docs",
          "insertions_count": 718,
          "message": "docs: Testing documentation touchups",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "f5cfdfe2fb25703ea308992c3d106b5c4b3b7af1",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-26 11:17:52 +0000",
          "deletions_count": 177,
          "description": "Fix examples syntax and parsing",
          "files_count": 19,
          "group": "docs",
          "insertions_count": 198,
          "message": "docs: Fix examples syntax and parsing",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c86b23818345136ea0bf911d92426440387b1620",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-26 11:34:17 +0000",
          "deletions_count": 12,
          "description": "Clarify guarantees language to be feature specific not component specific",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 12,
          "message": "docs: Clarify guarantees language to be feature specific not component specific",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8fae3d0a5524f0172a97a1235c13305f660bc07f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-26 11:46:58 +0000",
          "deletions_count": 9,
          "description": "Fix docker source config examples",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Fix docker source config examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "57434aa05893d89300cee34f7aa2be7c6be7405b",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-26 19:15:14 +0000",
          "deletions_count": 43,
          "description": "Fix sorting in make generate",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 35,
          "message": "chore(operations): Fix sorting in make generate",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "18da561ba25843b13ce013f5a2052dfbff877b2b",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-27 12:23:58 +0000",
          "deletions_count": 2,
          "description": "Add timeouts to crash tests",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 14,
          "message": "chore(testing): Add timeouts to crash tests (#1265)",
          "pr_number": 1265,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "3db6403a24c16a36ba3367dedff006c9c9924626",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-27 17:03:31 +0000",
          "deletions_count": 1,
          "description": "Run `x86_64-pc-windows-msvc` tests in release mode",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(testing): Run `x86_64-pc-windows-msvc` tests in release mode (#1269)",
          "pr_number": 1269,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "df2b5d8016f27e868e0bb2a6feaf8bd99caaf64f",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 10:22:21 +0000",
          "deletions_count": 41,
          "description": "Move env vars to reference section",
          "files_count": 11,
          "group": "docs",
          "insertions_count": 204,
          "message": "docs: Move env vars to reference section",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c9f96ffaef533272103a167a5900edad1ed5946c",
          "type": "docs"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-11-27 19:19:04 +0000",
          "deletions_count": 3,
          "description": "Custom DNS resolution",
          "files_count": 11,
          "group": "feat",
          "insertions_count": 733,
          "message": "feat(networking): Custom DNS resolution (#1118)",
          "pr_number": 1118,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "networking"
          },
          "sha": "77e582b526680a22ea4da616cbfdb3b0ad281097",
          "type": "feat"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 13:44:27 +0000",
          "deletions_count": 1697,
          "description": "Add env_vars key to all components",
          "files_count": 109,
          "group": "docs",
          "insertions_count": 3752,
          "message": "docs: Add env_vars key to all components",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "b255a52a6b53bcc1a9361ae746dde2c5d5fb9132",
          "type": "docs"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-27 18:41:49 +0000",
          "deletions_count": 616,
          "description": "Fix rate_limit and retry option names",
          "files_count": 20,
          "group": "docs",
          "insertions_count": 625,
          "message": "docs: Fix rate_limit and retry option names (#1270)",
          "pr_number": 1270,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8fac7296e4c17969c08841a58ce7b64f2ede5331",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 18:51:15 +0000",
          "deletions_count": 832,
          "description": "Fix variable field names",
          "files_count": 25,
          "group": "docs",
          "insertions_count": 79,
          "message": "docs: Fix variable field names",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0a06803a89aa3ca570edf72834abac52db94a0b8",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 19:10:37 +0000",
          "deletions_count": 72,
          "description": "Fix variable field names",
          "files_count": 26,
          "group": "docs",
          "insertions_count": 95,
          "message": "docs: Fix variable field names",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "e50767b1560288cb862bf9f933a4cc92e7b329a6",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 19:38:05 +0000",
          "deletions_count": 2210,
          "description": "Fix config examples category name",
          "files_count": 46,
          "group": "docs",
          "insertions_count": 894,
          "message": "docs: Fix config examples category name",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "79f28aa15f26d73175467fb621ed87bf34240991",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-27 19:52:42 +0000",
          "deletions_count": 70,
          "description": "Fix example categories",
          "files_count": 24,
          "group": "docs",
          "insertions_count": 53,
          "message": "docs: Fix example categories",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "ae90038afb5d89eb080bd7c760ce3a4f1c67f219",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-28 10:32:52 +0000",
          "deletions_count": 274,
          "description": "Build .deb packages for all musl targets",
          "files_count": 17,
          "group": "chore",
          "insertions_count": 500,
          "message": "chore(operations): Build .deb packages for all musl targets (#1247)",
          "pr_number": 1247,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "b3554b16fa333727e21c8eaae87df4533e217c96",
          "type": "chore"
        },
        {
          "author": "Dan Palmer",
          "breaking_change": false,
          "date": "2019-11-28 15:43:22 +0000",
          "deletions_count": 1,
          "description": "Typo",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Typo (#1273)",
          "pr_number": 1273,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "10de21ba24814324547d53553ed098742279f935",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-28 10:51:03 +0000",
          "deletions_count": 1,
          "description": "Remove console.log",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 0,
          "message": "docs: Remove console.log",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "9c531ca1e734234e187d82b76912bf5dfa188742",
          "type": "docs"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-11-29 15:29:25 +0000",
          "deletions_count": 0,
          "description": "Add a unit test guide",
          "files_count": 6,
          "group": "docs",
          "insertions_count": 253,
          "message": "docs: Add a unit test guide (#1278)",
          "pr_number": 1278,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "c815d27773da3acd0272ef009270f772a3103791",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-29 12:01:14 +0000",
          "deletions_count": 23,
          "description": "Add topology section",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 90,
          "message": "chore(website): Add topology section",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "7b5a7f322bffdbd7638791e32effa848deb1fdea",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-29 13:59:31 +0000",
          "deletions_count": 3,
          "description": "Default to centralized topology",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 3,
          "message": "chore(website): Default to centralized topology",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "ecdb56f5f49920353e5696e936f2d711d6881bbd",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-11-29 14:21:42 +0000",
          "deletions_count": 13,
          "description": "Fix rounded tabs",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 33,
          "message": "chore(website): Fix rounded tabs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "1dc6e303079bf6a9bb9802fe108e77edf0b0fd83",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 00:15:17 +0000",
          "deletions_count": 1,
          "description": "Increase CI output timeout",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 8,
          "message": "chore(operations): Increase CI output timeout (#1272)",
          "pr_number": 1272,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "4e98b8321cd334d780a5388bd848d83cb677003c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 00:37:24 +0000",
          "deletions_count": 24,
          "description": "Delete unused OpenSSL patch",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 0,
          "message": "chore(operations): Delete unused OpenSSL patch (#1282)",
          "pr_number": 1282,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "7dd271e9102d2a2eb2016f8d735c8d9710966210",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-29 22:11:41 +0000",
          "deletions_count": 1,
          "description": "Run nightly builds at 12am UTC",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Run nightly builds at 12am UTC",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "32e5bfc2ff07ce0dddf817d5b64a2b04cc40f9ab",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 01:14:25 +0000",
          "deletions_count": 5,
          "description": "Set up redirects for x86_64-unknown-linux-gnu archives",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 23,
          "message": "chore(operations): Set up redirects for x86_64-unknown-linux-gnu archives (#1284)",
          "pr_number": 1284,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "62992492de9c21e8a59464696b2ba226c50b82f0",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 01:42:23 +0000",
          "deletions_count": 122,
          "description": "Build multi-arch Docker images",
          "files_count": 9,
          "group": "chore",
          "insertions_count": 151,
          "message": "chore(operations): Build multi-arch Docker images (#1279)",
          "pr_number": 1279,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "5fa10916882cd07ee6c6726be10227b321f5880c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 02:06:35 +0000",
          "deletions_count": 11,
          "description": "Use `sidebar_label` as subpage title if possible",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 17,
          "message": "chore(website): Use `sidebar_label` as subpage title if possible (#1283)",
          "pr_number": 1283,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "5c6942f8e52971ec3eb95750d2a79574cb0c12bd",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 02:06:47 +0000",
          "deletions_count": 8,
          "description": "Simplify platform names in \"downloads\" section",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 8,
          "message": "chore(website): Simplify platform names in \"downloads\" section (#1285)",
          "pr_number": 1285,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "4058ef356271a8276ddd6b1f41933d25ddd585a6",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 10:13:42 +0000",
          "deletions_count": 2,
          "description": "Run nightly builds at 11am UTC",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(operations): Run nightly builds at 11am UTC",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "42c2a1f75e639ff29da5419cff29848fa3163d01",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 13:15:43 +0000",
          "deletions_count": 2,
          "description": "Remove extra `setup_remote_docker` step from `relase-docker`",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 0,
          "message": "fix(operations): Remove extra `setup_remote_docker` step from `relase-docker` (#1287)",
          "pr_number": 1287,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8f271aee3b9873b10a68ab5c747c4e895347acca",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 13:15:56 +0000",
          "deletions_count": 1,
          "description": "Fix S3 release verification",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(operations): Fix S3 release verification (#1286)",
          "pr_number": 1286,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "643716654c9049e18c057d9e88de4e78f566d983",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-11-30 18:26:36 +0000",
          "deletions_count": 22,
          "description": "Upgrade Docker on the step in which it is used",
          "files_count": 3,
          "group": "fix",
          "insertions_count": 22,
          "message": "fix(operations): Upgrade Docker on the step in which it is used (#1288)",
          "pr_number": 1288,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "08a297961a767d798ebb244a10baf05b318272e7",
          "type": "fix"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-11-30 16:14:02 +0000",
          "deletions_count": 618,
          "description": "Cleanup installation docs",
          "files_count": 32,
          "group": "docs",
          "insertions_count": 783,
          "message": "docs: Cleanup installation docs (#1289)",
          "pr_number": 1289,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "70965d8e6d0c0d850faa86fb674987a107df9b93",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-01 11:21:05 +0000",
          "deletions_count": 229,
          "description": "Update to docaurus 2.0.0-alpha.37",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 242,
          "message": "chore(website): Update to docaurus 2.0.0-alpha.37",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "469671dc457f867cee8bab247b6529026e7ae4ca",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-01 11:39:36 +0000",
          "deletions_count": 10,
          "description": "Group downloads by os",
          "files_count": 8,
          "group": "chore",
          "insertions_count": 62,
          "message": "chore(website): Group downloads by os",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "06a864b106bc2233c5d5a8ba78f045def8a937f6",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-01 13:15:28 +0000",
          "deletions_count": 25,
          "description": "Rename raspberry-pi to raspbian",
          "files_count": 10,
          "group": "docs",
          "insertions_count": 44,
          "message": "docs: Rename raspberry-pi to raspbian",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "8ee38009da9bcd41444e9cf2ed48683aa1870a1a",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-01 13:29:57 +0000",
          "deletions_count": 1,
          "description": "Fix responsive styling on homepage",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 9,
          "message": "chore(website): Fix responsive styling on homepage",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "73dc9d55803733c460f42ce38e09b8c7c8344680",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-01 23:44:37 +0000",
          "deletions_count": 5,
          "description": "Fix accessing custom front-matter in docs",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 13,
          "message": "chore(website): Fix accessing custom front-matter in docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "3fc6196a6b6e2df7c76e9d5924377a2054dcb5e2",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-02 09:58:25 +0000",
          "deletions_count": 62,
          "description": "Build RPM packages for ARM",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 220,
          "message": "chore(operations): Build RPM packages for ARM (#1292)",
          "pr_number": 1292,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a6668b0c1db009b537c989ef95d8c4e616440cb9",
          "type": "chore"
        },
        {
          "author": "Bruce Guenter",
          "breaking_change": false,
          "date": "2019-12-02 08:27:53 +0000",
          "deletions_count": 338,
          "description": "Refactor the sinks' request_* configuration",
          "files_count": 12,
          "group": "enhancement",
          "insertions_count": 321,
          "message": "enhancement(config): Refactor the sinks' request_* configuration (#1187)",
          "pr_number": 1187,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "config"
          },
          "sha": "62f9db5ba46a0824ed0e979743bc8aaec8e05010",
          "type": "enhancement"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-02 19:23:02 +0000",
          "deletions_count": 2,
          "description": "Fix Raspbian id capitalization",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Fix Raspbian id capitalization (#1295)",
          "pr_number": 1295,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "cbac5010444357dae078b299991304ca8055889c",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-02 22:44:56 +0000",
          "deletions_count": 0,
          "description": "Run `package-rpm*` jobs explicitly",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 3,
          "message": "fix(operations): Run `package-rpm*` jobs explicitly (#1298)",
          "pr_number": 1298,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "a0eec9935a8a2d0409e23c6cb23cba807b16a7df",
          "type": "fix"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-03 11:28:27 +0000",
          "deletions_count": 16,
          "description": "Fix section links",
          "files_count": 9,
          "group": "docs",
          "insertions_count": 24,
          "message": "docs: Fix section links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5ae3036f0a0de24aeeb92135621c877428bcfa02",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-02 11:36:52 +0000",
          "deletions_count": 1,
          "description": "Fix browse downloads link",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(website): Fix browse downloads link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "1f52116c3c40dcc439bd8f32c9cdf2a0a3b197d7",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 12:29:30 +0000",
          "deletions_count": 11,
          "description": "Add slugify method to mimic Docusaurus hashing logic for links",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 23,
          "message": "chore(website): Add slugify method to mimic Docusaurus hashing logic for links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "bd865b06bc2ff68edb3a131a574572b88fcc8b87",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 12:33:09 +0000",
          "deletions_count": 20,
          "description": "Fix buffers and batches hash link",
          "files_count": 10,
          "group": "chore",
          "insertions_count": 20,
          "message": "chore(website): Fix buffers and batches hash link",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "9d38c48a10b9d3deb8d35b6e97002cab4a03b885",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 13:30:43 +0000",
          "deletions_count": 2,
          "description": "Use the Rust regex tester, closes #634",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Use the Rust regex tester, closes #634",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "0c0f07265ad4020d68116c14113d917499ca862f",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 13:35:39 +0000",
          "deletions_count": 16,
          "description": "Fix example regex",
          "files_count": 6,
          "group": "chore",
          "insertions_count": 16,
          "message": "chore(website): Fix example regex",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "d962fa60fd1e71cd2c9c02fc4e1ead2fd0a5086c",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-03 15:55:03 +0000",
          "deletions_count": 35,
          "description": "Pass `TaskExecutor` to transform",
          "files_count": 25,
          "group": "chore",
          "insertions_count": 67,
          "message": "chore(topology): Pass `TaskExecutor` to transform (#1144)",
          "pr_number": 1144,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "topology"
          },
          "sha": "17a27b315b4e65f687adb0d64d2b6c5cf8890a95",
          "type": "chore"
        },
        {
          "author": "Binary Logic",
          "breaking_change": false,
          "date": "2019-12-03 17:28:50 +0000",
          "deletions_count": 223,
          "description": "Add community page with mailing list",
          "files_count": 13,
          "group": "chore",
          "insertions_count": 271,
          "message": "chore(website): Add community page with mailing list (#1309)",
          "pr_number": 1309,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "cf95723d77ba4bd3fa819dd45fa7676bd1a7d19d",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 17:45:00 +0000",
          "deletions_count": 2,
          "description": "Responsive styling for community page",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 14,
          "message": "chore(wensite): Responsive styling for community page",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "wensite"
          },
          "sha": "c912f16f1cbd924db1e800498dbfb240e9211212",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 18:04:41 +0000",
          "deletions_count": 5,
          "description": "Fix slide out main nav menu link labels",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 7,
          "message": "chore(website): Fix slide out main nav menu link labels",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "4c1718431e887c9a9f58392428cde6c2a33e5070",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 18:53:47 +0000",
          "deletions_count": 14,
          "description": "Re-add components list",
          "files_count": 5,
          "group": "chore",
          "insertions_count": 207,
          "message": "chore(website): Re-add components list",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "14ebf42842d90f937df7efa88f7acea1bb1859e8",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 21:44:27 +0000",
          "deletions_count": 29,
          "description": "Use ${ENV_VAR} syntax in relavant examples",
          "files_count": 9,
          "group": "docs",
          "insertions_count": 33,
          "message": "docs: Use ${ENV_VAR} syntax in relavant examples",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "6e60b2fab0de568ef47c5afdd606a60c3069531d",
          "type": "docs"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-12-04 12:21:43 +0000",
          "deletions_count": 9,
          "description": "Performance optimisations in metric buffer",
          "files_count": 2,
          "group": "perf",
          "insertions_count": 165,
          "message": "perf(metric data model): Performance optimisations in metric buffer (#1290)",
          "pr_number": 1290,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "metric data model"
          },
          "sha": "fcf6356f11ac7d80a5c378aeceabd6cf72168ef1",
          "type": "perf"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-03 23:24:55 +0000",
          "deletions_count": 5,
          "description": "Fix nav width",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 10,
          "message": "chore(website): Fix nav width",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "690d798e8cc4d08457b5ad3dd3fcee4da7fea4b3",
          "type": "chore"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-04 10:56:01 +0000",
          "deletions_count": 6,
          "description": "Update README with new links",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 8,
          "message": "docs: Update README with new links",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "53d2a9ca0ff85c8d39cf9b312265c859f079c170",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-04 13:23:44 +0000",
          "deletions_count": 113,
          "description": "Add `SinkContext` to `SinkConfig`",
          "files_count": 23,
          "group": "chore",
          "insertions_count": 146,
          "message": "chore(topology): Add `SinkContext` to `SinkConfig` (#1306)",
          "pr_number": 1306,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "topology"
          },
          "sha": "00e21e83c54d2ca5e0b50b3b96a3390e761bf2dd",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-04 19:18:34 +0000",
          "deletions_count": 9,
          "description": "Initial `new_relic_logs` sink implementation",
          "files_count": 13,
          "group": "feat",
          "insertions_count": 1166,
          "message": "feat(new sink): Initial `new_relic_logs` sink implementation (#1303)",
          "pr_number": 1303,
          "scope": {
            "category": "sink",
            "component_name": null,
            "component_type": "sink",
            "name": "new sink"
          },
          "sha": "52e4f176f62c305a6d0adcf6fa1f5b08bd2466dc",
          "type": "feat"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-04 19:48:24 +0000",
          "deletions_count": 11,
          "description": "Fix NR build signature",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 12,
          "message": "chore: Fix NR build signature",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4c1d8ceaef63fc9f73e5e568773bf569f6c2f460",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-04 15:29:04 +0000",
          "deletions_count": 182,
          "description": "Add map to ServiceBuilder and s3",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 346,
          "message": "chore: Add map to ServiceBuilder and s3 (#1189)",
          "pr_number": 1189,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "772672e65920de3c0f13fa5b86c9c428b2d3fbfb",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": true,
          "date": "2019-12-04 22:34:43 +0000",
          "deletions_count": 3,
          "description": "Rename `datadog` sink to `datadog_metrics`",
          "files_count": 1,
          "group": "breaking change",
          "insertions_count": 3,
          "message": "fix(datadog_metrics sink)!: Rename `datadog` sink to `datadog_metrics` (#1314)",
          "pr_number": 1314,
          "scope": {
            "category": "sink",
            "component_name": "datadog_metrics",
            "component_type": "sink",
            "name": "datadog_metrics sink"
          },
          "sha": "59fd318f227524a84a7520bbae004d2c75156365",
          "type": "fix"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-04 15:39:15 +0000",
          "deletions_count": 166,
          "description": "Sync with new toggle changes",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 2,
          "message": "chore(website): Sync with new toggle changes",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "e76083548a2d46664acd67a8e40f1835614d94c5",
          "type": "chore"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-12-05 09:31:01 +0000",
          "deletions_count": 0,
          "description": "Send aggregated distributions to Datadog",
          "files_count": 1,
          "group": "enhancement",
          "insertions_count": 231,
          "message": "enhancement(datadog_metrics sink): Send aggregated distributions to Datadog (#1263)",
          "pr_number": 1263,
          "scope": {
            "category": "sink",
            "component_name": "datadog_metrics",
            "component_type": "sink",
            "name": "datadog_metrics sink"
          },
          "sha": "5822ee199bafbc2558491d5ba9682b8f10ed95d0",
          "type": "enhancement"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-05 13:28:26 +0000",
          "deletions_count": 7,
          "description": "Test & validate subcommands without args target default path",
          "files_count": 3,
          "group": "enhancement",
          "insertions_count": 32,
          "message": "enhancement(cli): Test & validate subcommands without args target default path (#1313)",
          "pr_number": 1313,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "cli"
          },
          "sha": "e776d3a404810935810983caf888aa86138b448b",
          "type": "enhancement"
        },
        {
          "author": "Alexey Suslov",
          "breaking_change": false,
          "date": "2019-12-05 17:51:10 +0000",
          "deletions_count": 1,
          "description": "Fix statsd binding to loopback only",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(statsd sink): Fix statsd binding to loopback only (#1316)",
          "pr_number": 1316,
          "scope": {
            "category": "sink",
            "component_name": "statsd",
            "component_type": "sink",
            "name": "statsd sink"
          },
          "sha": "58d6e976cf81f2175e7fd6cc6d4c85c9e2bc88eb",
          "type": "fix"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-06 14:38:03 +0000",
          "deletions_count": 5,
          "description": "Fix multiple sources test",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 5,
          "message": "chore(testing): Fix multiple sources test (#1322)",
          "pr_number": 1322,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "324012b74c8879b1185ace3c5c36d9170222597e",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-12-06 15:54:01 +0000",
          "deletions_count": 2,
          "description": "Document `drop_field`",
          "files_count": 3,
          "group": "docs",
          "insertions_count": 42,
          "message": "docs(json_parser transform): Document `drop_field` (#1323)",
          "pr_number": 1323,
          "scope": {
            "category": "transform",
            "component_name": "json_parser",
            "component_type": "transform",
            "name": "json_parser transform"
          },
          "sha": "dc21766356a422e694287bff1b70fde8a49e74af",
          "type": "docs"
        },
        {
          "author": "Ben Johnson",
          "breaking_change": false,
          "date": "2019-12-07 10:53:05 +0000",
          "deletions_count": 207,
          "description": "Update to docusaurus 2.0.0-alpha.39",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 198,
          "message": "chore(website): Update to docusaurus 2.0.0-alpha.39",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "website"
          },
          "sha": "8d15fdd267df44ac9f5079e7b6a5a2bc122b9e1f",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-09 13:11:56 +0000",
          "deletions_count": 29,
          "description": "Add \"default-{musl,msvc}\" features",
          "files_count": 7,
          "group": "chore",
          "insertions_count": 93,
          "message": "chore(operations): Add \"default-{musl,msvc}\" features (#1331)",
          "pr_number": 1331,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "2c6982502c75409806da7d74a4cc019f2c60ed08",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-09 11:06:57 +0000",
          "deletions_count": 1,
          "description": "Fix validating environment title",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 1,
          "message": "docs: Fix validating environment title",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "fb7f1f5743e464294c62d11e1be0d26e309f2061",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-09 15:35:33 +0000",
          "deletions_count": 87,
          "description": "Use LLVM-9 from the distribution repository",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 31,
          "message": "chore(operations): Use LLVM-9 from the distribution repository (#1333)",
          "pr_number": 1333,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "8cb9ec9406315d87c10f297da115ced93c2418f1",
          "type": "chore"
        },
        {
          "author": "Kruno Tomola Fabro",
          "breaking_change": false,
          "date": "2019-12-09 13:26:38 +0000",
          "deletions_count": 44,
          "description": "Initial `splunk_hec` source implementation",
          "files_count": 7,
          "group": "feat",
          "insertions_count": 1142,
          "message": "feat(new source): Initial `splunk_hec` source implementation",
          "pr_number": null,
          "scope": {
            "category": "source",
            "component_name": null,
            "component_type": "source",
            "name": "new source"
          },
          "sha": "a68c9781a12cd35f2ee1cd7686320d1bd6e52c05",
          "type": "feat"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-09 17:00:16 +0000",
          "deletions_count": 63,
          "description": "Use LLVM from an archive instead of Git",
          "files_count": 3,
          "group": "chore",
          "insertions_count": 33,
          "message": "chore(operations): Use LLVM from an archive instead of Git (#1334)",
          "pr_number": 1334,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "9c53a5dd65c4711c58a5afede4a23c048c4bed4d",
          "type": "chore"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-09 10:57:26 +0000",
          "deletions_count": 7,
          "description": "Update `shiplift 0.6`",
          "files_count": 2,
          "group": "chore",
          "insertions_count": 7,
          "message": "chore(docker source): Update `shiplift 0.6` (#1335)",
          "pr_number": 1335,
          "scope": {
            "category": "source",
            "component_name": "docker",
            "component_type": "source",
            "name": "docker source"
          },
          "sha": "86abe53556fd7647717ddfecc21834f87adaa62b",
          "type": "chore"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-09 16:04:27 +0000",
          "deletions_count": 54,
          "description": "Rewrite getting started guide.",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 76,
          "message": "docs: Rewrite getting started guide. (#1332)",
          "pr_number": 1332,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "4b93936dc588438a3023a6d86075ca75a33921f3",
          "type": "docs"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-09 16:05:58 +0000",
          "deletions_count": 18,
          "description": "Update contribution guide for docs",
          "files_count": 2,
          "group": "docs",
          "insertions_count": 53,
          "message": "docs: Update contribution guide for docs",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5461ff419b9587264bbce823af227e1a3007a578",
          "type": "docs"
        },
        {
          "author": "Lucio Franco",
          "breaking_change": false,
          "date": "2019-12-09 11:06:50 +0000",
          "deletions_count": 0,
          "description": "Add missing rate limited log",
          "files_count": 1,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(grok_parser transform): Add missing rate limited log (#1336)",
          "pr_number": 1336,
          "scope": {
            "category": "transform",
            "component_name": "grok_parser",
            "component_type": "transform",
            "name": "grok_parser transform"
          },
          "sha": "285b967ab228a94b4a140803cec38b71bb59ad14",
          "type": "fix"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-10 09:34:53 +0000",
          "deletions_count": 2,
          "description": "Edit getting started guide",
          "files_count": 1,
          "group": "docs",
          "insertions_count": 2,
          "message": "docs: Edit getting started guide",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "137c51de9122c32cbbfba983f3068b6df1d6a68e",
          "type": "docs"
        },
        {
          "author": "Ashley Jeffs",
          "breaking_change": false,
          "date": "2019-12-10 16:42:08 +0000",
          "deletions_count": 39,
          "description": "Fix unit test spec rendering",
          "files_count": 5,
          "group": "docs",
          "insertions_count": 43,
          "message": "docs: Fix unit test spec rendering",
          "pr_number": null,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "core"
          },
          "sha": "5c2c0af26554258d746051a5861ce9aaa869a8be",
          "type": "docs"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-11 17:09:12 +0000",
          "deletions_count": 44,
          "description": "Build `msi` package for Vector",
          "files_count": 23,
          "group": "chore",
          "insertions_count": 780,
          "message": "chore(operations): Build `msi` package for Vector (#1345)",
          "pr_number": 1345,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "da89fa9fd801ff6f87412fb78d686936115b241c",
          "type": "chore"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-12-11 15:56:33 +0000",
          "deletions_count": 16,
          "description": "Remove sleeps from topology tests",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 1,
          "message": "fix(testing): Remove sleeps from topology tests (#1346)",
          "pr_number": 1346,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "testing"
          },
          "sha": "8561d42eba3c5d30d57ab47c6454f19978c5ea4b",
          "type": "fix"
        },
        {
          "author": "Luke Steensen",
          "breaking_change": false,
          "date": "2019-12-11 16:30:27 +0000",
          "deletions_count": 21,
          "description": "Detect and read gzipped files",
          "files_count": 7,
          "group": "feat",
          "insertions_count": 127,
          "message": "feat(file source): Detect and read gzipped files (#1344)",
          "pr_number": 1344,
          "scope": {
            "category": "source",
            "component_name": "file",
            "component_type": "source",
            "name": "file source"
          },
          "sha": "8c991293ee2cd478fc639e96e6c27df794a0c5ec",
          "type": "feat"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-12 15:49:31 +0000",
          "deletions_count": 11,
          "description": "Put `etc` directory only to Linux archives",
          "files_count": 2,
          "group": "fix",
          "insertions_count": 11,
          "message": "fix(operations): Put `etc` directory only to Linux archives (#1352)",
          "pr_number": 1352,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "cbba6f180a583d4d7f236b64b77fdd6406bc6c63",
          "type": "fix"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-12 16:22:49 +0000",
          "deletions_count": 1,
          "description": "Allow passing features to `make build`",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Allow passing features to `make build` (#1356)",
          "pr_number": 1356,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "1f9b9cf6eddf27557bcaa6a1e1139da0137dcb4c",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-12 16:53:31 +0000",
          "deletions_count": 1,
          "description": "Compress release archives with `gzip -9`",
          "files_count": 1,
          "group": "chore",
          "insertions_count": 1,
          "message": "chore(operations): Compress release archives with `gzip -9` (#1294)",
          "pr_number": 1294,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "779d727fb49c907d6babbd8ed48e0db2cec14604",
          "type": "chore"
        },
        {
          "author": "Alexander Rodin",
          "breaking_change": false,
          "date": "2019-12-12 19:11:22 +0000",
          "deletions_count": 1,
          "description": "Add notices for OpenSSL to the license for binary distributions",
          "files_count": 4,
          "group": "chore",
          "insertions_count": 22,
          "message": "chore(operations): Add notices for OpenSSL to the license for binary distributions (#1351)",
          "pr_number": 1351,
          "scope": {
            "category": "core",
            "component_name": null,
            "component_type": null,
            "name": "operations"
          },
          "sha": "f8ad1b5a0edcf214865e4ba1133b3a0df1465905",
          "type": "chore"
        }
      ],
      "compare_url": "https://github.com/timberio/vector/compare/v0.5.0...v0.6.0",
      "date": "2019-12-09",
      "deletions_count": 9213,
      "insertions_count": 22141,
      "last_version": "0.5.0",
      "posts": [
        {
          "author_id": "ben",
          "date": "2019-11-19",
          "description": "Vector now supports ARM architectures on the Linux platform! These\narchitectures are widely used in embeded devices and recently started to get\ntraction on servers. To get started, you can follow the installation\ninstructions for your preferred method:",
          "id": "arm-support-on-linux",
          "path": "website/blog/2019-11-19-arm-support-on-linux.md",
          "permalink": "https://vector.dev/blog/arm-support-on-linux",
          "tags": [
            "type: announcement",
            "domain: platforms",
            "platform: arm"
          ],
          "title": "ARMv7 & ARM64 Support on Linux"
        },
        {
          "author_id": "ben",
          "date": "2019-11-21",
          "description": "We're excited to announce that Vector can now be installed on Windows!\nTo get started, check out the Windows installation instructions\nor head over to the releases section and download the\nappropriate Windows archive. Just like on Linux, installation on Windows is\nquick and easy. Let us know what you think!.",
          "id": "windows-support",
          "path": "website/blog/2019-11-21-windows-support.md",
          "permalink": "https://vector.dev/blog/windows-support",
          "tags": [
            "type: announcement",
            "domain: platforms",
            "platform: windows"
          ],
          "title": "Windows Support Is Here!"
        },
        {
          "author_id": "ashley",
          "date": "2019-11-25",
          "description": "Today we're excited to announce beta support for unit testing Vector\nconfigurations, allowing you to define tests directly within your Vector\nconfiguration file. These tests are used to assert the output from topologies of\ntransform components given certain input events, ensuring\nthat your configuration behavior does not regress; a very powerful feature for\nmission-critical production pipelines that are collaborated on.",
          "id": "unit-testing-vector-config-files",
          "path": "website/blog/2019-11-25-unit-testing-vector-config-files.md",
          "permalink": "https://vector.dev/blog/unit-testing-vector-config-files",
          "tags": [
            "type: announcement",
            "domain: config"
          ],
          "title": "Unit Testing Your Vector Config Files"
        }
      ],
      "type": "initial dev",
      "type_url": "https://semver.org/#spec-item-4",
      "upgrade_guides": [
        {
          "body": "<p>\nThe `file` and `console` sinks now require an explicit `encoding` option. The previous implicit nature was confusing and this should eliminate any suprises related to the output encoding format. Migration is easy:\n</p>\n\n<pre>\n [sinks.my_console_sink]\n   type = \"console\"\n+  encoding = \"json\" # or \"text\"\n\n\n [sinks.my_file_sink]\n   type = \"file\"\n+  encoding = \"json\" # or \"text\"\n</pre>\n",
          "commits": [

          ],
          "id": "encoding-guide",
          "title": "The `file` and `console` sinks now require `encoding`"
        },
        {
          "body": "<p>\nThe `datadog` sink was incorrectly named since we'll be adding future support for DataDog logs. Migrating is as simple as renaming your sink:\n</p>\n\n<pre>\n [sinks.my_sink]\n-  type = \"datadog\"\n+  type = \"datadog_metrics\"\n</pre>\n",
          "commits": [

          ],
          "id": "datadog-guide",
          "title": "The `datadog` sink has been renamed to `datadog_metrics`"
        }
      ],
      "version": "0.6.0"
    }
  },
  "sinks": {
    "aws_cloudwatch_logs": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to [Amazon Web Service's CloudWatch Logs service][urls.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html).",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_cloudwatch_logs_sink",
      "name": "aws_cloudwatch_logs",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "aws_cloudwatch_metrics": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Streams metric events to [Amazon Web Service's CloudWatch Metrics service][urls.aws_cw_metrics] via the [`PutMetricData` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricData.html).",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "aws_cloudwatch_metrics_sink",
      "name": "aws_cloudwatch_metrics",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "aws_kinesis_firehose": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to [Amazon Web Service's Kinesis Data Firehose][urls.aws_kinesis_data_firehose] via the [`PutRecordBatch` API endpoint](https://docs.aws.amazon.com/firehose/latest/APIReference/API_PutRecordBatch.html).",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_kinesis_firehose_sink",
      "name": "aws_kinesis_firehose",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "aws_kinesis_streams": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to [Amazon Web Service's Kinesis Data Stream service][urls.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html).",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_kinesis_streams_sink",
      "name": "aws_kinesis_streams",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "aws_s3": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to [Amazon Web Service's S3 service][urls.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html).",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "aws_s3_sink",
      "name": "aws_s3",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "blackhole": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Streams log and metric events to a blackhole that simply discards data, designed for testing and benchmarking purposes.",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "test",
      "id": "blackhole_sink",
      "name": "blackhole",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "clickhouse": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Batches log events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http].",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "clickhouse_sink",
      "name": "clickhouse",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "console": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Streams log and metric events to [standard output streams][urls.standard_streams], such as `STDOUT` and `STDERR`.",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "test",
      "id": "console_sink",
      "name": "console",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "datadog_metrics": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Batches metric events to [Datadog's][urls.datadog] metrics service using [HTTP API](https://docs.datadoghq.com/api/?lang=bash#metrics).",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "datadog_metrics_sink",
      "name": "datadog_metrics",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "elasticsearch": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Batches log events to [Elasticsearch][urls.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html).",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "elasticsearch_sink",
      "name": "elasticsearch",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "file": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Streams log events to a file.",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "file_sink",
      "name": "file",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "gcp_pubsub": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Batches log events to [Google Cloud Platform's Pubsub service][urls.gcp_pubsub] via the [REST Interface][urls.gcp_pubsub_rest].",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "gcp_pubsub_sink",
      "name": "gcp_pubsub",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "http": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to a generic HTTP endpoint.",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "http_sink",
      "name": "http",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "kafka": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "description": "Streams log events to [Apache Kafka][urls.kafka] via the [Kafka protocol][urls.kafka_protocol].",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "kafka_sink",
      "name": "kafka",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "new_relic_logs": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to [New Relic's log service][urls.new_relic] via their [log API][urls.new_relic_log_api].",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "new_relic_logs_sink",
      "name": "new_relic_logs",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "prometheus": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Exposes metric events to [Prometheus][urls.prometheus] metrics service.",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "prometheus_sink",
      "name": "prometheus",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "socket": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Streams log events to a socket, such as a TCP, UDP, or Unix socket.",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "socket_sink",
      "name": "socket",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "splunk_hec": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "description": "Batches log events to a [Splunk's HTTP Event Collector][urls.splunk_hec].",
      "event_types": [
        "log"
      ],
      "function_category": "transmit",
      "id": "splunk_hec_sink",
      "name": "splunk_hec",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "statsd": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Streams metric events to [StatsD][urls.statsd] metrics service.",
      "event_types": [
        "metric"
      ],
      "function_category": "transmit",
      "id": "statsd_sink",
      "name": "statsd",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    },
    "vector": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Streams log events to another downstream [`vector` source][docs.sources.vector].",
      "event_types": [
        "log"
      ],
      "function_category": "proxy",
      "id": "vector_sink",
      "name": "vector",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "sink",
      "unsupported_operating_systems": [

      ]
    }
  },
  "sources": {
    "docker": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through the docker engine daemon and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "docker_source",
      "name": "docker",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "file": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through one or more local files and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "file_source",
      "name": "file",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "journald": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through log records from journald and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "journald_source",
      "name": "journald",
      "operating_systems": [
        "linux"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [
        "macos",
        "windows"
      ]
    },
    "kafka": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Ingests data through Kafka 0.9 or later and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "collect",
      "id": "kafka_source",
      "name": "kafka",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "prometheus": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through the Prometheus text exposition format and outputs metric events.",
      "event_types": [
        "metric"
      ],
      "function_category": "receive",
      "id": "prometheus_source",
      "name": "prometheus",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "socket": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through a socket, such as a TCP, UDP, or Unix socket and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "socket_source",
      "name": "socket",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "splunk_hec": {
      "beta": true,
      "delivery_guarantee": "at_least_once",
      "description": "Ingests data through the [Splunk HTTP Event Collector protocol][urls.splunk_hec_protocol] and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "splunk_hec_source",
      "name": "splunk_hec",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "statsd": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through the StatsD UDP protocol and outputs metric events.",
      "event_types": [
        "metric"
      ],
      "function_category": "receive",
      "id": "statsd_source",
      "name": "statsd",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "stdin": {
      "beta": false,
      "delivery_guarantee": "at_least_once",
      "description": "Ingests data through standard input (STDIN) and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "stdin_source",
      "name": "stdin",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "syslog": {
      "beta": false,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through the Syslog 5424 protocol and outputs log events.",
      "event_types": [
        "log"
      ],
      "function_category": "receive",
      "id": "syslog_source",
      "name": "syslog",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    },
    "vector": {
      "beta": true,
      "delivery_guarantee": "best_effort",
      "description": "Ingests data through another upstream [`vector` sink][docs.sinks.vector] and outputs log and metric events.",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "proxy",
      "id": "vector_source",
      "name": "vector",
      "operating_systems": [
        "linux",
        "macos",
        "windows"
      ],
      "service_provider": null,
      "status": "beta",
      "type": "source",
      "unsupported_operating_systems": [

      ]
    }
  },
  "team": [
    {
      "avatar": "https://github.com/a-rodin.png",
      "github": "https://github.com/a-rodin",
      "id": "alex",
      "name": "Alexander"
    },
    {
      "avatar": "https://github.com/loony-bean.png",
      "github": "https://github.com/loony-bean",
      "id": "alexey",
      "name": "Alexey"
    },
    {
      "avatar": "https://github.com/Jeffail.png",
      "github": "https://github.com/Jeffail",
      "id": "ashley",
      "name": "Ashley"
    },
    {
      "avatar": "https://github.com/binarylogic.png",
      "github": "https://github.com/binarylogic",
      "id": "ben",
      "name": "Ben"
    },
    {
      "avatar": "https://github.com/bruceg.png",
      "github": "https://github.com/bruceg",
      "id": "bruce",
      "name": "Bruce"
    },
    {
      "avatar": "https://github.com/ktff.png",
      "github": "https://github.com/ktff",
      "id": "kruno",
      "name": "Kruno"
    },
    {
      "avatar": "https://github.com/LucioFranco.png",
      "github": "https://github.com/LucioFranco",
      "id": "lucio",
      "name": "Lucio"
    },
    {
      "avatar": "https://github.com/lukesteensen.png",
      "github": "https://github.com/lukesteensen",
      "id": "luke",
      "name": "Luke"
    }
  ],
  "transforms": {
    "add_fields": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to add one or more log fields.",
      "event_types": [
        "log"
      ],
      "function_category": "shape",
      "id": "add_fields_transform",
      "name": "add_fields",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "add_tags": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts metric events and allows you to add one or more metric tags.",
      "event_types": [
        "metric"
      ],
      "function_category": "shape",
      "id": "add_tags_transform",
      "name": "add_tags",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "ansi_stripper": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to strips ANSI characters from the specified field.",
      "event_types": [
        "log"
      ],
      "function_category": "sanitize",
      "id": "ansi_stripper_transform",
      "name": "ansi_stripper",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "aws_ec2_metadata": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to enrich logs with AWS EC2 instance metadata.",
      "event_types": [
        "log"
      ],
      "function_category": "enrich",
      "id": "aws_ec2_metadata_transform",
      "name": "aws_ec2_metadata",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "coercer": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to coerce log fields into fixed types.",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "coercer_transform",
      "name": "coercer",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "concat": {
      "beta": true,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to concat (substrings) of other fields to a new one.",
      "event_types": [
        "log"
      ],
      "function_category": [
        "filter"
      ],
      "id": "concat_transform",
      "name": "concat",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "field_filter": {
      "beta": true,
      "delivery_guarantee": null,
      "description": "Accepts log and metric events and allows you to filter events by a log field's value.",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "filter",
      "id": "field_filter_transform",
      "name": "field_filter",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "geoip": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to enrich events with geolocation data from the MaxMind GeoIP2 and GeoLite2 city databases.",
      "event_types": [
        "log"
      ],
      "function_category": "enrich",
      "id": "geoip_transform",
      "name": "geoip",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "grok_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to parse a log field value with [Grok][urls.grok].",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "grok_parser_transform",
      "name": "grok_parser",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "json_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to parse a log field value as JSON.",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "json_parser_transform",
      "name": "json_parser",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "log_to_metric": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to convert logs into one or more metrics.",
      "event_types": [
        "log",
        "metric"
      ],
      "function_category": "convert",
      "id": "log_to_metric_transform",
      "name": "log_to_metric",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "logfmt_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to extract data from a logfmt-formatted log field.",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "logfmt_parser_transform",
      "name": "logfmt_parser",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "lua": {
      "beta": true,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to transform events with a full embedded [Lua][urls.lua] engine.",
      "event_types": [
        "log"
      ],
      "function_category": "program",
      "id": "lua_transform",
      "name": "lua",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "regex_parser": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to parse a log field's value with a [Regular Expression][urls.regex].",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "regex_parser_transform",
      "name": "regex_parser",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "remove_fields": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to remove one or more log fields.",
      "event_types": [
        "log"
      ],
      "function_category": "shape",
      "id": "remove_fields_transform",
      "name": "remove_fields",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "remove_tags": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts metric events and allows you to remove one or more metric tags.",
      "event_types": [
        "metric"
      ],
      "function_category": "shape",
      "id": "remove_tags_transform",
      "name": "remove_tags",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "sampler": {
      "beta": true,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to sample events with a configurable rate.",
      "event_types": [
        "log"
      ],
      "function_category": "filter",
      "id": "sampler_transform",
      "name": "sampler",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "beta",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "split": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to split a field's value on a given separator and zip the tokens into ordered field names.",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "split_transform",
      "name": "split",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    },
    "tokenizer": {
      "beta": false,
      "delivery_guarantee": null,
      "description": "Accepts log events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names.",
      "event_types": [
        "log"
      ],
      "function_category": "parse",
      "id": "tokenizer_transform",
      "name": "tokenizer",
      "operating_systems": [

      ],
      "service_provider": null,
      "status": "prod-ready",
      "type": "transform",
      "unsupported_operating_systems": [

      ]
    }
  }
};
