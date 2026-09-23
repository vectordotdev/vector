use anyhow::Result;
use clap::Args;

use super::released_tree::{self, VendorSpec};

/// Refresh vendored Datadog Agent trace protobufs from a release tag.
#[derive(Args, Debug)]
#[command()]
pub(super) struct Cli {
    /// Release tag to vendor. Defaults to the highest `MAJOR.MINOR.PATCH` tag.
    tag: Option<String>,
}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        released_tree::refresh(
            &VendorSpec {
                repo_url: "https://github.com/DataDog/datadog-agent",
                dest: "lib/datadog-proto/proto/datadog/trace",
                src_rel: "pkg/proto/datadog/trace",
                tag_pattern: r"^[0-9]+\.[0-9]+\.[0-9]+$",
                title: "Datadog Agent trace protobufs",
                command: "update datadog-proto",
            },
            self.tag.as_deref(),
        )
    }
}
