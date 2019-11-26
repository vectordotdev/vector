/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
import * as React from 'react';

function determineTheme() {
  let theme = null;

  // Make sure we're in the browser / client context
  if (typeof document !== 'undefined' && typeof window !== 'undefined') {
    theme = document.querySelector('html').getAttribute('data-theme');

    // Reset the theme if it is a null like value
    if (theme == '' || theme == 'null') {
      theme = null;
    }

    if (theme === null)
      theme = window.localStorage.getItem('theme');

    if (theme === null && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      theme = 'dark';
    }

    if (theme === null && window.matchMedia("(prefers-color-scheme: light)").matches) {
      theme = '';
    }

    if (theme === null) {
      let utcDate = new Date();
      let offset = (new Date().getTimezoneOffset() / 60) * -1;
      let date = new Date(utcDate.getTime() + offset);
      theme = (date.getHours() >= 18 || date.getHours() < 7 ? 'dark' : null);
    }
  }

  return theme;
}

const useTheme = () => {
  let defaultTheme = determineTheme();
  const [theme, setTheme] = React.useState(defaultTheme);

  React.useEffect(() => {
    try {
      setTheme(determineTheme());
    } catch (err) {
      console.error(err);
    }
  }, [setTheme]);

  const setThemeSyncWithLocalStorage = React.useCallback(
    nextTheme => {
      try {
        localStorage.setItem('theme', nextTheme);
        setTheme(nextTheme);
      } catch (err) {
        console.error(err);
      }
    },
    [setTheme, theme],
  );

  return [theme, setThemeSyncWithLocalStorage];
};

export default useTheme;
