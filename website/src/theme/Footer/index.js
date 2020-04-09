
import React from 'react';

import Link from '@docusaurus/Link';
import MailingListForm from '@site/src/components/MailingListForm';
import SVG from 'react-inlinesvg';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';


function FooterLink({to, href, label, ...props}) {
  const toUrl = useBaseUrl(to);
  return (
    <Link
      className="footer__link-item"
      {...(href
        ? {
            target: '_blank',
            rel: 'noopener noreferrer',
            href,
          }
        : {
            to: toUrl,
          })}
      {...props}>
      {label}
    </Link>
  );
}

const FooterLogo = ({url, alt}) => (
  <SVG className="footer__logo" alt={alt} src={url} />
);

function Footer() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {themeConfig = {}} = siteConfig;
  const {footer} = themeConfig;

  const {copyright, links = [], logo = {}} = footer || {};
  const logoUrl = useBaseUrl(logo.src);

  if (!footer) {
    return null;
  }

  return (
    <footer
      className={classnames('footer', {
        'footer--dark': footer.style === 'dark',
      })}>
      <div className="container">
        {links && links.length > 0 && (
          <div className="row footer__links">
            <div className="col col--5 footer__col">
              <div className="margin-bottom--md">
                <SVG className="navbar__logo" src="/img/logo-light.svg" alt="Vector" width="150" height="auto" />
              </div>
              <div className="margin-bottom--md">
                <MailingListForm description={false} width="150px" />
              </div>
              <div>
                <a href="https://twitter.com/vectordotdev" target="_blank"><i className="feather icon-twitter" alt="Vector's Twitter"></i></a>
                &nbsp;&nbsp;&nbsp;&nbsp;
                <a href="https://chat.vector.dev" target="_blank"><i className="feather icon-message-circle" alt="Vector's Chat"></i></a>
                &nbsp;&nbsp;&nbsp;&nbsp;
                <a href="https://github.com/timberio/vector" target="_blank"><i className="feather icon-github" alt="Vector's Github Repo"></i></a>
                &nbsp;&nbsp;&nbsp;&nbsp;
                <a href="https://vector.dev/blog/atom.xml" target="_blank"><i className="feather icon-rss" alt="Vector's RSS feed"></i></a>
              </div>
            </div>
            {links.map((linkItem, i) => (
              <div key={i} className="col footer__col">
                {linkItem.title != null ? (
                  <h4 className="footer__title">{linkItem.title}</h4>
                ) : null}
                {linkItem.items != null &&
                Array.isArray(linkItem.items) &&
                linkItem.items.length > 0 ? (
                  <ul className="footer__items">
                    {linkItem.items.map((item, key) =>
                      item.html ? (
                        <li
                          key={key}
                          className="footer__item"
                          dangerouslySetInnerHTML={{
                            __html: item.html,
                          }}
                        />
                      ) : (
                        <li key={item.href || item.to} className="footer__item">
                          <FooterLink {...item} />
                        </li>
                      ),
                    )}
                  </ul>
                ) : null}
              </div>
            ))}
          </div>
        )}
        {(logo || copyright) && (
          <div className="text--center">
            {logo && logo.src && (
              <div className="margin-bottom--sm">
                {logo.href ? (
                  <a
                    href={logo.href}
                    target="_blank"
                    rel="noopener noreferrer"
                    className={styles.footerLogoLink}>
                    <FooterLogo alt={logo.alt} url={logoUrl} />
                  </a>
                ) : (
                  <FooterLogo alt={logo.alt} url={logoUrl} />
                )}
              </div>
            )}
            {copyright}<br />
            <small>
              <a href="https://github.com/timberio/vector/security/policy">Security Policy</a>&nbsp;&bull;&nbsp;
              <a href="https://github.com/timberio/vector/blob/master/PRIVACY.md">Privacy Policy</a>
            </small>
          </div>
        )}
      </div>
    </footer>
  );
}

export default Footer;
