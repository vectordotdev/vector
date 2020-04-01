import React, {useState, useEffect, Children} from 'react';

import Select from 'react-select';

import classnames from 'classnames';
import queryString from 'query-string';
import useTabGroupChoiceContext from '@theme/hooks/useTabGroupChoiceContext';

const keys = {
  left: 37,
  right: 39,
};

function ListSwitcher({block, centered, changeSelectedValue, className, handleKeydown, style, values, selectedValue, tabRefs}) {
  return (
    <div className={centered ? "tabs--centered" : null}>
      <ul
        role="tablist"
        aria-orientation="horizontal"
        className={classnames('tabs', className, {
          'tabs--block': block,
        })}
        style={style}
        >
        {values.map(({value, label}) => (
          <li
            role="tab"
            tabIndex="0"
            aria-selected={selectedValue === value}
            className={classnames('tab-item', {
              'tab-item--active': selectedValue === value,
            })}
            key={value}
            ref={tabControl => tabRefs.push(tabControl)}
            onKeyDown={event => handleKeydown(tabRefs, event.target, event)}
            onFocus={() => changeSelectedValue(value)}
            onClick={() => changeSelectedValue(value)}>
            {label}
          </li>
        ))}
      </ul>
    </div>
  );
}
function SelectSwitcher({placeholder, selectedValue, changeSelectedValue, size, values}) {
  let options = values;

  if (options[0].group) {
    let groupedOptions = _.groupBy(options, 'group');

    options = Object.keys(groupedOptions).map(group => {
      return {
        label: group,
        options: groupedOptions[group]
      }
    });
  }

  return (
    <Select
      className={`react-select-container react-select--${size}`}
      classNamePrefix='react-select'
      options={options}
      isClearable={selectedValue}
      placeholder={placeholder}
      value={values.find(option => option.value == selectedValue)}
      onChange={(selectedOption) => changeSelectedValue(selectedOption ? selectedOption.value : null)} />
  );
}

function Tabs(props) {
  const {block, centered, children, defaultValue, groupId, label, placeholder, select, size, style, values, urlKey} = props;
  const {tabGroupChoices, setTabGroupChoices} = useTabGroupChoiceContext();
  const [selectedValue, setSelectedValue] = useState(defaultValue);

  if (groupId != null) {
    const relevantTabGroupChoice = tabGroupChoices[groupId];
    if (
      relevantTabGroupChoice != null &&
      relevantTabGroupChoice !== selectedValue
    ) {
      setSelectedValue(relevantTabGroupChoice);
    }
  }

  const changeSelectedValue = newValue => {
    setSelectedValue(newValue);
    if (groupId != null) {
      setTabGroupChoices(groupId, newValue);
    }
  };

  const tabRefs = [];

  const focusNextTab = (tabs, target) => {
    const next = tabs.indexOf(target) + 1;

    if (!tabs[next]) {
      tabs[0].focus();
    } else {
      tabs[next].focus();
    }
  };

  const focusPreviousTab = (tabs, target) => {
    const prev = tabs.indexOf(target) - 1;

    if (!tabs[prev]) {
      tabs[tabs.length - 1].focus();
    } else {
      tabs[prev].focus();
    }
  };

  const handleKeydown = (tabs, target, event) => {
    switch (event.keyCode) {
      case keys.right:
        focusNextTab(tabs, target);
        break;
      case keys.left:
        focusPreviousTab(tabs, target);
        break;
      default:
        break;
    }
  };

  useEffect(() => {
    if (typeof window !== 'undefined' && window.location && urlKey) {
      let queryObj = queryString.parse(window.location.search);

      if (queryObj[urlKey])
        setSelectedValue(queryObj[urlKey]);
    }
  }, []);

  return (
    <>
      <div className={`margin-bottom--${size || 'md'}`}>
        {label && <div className="margin-vert--sm">{label}</div>}
        {values.length > 1 && (select ?
          <SelectSwitcher
            changeSelectedValue={changeSelectedValue}
            handleKeydown={handleKeydown}
            placeholder={placeholder}
            selectedValue={selectedValue}
            size={size}
            tabRefs={tabRefs}
            {...props} /> :
          <ListSwitcher
            changeSelectedValue={changeSelectedValue}
            handleKeydown={handleKeydown}
            selectedValue={selectedValue}
            tabRefs={tabRefs}
            {...props} />)}
      </div>

      {
        Children.toArray(children).filter(
          child => child.props.value === selectedValue,
        )[0]
      }
    </>
  );
}

export default Tabs;
