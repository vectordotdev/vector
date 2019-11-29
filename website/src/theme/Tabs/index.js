/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, {useState, useEffect, Children} from 'react';

import classnames from 'classnames';
import queryString from 'query-string';

function Tabs(props) {
  const {block, centered, children, defaultValue, style, values, urlKey} = props;
  const [selectedValue, setSelectedValue] = useState(defaultValue);

  useEffect(() => {
    if (typeof window !== 'undefined' && window.location && urlKey) {
      let queryObj = queryString.parse(window.location.search);

      if (queryObj[urlKey])
        setSelectedValue(queryObj[urlKey]);
    }
  }, []);

  return (
    <div>
      <div className={centered ? "tabs--centered" : ""}>
        <ul
          className={classnames('tabs', props.className, {
            'tabs--block': block,
          })}
          style={style}
          >
          {values.map(({value, label}) => (
            <li
              className={classnames('tab-item', {
                'tab-item--active': selectedValue === value,
              })}
              key={value}
              onClick={() => setSelectedValue(value)}>
              {label}
            </li>
          ))}
        </ul>
      </div>
      <div className="margin-vert--md">
        {
          Children.toArray(children).filter(
            child => child.props.value === selectedValue,
          )[0]
        }
      </div>
    </div>
  );
}

export default Tabs;
