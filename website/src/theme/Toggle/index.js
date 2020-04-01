import React from 'react';
import Toggle from 'react-toggle';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import classnames from 'classnames';
import styles from './styles.module.css';

const Moon = () => <span className={classnames(styles.toggle, styles.moon)} />;
const Sun = () => <span className={classnames(styles.toggle, styles.sun)} />;

export default function(props) {
  const {isClient} = useDocusaurusContext();
  return (
    <Toggle
      disabled={!isClient}
      icons={{
        checked: <Moon />,
        unchecked: <Sun />,
      }}
      {...props}
    />
  );
}
