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
  // The bar starts focused, so focus alone does not mean the user is
  // typing; only typing stops the page's address from replacing the text.
  address.onfocus = function () { address.select(); };
  address.oninput = function () { editing = true; };
  address.onblur = function () { editing = false; };

  // Keep the address bar in step with the page while the user isn't typing.
  setInterval(function () {
    if (editing) return;
    invoke('snapshot').then(function (s) {
      if (!editing && s && typeof s.address === 'string' && s.address !== 'about:blank') {
        if (address.value !== s.address) {
          address.value = s.address;
          // Still focused and untouched: keep it selected, so typing
          // replaces the address instead of adding to its end.
          if (document.activeElement === address) address.select();
        }
      }
    }).catch(function () { });
  }, 500);
})();
