import React from 'react';

import queryString from 'query-string';

import './styles.css';

function Steps({children, headingDepth}) {
  let location = typeof(window) !== 'undefined' ? window.location : null;
  let issueQueryString = {
    title: `Tutorial on ${location} failed`,
    body: `The tutorial on:\n\n${location}\n\nHere's what went wrong:\n\n<!-- Insert command output and details. Thank you for reporting! :) -->`
  };

  return (
    <div className={`steps steps--h${headingDepth}`}>
      {children}
      <div className="steps--feedback">
        How was it? Did this tutorial work?&nbsp;&nbsp;
        <span className="button button--sm button--primary">Yes</span>&nbsp;&nbsp;
        <a href={`https://github.com/timberio/vector/issues/new?${queryString.stringify(issueQueryString)}`} target="_blank" className="button button--sm button--primary">No</a>
      </div>
    </div>

  );
}

export default Steps;
