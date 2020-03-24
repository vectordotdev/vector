import {loader} from 'webpack';
import {truncate, linkify} from './guideUtils';
const {parseQuery, getOptions} = require('loader-utils');

export = function(fileString: string) {
  const callback = this.async();
  const {truncateMarker, siteDir, contentPath, guides} = getOptions(this);
  // Linkify posts
  let finalContent = linkify(fileString, siteDir, contentPath, guides);

  // Truncate content if requested (e.g: file.md?truncated=true).
  const {truncated} = this.resourceQuery && parseQuery(this.resourceQuery);
  if (truncated) {
    finalContent = truncate(finalContent, truncateMarker);
  }

  return callback && callback(null, finalContent);
} as loader.Loader;
