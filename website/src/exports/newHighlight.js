import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

export function fetchNewHighlight() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {latest_highlight: latestHighlight}} = siteConfig.customFields;
  const date = Date.parse(latestHighlight.date);
  const now = new Date();
  const diffTime = Math.abs(now - date);
  const diffDays = Math.ceil(diffTime / (1000 * 60 * 60 * 24));

  let viewedAt = null;

  if (typeof window !== 'undefined') {
    viewedAt = new Date(parseInt(window.localStorage.getItem('highlightsViewedAt') || '0'));
  }

  if (diffDays < 30 && (!viewedAt || viewedAt < date)) {
    return latestHighlight;
  }

  return null;
}

export function viewedNewHighlight() {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem('highlightsViewedAt', new Date().getTime());
  }
}

export default {fetchNewHighlight, viewedNewHighlight};
