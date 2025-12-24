'use strict';

(function() {
  const domain = window.location.hostname;
  const suffix = ` [${domain}]`;

  const formatTitle = () => {
    if (document.title && !document.title.endsWith(suffix)) {
      document.title += suffix;
    }
  };

  let titleObserver = null;
  const watchTitleElement = (titleNode) => {
    if (titleObserver) titleObserver.disconnect();

    if (titleNode) {
      titleObserver = new MutationObserver(formatTitle);
      titleObserver.observe(titleNode, {
        childList: true,
        characterData: true,
        subtree: true
      });

      formatTitle();
    }
  };

  const attachHeadObserver = () => {
    const head = document.querySelector('head');

    if (!head) {
      return false;
    }

    watchTitleElement(document.querySelector('title'));
    formatTitle();

    new MutationObserver((mutations) => {
      for (const m of mutations) {
        for (const node of m.addedNodes) {
          if (node.tagName === 'TITLE') {
            watchTitleElement(node);
            return; 
          }
        }
      }
    }).observe(head, {
      childList: true, 
      subtree: false
    });

    return true;
  };

  if (!attachHeadObserver()) {
    window.addEventListener('DOMContentLoaded', attachHeadObserver);
  }
})();
