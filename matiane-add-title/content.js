'use strict';

(function() {
  const domain = window.location.hostname;
  const suffix = ` [${domain}]`;

  const formatTitle = () => {
    if (document.title && !document.title.endsWith(suffix)) {
      document.title += suffix;
    }
  };

  formatTitle();

  const attachObserver = () => {
    const target = document.querySelector('title') || document.head;

    if (target) {
      new MutationObserver(formatTitle).observe(target, {
        subtree: true,
        characterData: true,
        childList: true
      });

      return true;
    }

    return false;
  };

  if (!attachObserver()) {
    window.addEventListener('DOMContentLoaded', () => {
      formatTitle();
      attachObserver();
    });
  }
})();
