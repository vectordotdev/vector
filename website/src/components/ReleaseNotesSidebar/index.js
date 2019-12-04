import React, {useState, useCallback} from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const MOBILE_TOGGLE_SIZE = 24;

function ReleaseNotesSidebar() {
  const [showResponsiveSidebar, setShowResponsiveSidebar] = useState(false);

  return (
    <div className="sidebar">
      <div
        className={classnames('menu', 'menu--responsive', {
          'menu--show': showResponsiveSidebar,
        })}>
        <button
          aria-label={showResponsiveSidebar ? 'Close Menu' : 'Open Menu'}
          className="button button--secondary button--sm menu__button"
          type="button"
          onClick={() => {
            setShowResponsiveSidebar(!showResponsiveSidebar);
          }}>
          {showResponsiveSidebar ? (
            <span
              className={classnames(
                "sidebar--menu-icon",
                "sidebar--menu-icon-close",
              )}>
              &times;
            </span>
          ) : (
            <svg
              className="sidebar--menu-icon"
              xmlns="http://www.w3.org/2000/svg"
              height={MOBILE_TOGGLE_SIZE}
              width={MOBILE_TOGGLE_SIZE}
              viewBox="0 0 32 32"
              role="img"
              focusable="false">
              <title>Menu</title>
              <path
                stroke="currentColor"
                strokeLinecap="round"
                strokeMiterlimit="10"
                strokeWidth="2"
                d="M4 7h22M4 15h22M4 23h22"
              />
            </svg>
          )}
        </button>
        <ul className="menu__list">
          <li className="menu__list-item">
            <Link
              activeClassName="menu__link--active"
              className="menu__link"
              exact
              to="/docs">
              Test
            </Link>
          </li>
        </ul>
      </div>
    </div>
  );
}

export default ReleaseNotesSidebar;