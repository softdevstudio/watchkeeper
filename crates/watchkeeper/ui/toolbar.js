(function () {
  'use strict';
  var invoke = window.__TAURI_INTERNALS__.invoke;
  var address = document.getElementById('address');
  var error = document.getElementById('error');
  var editing = false;

  function call(cmd, args) {
    error.textContent = '';
    return invoke(cmd, args || {}).catch(function (e) { error.textContent = String(e); });
  }

  document.getElementById('back').onclick = function () { call('go_back'); };
  document.getElementById('forward').onclick = function () { call('go_forward'); };
  document.getElementById('reload').onclick = function () { call('reload'); };
  document.getElementById('go').onsubmit = function (e) {
    e.preventDefault();
    editing = false;
    call('navigate', { url: address.value });
    address.blur();
  };
  address.onfocus = function () { editing = true; address.select(); };
  address.onblur = function () { editing = false; };

  // Keep the address bar in step with the page while the user isn't typing.
  setInterval(function () {
    if (editing) return;
    invoke('snapshot').then(function (s) {
      if (!editing && s && typeof s.page_url === 'string' && s.page_url !== 'about:blank') {
        if (address.value !== s.page_url) address.value = s.page_url;
      }
    }).catch(function () { });
  }, 500);
})();
