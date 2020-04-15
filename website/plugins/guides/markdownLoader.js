"use strict";
const guideUtils_1 = require("./guideUtils");
const { parseQuery, getOptions } = require('loader-utils');
module.exports = function (fileString) {
    const callback = this.async();
    const { truncateMarker, siteDir, contentPath, guides } = getOptions(this);
    // Linkify posts
    let finalContent = guideUtils_1.linkify(fileString, siteDir, contentPath, guides);
    // Truncate content if requested (e.g: file.md?truncated=true).
    const { truncated } = this.resourceQuery && parseQuery(this.resourceQuery);
    if (truncated) {
        finalContent = guideUtils_1.truncate(finalContent, truncateMarker);
    }
    return callback && callback(null, finalContent);
};
