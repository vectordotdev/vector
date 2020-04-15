"use strict";
const releaseUtils_1 = require("./releaseUtils");
const { parseQuery, getOptions } = require('loader-utils');
module.exports = function (fileString) {
    const callback = this.async();
    const { truncateMarker, siteDir, contentPath, releases } = getOptions(this);
    // Linkify posts
    let finalContent = releaseUtils_1.linkify(fileString, siteDir, contentPath, releases);
    // Truncate content if requested (e.g: file.md?truncated=true).
    const { truncated } = this.resourceQuery && parseQuery(this.resourceQuery);
    if (truncated) {
        finalContent = releaseUtils_1.truncate(finalContent, truncateMarker);
    }
    return callback && callback(null, finalContent);
};
