import GithubSlugger from 'github-slugger';

function enrichTag(tag, section) {
  if (tag.enriched) {
    return tag
  }

  const labelParts = tag.label.split(': ', 2);
  const category = labelParts[0];
  const value = labelParts[1];
  let style = 'primary';

  switch(category) {
    case 'domain':
      style = 'blue';
      break;

    case 'type':
      style = 'pink'
      break;

    default:
      style = 'primary';
      break;
  }

  return {
    category: category,
    count: tag.count,
    enriched: true,
    label: tag.label,
    permalink: tag.permalink,
    style: style,
    value: value,
  };
}

export function enrichTags(tags, section) {
  const slugger = new GithubSlugger();

  return tags.map(tag => {
    let normalizedTag = tag;

    if (typeof(tag) == 'string') {
      normalizedTag = {label: tag, permalink: `/${section}/tags/${slugger.slug(tag)}`};
    }

    return enrichTag(normalizedTag, section)
  });
}

export function extractTagValue(tags, category) {
  let prefix = category + ': ';

  let tag = tags.find(tag => tag.startsWith(prefix));

  if (tag) {
    return tag.replace(prefix, '');
  } else {
    return null;
  }
}

export default {enrichTags, extractTagValue};
