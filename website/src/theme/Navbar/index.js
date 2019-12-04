/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, {useCallback, useState} from 'react';

import Link from '@docusaurus/Link';
import Head from '@docusaurus/Head';
import SearchBar from '@theme/SearchBar';
import SVG from 'react-inlinesvg';
import Toggle from '@theme/Toggle';

import classnames from 'classnames';
import {fetchNewPost} from '@site/src/exports/newPost';
import {fetchNewRelease} from '@site/src/exports/newRelease';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useTheme from '@theme/hooks/useTheme';

import styles from './styles.module.css';

function navLinkAttributes(label) {
  switch(label.toLowerCase()) {
    case 'blog':
      const newPost = fetchNewPost();

      if (newPost) {
        return {
          badge: 'new',
          badgeStyle: 'primary',
        };
      } else {
        return {};
      }

    case 'download':
      const newRelease = fetchNewRelease();

      let downloadAttrs = {
        icon: 'download',
        hideText: true
      }

      if (newRelease) {
        downloadAttrs.badge = 'new';
        downloadAttrs.badgeStyle = 'primary';
      }

      return downloadAttrs;

    case 'github':
      return {
        badge: '3k',
        icon: 'github',
        hideText: true 
      };

    default:
      return {};
  };
}

function NavLink(props) {
  let attributes = navLinkAttributes(props.label) || {};
  const toUrl = useBaseUrl(props.to);
  const menu = props.menu == true;

  return (
    <Link
      className="navbar__item navbar__link"
      title={!menu && attributes.hideText && props.label}
      {...props}
      {...(props.href
        ? {
            target: '_blank',
            rel: 'noopener noreferrer',
            href: props.href,
          }
        : {
            activeClassName: 'navbar__link--active',
            to: toUrl,
          })}>
      {!menu && attributes.icon && <><i className={`feather icon-${attributes.icon}`}></i> </>}
      {(menu || !attributes.hideText) && props.label}
      {attributes.badge && <span className={classnames('badge', `badge--${attributes.badgeStyle || 'secondary'}`)}>{attributes.badge}</span>}
    </Link>
  );
}

function Navbar() {
  const context = useDocusaurusContext();
  const [sidebarShown, setSidebarShown] = useState(false);
  const [isSearchBarExpanded, setIsSearchBarExpanded] = useState(false);
  const [theme, setTheme] = useTheme();
  const {siteConfig = {}} = context;
  const {baseUrl, themeConfig = {}} = siteConfig;
  const {navbar = {}, disableDarkMode = false} = themeConfig;
  const {title, logo = {}, links = []} = navbar;

  const showSidebar = useCallback(() => {
    setSidebarShown(true);
  }, [setSidebarShown]);
  const hideSidebar = useCallback(() => {
    setSidebarShown(false);
  }, [setSidebarShown]);

  const onToggleChange = useCallback(
    e => setTheme(e.target.checked ? 'dark' : ''),
    [setTheme],
  );

  const logoUrl = useBaseUrl(logo.src);
  return (
    <>
      <Head>
        {/* TODO: Do not assume that it is in english language */}
        <html lang="en" data-theme={theme} />
      </Head>
      <nav
        className={classnames('navbar', 'navbar--light', 'navbar--fixed-top', {
          'navbar-sidebar--show': sidebarShown,
        })}>
        <div className="navbar__inner">
          <div className="navbar__items">
            <div
              aria-label="Navigation bar toggle"
              className="navbar__toggle"
              role="button"
              tabIndex={0}
              onClick={showSidebar}
              onKeyDown={showSidebar}>
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="30"
                height="30"
                viewBox="0 0 30 30"
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
            </div>
            <Link className="navbar__brand" to={baseUrl}>
              {logo != null && (
                <SVG className="navbar__logo" src={logoUrl} alt={logo.alt} />
              )}
              {title != null && (
                <strong
                  className={isSearchBarExpanded ? styles.hideLogoText : ''}>
                  {title}
                </strong>
              )}
            </Link>
            {links
              .filter(linkItem => linkItem.position !== 'right')
              .map((linkItem, i) => (
                <NavLink {...linkItem} menu={false} key={i} />
              ))}
          </div>
          <div className="navbar__items navbar__items--right">
            {links
              .filter(linkItem => linkItem.position === 'right')
              .map((linkItem, i) => (
                <NavLink {...linkItem} menu={false} key={i} />
              ))}
            {!disableDarkMode && (
              <Toggle
                className={styles.displayOnlyInLargeViewport}
                aria-label="Dark mode toggle"
                checked={theme === 'dark'}
                onChange={onToggleChange}
              />
            )}
            <SearchBar
              handleSearchBarToggle={setIsSearchBarExpanded}
              isSearchBarExpanded={isSearchBarExpanded}
            />
          </div>
        </div>
        <div
          role="presentation"
          className="navbar-sidebar__backdrop"
          onClick={() => {
            setSidebarShown(false);
          }}
        />
        <div className="navbar-sidebar">
          <div className="navbar-sidebar__brand">
            <Link className="navbar__brand" onClick={hideSidebar} to={baseUrl}>
              {logo != null && (
                <SVG className="navbar__logo" src={logoUrl} alt={logo.alt} />
              )}
              {title != null && <strong>{title}</strong>}
            </Link>
            {!disableDarkMode && sidebarShown && (
              <Toggle
                aria-label="Dark mode toggle in sidebar"
                checked={theme === 'dark'}
                onChange={onToggleChange}
              />
            )}
          </div>
          <div className="navbar-sidebar__items">
            <div className="menu">
              <ul className="menu__list">
                {links.map((linkItem, i) => (
                  <li className="menu__list-item" key={i}>
                    <NavLink
                      className="menu__link"
                      {...linkItem}
                      menu={true}
                      onClick={hideSidebar}
                    />
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </div>
      </nav>
    </>
  );
}

export default Navbar;
