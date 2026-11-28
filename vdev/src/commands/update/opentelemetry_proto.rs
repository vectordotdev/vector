use anyhow::Result;
use clap::Args;

use super::released_tree::{self, VendorSpec};

/// Refresh vendored OpenTelemetry protobuf definitions from a release tag.
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
                repo_url: "https://github.com/open-telemetry/opentelemetry-proto",
                dest: "lib/opentelemetry-proto/src/proto/opentelemetry-proto/opentelemetry/proto",
                src_rel: "opentelemetry/proto",
                tag_pattern: r"^v[0-9]+\.[0-9]+\.[0-9]+$",
                title: "OpenTelemetry protobuf definitions",
                command: "update opentelemetry-proto",
            },
            self.tag.as_deref(),
        )
    }
}
