import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

export function fetchNewRelease() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {latest_release: latestRelease}} = siteConfig.customFields;
  const releaseDate = Date.parse(latestRelease.date);
  const releaseNow = new Date();
  const releaseDiffTime = Math.abs(releaseNow - releaseDate);
  const releaseDiffDays = Math.ceil(releaseDiffTime / (1000 * 60 * 60 * 24));

  let releaseViewedAt = null;

  if (typeof window !== 'undefined') {
    releaseViewedAt = new Date(parseInt(window.localStorage.getItem('releaseViewedAt') || '0'));
  }

  if (releaseDiffDays < 30 && (!releaseViewedAt || releaseViewedAt < releaseDate)) {
    return latestRelease;
  }

  return null;
}

export function viewedNewRelease() {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem('releaseViewedAt', new Date().getTime());
  }
}

export default {fetchNewRelease, viewedNewRelease};
