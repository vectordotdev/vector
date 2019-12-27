import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

export function fetchNewPost() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {latest_post: latestPost}} = siteConfig.customFields;
  const date = Date.parse(latestPost.date);
  const now = new Date();
  const diffTime = Math.abs(now - date);
  const diffDays = Math.ceil(diffTime / (1000 * 60 * 60 * 24));

  let viewedAt = null;

  if (typeof window !== 'undefined') {
    viewedAt = new Date(parseInt(window.localStorage.getItem('blogViewedAt') || '0'));
  }

  if (diffDays < 30 && (!viewedAt || viewedAt < date)) {
    return latestPost;
  }

  return null;
}

export function viewedNewPost() {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem('blogViewedAt', new Date().getTime());
  }
}

export default {fetchNewPost, viewedNewPost};
