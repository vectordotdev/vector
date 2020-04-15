import React, {useCallback, useState} from 'react';

import GitHubButton from 'react-github-btn'
import Link from '@docusaurus/Link';
import Head from '@docusaurus/Head';
import SearchBar from '@theme/SearchBar';
import SVG from 'react-inlinesvg';
import Toggle from '@theme/Toggle';

import classnames from 'classnames';
import {fetchNewHighlight} from '@site/src/exports/newHighlight';
import {fetchNewPost} from '@site/src/exports/newPost';
import {fetchNewRelease} from '@site/src/exports/newRelease';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useHideableNavbar from '@theme/hooks/useHideableNavbar';
import useLockBodyScroll from '@theme/hooks/useLockBodyScroll';
import useLogo from '@theme/hooks/useLogo';
import useThemeContext from '@theme/hooks/useThemeContext';

import styles from './styles.module.css';

function navLinkAttributes(label, right) {
  let attrs = {'label': label};

  switch(label.toLowerCase()) {
    case 'blog':
      const newPost = fetchNewPost();

      if (newPost) {
        attrs.badge = 'new';
        attrs.badgeStyle = 'primary';
      }

      return attrs;

    case 'chat':
      attrs.hideText = right == true;
      attrs.icon = 'message-circle';
      return attrs;

    case 'community':
      attrs.hideText = right == true;
      attrs.icon = 'users';
      return attrs;

    case 'download':
      const newRelease = fetchNewRelease();

      attrs.hideText = right == true;
      attrs.icon = 'download';

      if (newRelease) {
        attrs.badge = 'new';
        attrs.badgeStyle = 'primary';
      }

      return attrs;

    case 'github':
      attrs.badge = '4k';
      attrs.hideText = false;
      attrs.icon = 'github';
      return attrs;

    case 'highlights':
      const newHighlight = fetchNewHighlight();

      if (newHighlight) {
        attrs.badge = 'new';
        attrs.badgeStyle = 'primary';
      }

      attrs.hideText = right == true;
      attrs.icon = 'gift';
      return attrs;

    default:
      return attrs;
  };
}

function NavLink({href, hideIcon, label, onClick, position, right, to}) {
  let attributes = navLinkAttributes(label, right) || {};
  const toUrl = useBaseUrl(to);

  return (
    <Link
      className={classnames(
        "navbar__item navbar__link",
        attributes.className,
        {
          'navbar__item__icon_only': attributes.hideText
        }
      )}
      title={attributes.hideText ? label : null}
      onClick={onClick}
      {...(href
        ? {
            target: '_blank',
            rel: 'noopener noreferrer',
            href: href,
          }
        : {
            activeClassName: 'navbar__link--active',
            to: toUrl,
          })}>
      {!hideIcon && attributes.icon && <><i className={`feather icon-${attributes.icon}`}></i> {attributes.iconLabel}</>}
      {!attributes.hideText && attributes.label}
      {attributes.badge && <span className={classnames('badge', `badge--${attributes.badgeStyle || 'secondary'}`)}>{attributes.badge}</span>}
    </Link>
  );
}

function Navbar() {
  const {
    siteConfig: {
      themeConfig: {
        navbar: {title, links = [], hideOnScroll = false} = {},
        disableDarkMode = false,
      },
    },
    isClient,
  } = useDocusaurusContext();
  const [sidebarShown, setSidebarShown] = useState(false);
  const [isSearchBarExpanded, setIsSearchBarExpanded] = useState(false);
  const {isDarkTheme, setLightTheme, setDarkTheme} = useThemeContext();
  const {navbarRef, isNavbarVisible} = useHideableNavbar(hideOnScroll);
  const {logoLink, logoLinkProps, logoImageUrl, logoAlt} = useLogo();

  useLockBodyScroll(sidebarShown);

  const showSidebar = useCallback(() => {
    setSidebarShown(true);
  }, [setSidebarShown]);
  const hideSidebar = useCallback(() => {
    setSidebarShown(false);
  }, [setSidebarShown]);

  const onToggleChange = useCallback(
    e => (e.target.checked ? setDarkTheme() : setLightTheme()),
    [setLightTheme, setDarkTheme],
  );

  return (
    <nav
      ref={navbarRef}
      className={classnames('navbar', 'navbar--light', 'navbar--fixed-top', {
        'navbar-sidebar--show': sidebarShown,
        [styles.navbarHideable]: hideOnScroll,
        [styles.navbarHidden]: !isNavbarVisible,
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
          <Link className="navbar__brand" to={logoLink} {...logoLinkProps}>
            {logoImageUrl != null && (
              <SVG className="navbar__logo" src={logoImageUrl} alt={logoAlt} />
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
              <NavLink {...linkItem} left={true} key={i} />
            ))}
        </div>
        <div className="navbar__items navbar__items--right">
          {links
            .filter(linkItem => linkItem.position === 'right')
            .map((linkItem, i) => (
              <NavLink {...linkItem} right={true} key={i} />
            ))}
          {!disableDarkMode && (
            <Toggle
              className={styles.displayOnlyInLargeViewport}
              aria-label="Dark mode toggle"
              checked={isDarkTheme}
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
        onClick={hideSidebar}
      />
      <div className="navbar-sidebar">
        <div className="navbar-sidebar__brand">
          <Link
            className="navbar__brand"
            onClick={hideSidebar}
            to={logoLink}
            {...logoLinkProps}>
            {logoImageUrl != null && (
              <SVG className="navbar__logo" src={logoImageUrl} alt={logoAlt} />
            )}
            {title != null && <strong>{title}</strong>}
          </Link>
          {!disableDarkMode && sidebarShown && (
            <Toggle
              aria-label="Dark mode toggle in sidebar"
              checked={isDarkTheme}
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
                    hideIcon={true}
                    onClick={hideSidebar}
                  />
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    </nav>
  );
}

export default Navbar;
