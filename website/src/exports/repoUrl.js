import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

export default function useBaseUrl(url) {
  const {siteConfig} = useDocusaurusContext();
  const githubHost = siteConfig.githubHost || 'github.com';

  return `https://${githubHost}/${siteConfig.organizationName}/${siteConfig.projectName}`
}