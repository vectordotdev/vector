import {loader} from 'webpack';
import {truncate, linkify} from './highlightUtils';
const {parseQuery, getOptions} = require('loader-utils');

export = function(fileString: string) {
  const callback = this.async();
  const {truncateMarker, siteDir, contentPath, highlights} = getOptions(this);
  // Linkify posts
  let finalContent = linkify(fileString, siteDir, contentPath, highlights);

  // Truncate content if requested (e.g: file.md?truncated=true).
  const {truncated} = this.resourceQuery && parseQuery(this.resourceQuery);
  if (truncated) {
    finalContent = truncate(finalContent, truncateMarker);
  }

  return callback && callback(null, finalContent);
} as loader.Loader;
