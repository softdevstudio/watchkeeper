(function () {
  'use strict';
  var invoke = window.__TAURI_INTERNALS__.invoke;
  function set(id, v) { document.getElementById(id).textContent = String(v); }

  var last = '';
  function render(s) {
    var key = JSON.stringify(s);
    if (key === last) return;
    last = key;
    set('site', s.site || 'No page yet');
    set('sites', s.third_party_sites.length);
    var ul = document.getElementById('site-list');
    ul.textContent = '';
    s.third_party_sites.forEach(function (name) {
      var li = document.createElement('li');
      li.textContent = name;
      ul.appendChild(li);
    });
    set('requests', s.requests);
    set('third-requests', s.third_party_requests);
    set('first-cookies', s.first_party_cookies);
    set('third-cookies', s.third_party_cookies);
    set('storage', s.storage_writes);
    set('fp', s.fingerprinting.length ? s.fingerprinting.join(', ') : 'none');
  }

  setInterval(function () {
    invoke('snapshot').then(render).catch(function () { });
  }, 500);
})();
