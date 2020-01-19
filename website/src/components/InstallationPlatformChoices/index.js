import React from 'react';

import Jump from '@site/src/components/Jump';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function getInstallation() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {installation}} = siteConfig.customFields;

  return installation;
}

function ArchChoices({arch, docker, os, packageManager}) {
  const {containers, downloads, package_managers: packageManagers} = getInstallation();

  const archiveDownload = downloads.filter(download => (
    download.arch.toLowerCase() == arch.toLowerCase() &&
      download.os.toLowerCase() == os.toLowerCase() &&
      download.type == "archive")
  )[0];

  const dockerContainer = containers.find(c => c.id == "docker");
  const dockerSupported = dockerContainer.archs.includes(arch) && dockerContainer.oss.includes(os);
  const packageManagerSupported = packageManager && packageManagers.find(p => p.name == packageManager).archs.includes(arch);

  return (
    <div>
      {packageManagerSupported && <Jump to={`/docs/setup/installation/package-managers/${packageManager.toLowerCase()}/?arch=${arch}`}>
        <i className="feather icon-package"></i> {packageManager} ({arch}) <span className="badge badge--primary">recommended</span>
      </Jump>}
      {!packageManagerSupported && dockerSupported && <Jump to="/docs/setup/installation/containers/docker/">
        <i className="feather icon-terminal"></i> Docker ({arch}) <span className="badge badge--primary">recommended</span>
      </Jump>}
      {!packageManagerSupported && !dockerSupported && <Jump to={`/docs/setup/installation/manual/from-archives/?file_name=${archiveDownload.file_name}`}>
        <i className="feather icon-terminal"></i> From an Archive ({arch})  <span className="badge badge--primary">recommended</span>
      </Jump>}

      <p>Alternatively, you can use your preferred method:</p>

      {(packageManagerSupported && dockerSupported) && <Jump to="/docs/setup/installation/containers/docker/" size="sm">
        <i className="feather icon-package"></i> Docker ({arch})
      </Jump>}

      {(packageManagerSupported || dockerSupported) && <Jump to={`/docs/setup/installation/manual/from-archives/?file_name=${archiveDownload.file_name}`} size="sm">
        <i className="feather icon-terminal"></i> From an Archive ({arch})
      </Jump>}

      <Jump to="/docs/setup/installation/manual/from-source/" size="sm">
        <i className="feather icon-terminal"></i> From Source
      </Jump>
    </div>
  );
}

function InstallationPlatformChoices({docker, os, packageManager}) {
  const {downloads} = getInstallation();
  const archiveDownloads = downloads.filter(download => (download.os.toLowerCase() == os.toLowerCase() && download.type == "archive"));
  const archs = archiveDownloads.map(download => download.arch);

  return (
    <div>
      <Tabs
        block={true}
        defaultValue={archs[0]}
        urlKey="os"
        values={archs.map(arch => ({label: <><i className="feather icon-cpu"></i> {arch}</>, value: arch}))}>

        {archs.map((arch, idx) => (
          <TabItem key={idx} value={arch}>
            <ArchChoices arch={arch} docker={docker} os={os} packageManager={packageManager} />
          </TabItem>
        ))}
      </Tabs>
    </div>
  );
}


export default InstallationPlatformChoices;
export {ArchChoices};
