use anyhow::Result;
use clap::Args;

use super::released_tree::{self, VendorSpec};

/// Refresh the vendored Datadog metrics intake protobuf from a release tag.
#[derive(Args, Debug)]
#[command()]
pub(super) struct Cli {
    /// Release tag to vendor. Defaults to the highest `vMAJOR.MINOR.PATCH` tag.
    tag: Option<String>,
}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        released_tree::refresh(
            &VendorSpec {
                repo_url: "https://github.com/DataDog/agent-payload",
                dest: "lib/datadog-proto/proto/datadog/metrics",
                src_rel: "proto/metrics",
                tag_pattern: r"^v[0-9]+\.[0-9]+\.[0-9]+$",
                title: "Datadog metrics intake protobuf",
                command: "update datadog-metrics-proto",
            },
            self.tag.as_deref(),
        )
    }
}
