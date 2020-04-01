import React from 'react';
import Link from '@docusaurus/Link';

import classnames from 'classnames';

function PagePaginator({className, previous, next}) {
  return (
    <nav className={classnames('pagination-nav', className)}>
      <div className="pagination-nav__item">
        {previous && (
          <Link
            className="pagination-nav__link"
            to={previous.permalink}>
            <h5 className="pagination-nav__link--sublabel">Previous</h5>
            <h4 className="pagination-nav__link--label">
              &laquo; {previous.title}
            </h4>
          </Link>
        )}
      </div>
      <div className="pagination-nav__item pagination-nav__item--next">
        {next && (
          <Link className="pagination-nav__link" to={next.permalink}>
            <h5 className="pagination-nav__link--sublabel">Next</h5>
            <h4 className="pagination-nav__link--label">
              {next.title} &raquo;
            </h4>
          </Link>
        )}
      </div>
    </nav>
  );
}

export default PagePaginator;
