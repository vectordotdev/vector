import React, {useEffect, useState, useRef} from 'react';
import classnames from 'classnames';
import Highlight, {defaultProps} from 'prism-react-renderer';
import defaultTheme from 'prism-react-renderer/themes/palenight';
import Clipboard from 'clipboard';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import styles from './styles.module.css';
import Prism from 'prism-react-renderer/prism';

(typeof global !== 'undefined' ? global : window).Prism = Prism;
require('prismjs/components/prism-protobuf');
require('prismjs/components/prism-rust');
require('prismjs/components/prism-toml');

export default ({children, className: languageClassName}) => {
  const {
    siteConfig: {
      themeConfig: {prismTheme},
    },
  } = useDocusaurusContext();
  const [showCopied, setShowCopied] = useState(false);
  const target = useRef(null);
  const button = useRef(null);

  useEffect(() => {
    let clipboard;

    if (button.current) {
      clipboard = new Clipboard(button.current, {
        target: () => target.current,
      });
    }

    return () => {
      if (clipboard) {
        clipboard.destroy();
      }
    };
  }, [button.current, target.current]);

  const language =
    languageClassName && languageClassName.replace(/language-/, '');

  const handleCopyCode = () => {
    window.getSelection().empty();
    setShowCopied(true);

    setTimeout(() => setShowCopied(false), 2000);
  };
  
  return (
    <Highlight
      {...defaultProps}
      theme={prismTheme || defaultTheme}
      code={children.trim()}
      language={language}>
      {({className, style, tokens, getLineProps, getTokenProps}) => (
        <div className={styles.codeBlockWrapper}>
          <pre
            ref={target}
            className={classnames(className, styles.codeBlock)}
            style={style}>
            {tokens.map((line, i) => (
              <div key={i} {...getLineProps({line, key: i})}>
                {line.map((token, key) => (
                  <span key={key} {...getTokenProps({token, key})} />
                ))}
              </div>
            ))}
          </pre>
          <button
            ref={button}
            type="button"
            aria-label="Copy code to clipboard"
            className={styles.copyButton}
            onClick={handleCopyCode}>
            {showCopied ? 'Copied' : 'Copy'}
          </button>
        </div>
      )}
    </Highlight>
  );
};
