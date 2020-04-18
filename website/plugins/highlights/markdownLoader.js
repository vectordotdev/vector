"use strict";
const highlightUtils_1 = require("./highlightUtils");
const { parseQuery, getOptions } = require('loader-utils');
module.exports = function (fileString) {
    const callback = this.async();
    const { truncateMarker, siteDir, contentPath, highlights } = getOptions(this);
    // Linkify posts
    let finalContent = highlightUtils_1.linkify(fileString, siteDir, contentPath, highlights);
    // Truncate content if requested (e.g: file.md?truncated=true).
    const { truncated } = this.resourceQuery && parseQuery(this.resourceQuery);
    if (truncated) {
        finalContent = highlightUtils_1.truncate(finalContent, truncateMarker);
    }
    return callback && callback(null, finalContent);
};
