/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, {useState} from 'react';

import GuideItem from '@theme/GuideItem';
import Layout from '@theme/Layout';
import qs from 'qs';
import classnames from 'classnames';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();

  const queryObj = props.location ? qs.parse(props.location.search, {ignoreQueryPrefix: true}) : {};

  let items_filtered = items.slice(0);

  let seen = {};
  items_filtered = items_filtered.filter(item => {
    let title = item.content.metadata.title;
    let dupe = seen[title] == true
    seen[title] = true;
    if ( dupe ) {
      console.log(`WARNING: Found duplicate guide: ${title}`);
    }
    return !dupe;
  });

  //
  // State
  //

  const [onlyFeatured, setOnlyFeatured] = useState(queryObj['featured'] == 'true');
  const [searchTerm, setSearchTerm] = useState(null);
  const [searchLimit, setSearchLimit] = useState(20);

  //
  // Filtering
  //

  if (searchTerm) {
    items_filtered = items_filtered.filter(item => {
      let searchTerms = searchTerm.split(" ");
      let content = `${item.content.metadata.title.toLowerCase()}`; // ${item.content.metadata.description.toLowerCase()}`;
      return searchTerms.every(term => {
        let index = content.indexOf(term.toLowerCase());
        if ( index == -1 ) {
          return false
        }
        content = content.slice(index);
        return true
      })
    });
  }

  if (onlyFeatured) {
    items_filtered = items_filtered.filter(item => item.content.metadata.featured == true);
  }

  items_filtered = items_filtered.slice(0, searchLimit);

  return (
    <Layout title="Guides" description="Vector Guides">
      <header className={classnames('hero', 'domain-bg', {'header':true})}>
        <div className={classnames('container', {'headerContainer': true})}>
          <h1>Vector Guides</h1>
          <p>A collection of guides to walk you through Vector use cases.</p>
        </div>
      </header>
      <div className="guide-list container">
        <div className={classnames('vector-components', {'vector-components--cols': true})}>
          <div className="filters">
            <div className="search">
              <input
                type="text"
                onChange={(event) => setSearchTerm(event.currentTarget.value)}
                placeholder="🔍 Search..." />
            </div>
            <div className="filter">
              <div className="filter--label">
                Filters
              </div>
              <div className="filter--choices">
                <label title="Show only featured guides.">
                  <input
                    type="checkbox"
                    onChange={(event) => setOnlyFeatured(event.currentTarget.checked)}
                    checked={onlyFeatured} /> Featured
                </label>
              </div>
            </div>
          </div>
        </div>
        <div className="guide-list--items">
          {items_filtered.map(({content: GuideContent}) => (
            <GuideItem
              key={GuideContent.metadata.permalink}
              frontMatter={GuideContent.frontMatter}
              metadata={GuideContent.metadata}
              truncated>
              <GuideContent />
            </GuideItem>
          ))}
          <button className="button button--secondary guide-show-more" onClick={() => setSearchLimit(searchLimit+10)}>Show more</button>
        </div>
      </div>
    </Layout>
  );
}

export default GuideListPage;
