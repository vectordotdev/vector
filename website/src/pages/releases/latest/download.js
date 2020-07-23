import React from "react";
import { Redirect } from "@docusaurus/router";

import useDocusaurusContext from "@docusaurus/useDocusaurusContext";

function LatestDownload() {
  const context = useDocusaurusContext();
  const { siteConfig = {} } = context;
  const {
    metadata: { latest_release: latestRelease },
  } = siteConfig.customFields;

  return <Redirect to={`/releases/${latestRelease.version}/download/`} />;
}

export default LatestDownload;
