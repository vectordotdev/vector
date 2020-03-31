import React, {useState} from 'react';

import classnames from 'classnames';
import queryString from 'query-string';

import './styles.css';

function Steps({children, headingDepth}) {
  let location = typeof(window) !== 'undefined' ? window.location : null;
  let issueQueryString = {
    title: `Tutorial on ${location} failed`,
    body: `The tutorial on:\n\n${location}\n\nHere's what went wrong:\n\n<!-- Insert command output and details. Thank you for reporting! :) -->`
  };
  let issueURL = `https://github.com/timberio/vector/issues/new?${queryString.stringify(issueQueryString)}`;

  const [feedbackAnswer, setFeedbackAnswer] = useState(null);

  return (
    <div className={`steps steps--h${headingDepth}`}>
      {children}
      {!feedbackAnswer && <div className="steps--feedback">
        How was it? Did this tutorial work?&nbsp;&nbsp;
        <span
          className="button button--sm button--primary"
          onClick={() => setFeedbackAnswer('yes')}>
          Yes
        </span>&nbsp;&nbsp;
        <a
          href={issueURL}
          target="_blank"
          className="button button--sm button--primary">
          No
        </a>
      </div>}
      {feedbackAnswer == 'yes' && <div className="steps--feedback steps--feedback--success">
        Thanks! If you're enjoying Vector please consider <a href="https://github.com/timberio/vector/" target="_blank">starring our Github repo</a>.
      </div>}
    </div>

  );
}

export default Steps;
