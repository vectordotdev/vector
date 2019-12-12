function enrichTag(tag) {
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
  }

  return {
    category: category,
    label: tag.label,
    permalink: tag.permalink,
    style: style,
    value: value,
  };
}

export function enrichTags(tags) {
  return tags.map(tag => enrichTag(tag));
}

export default {enrichTags};