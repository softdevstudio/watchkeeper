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
    document.getElementById('hint').hidden = !!s.site;
    set('sites', s.third_party_sites.length);
    var ul = document.getElementById('site-list');
    ul.textContent = '';
    s.third_party_sites.forEach(function (name) {
      var li = document.createElement('li');
      li.textContent = name;
      ul.appendChild(li);
    });
    set('addresses', s.addresses);
    set('third-addresses', s.third_party_addresses);
    set('first-cookies', s.first_party_cookies);
    set('third-cookies', s.third_party_cookies);
    set('storage', s.storage_writes);
    set('third-storage', s.third_party_storage_writes);
    set('fp', s.fingerprinting.length ? s.fingerprinting.join(', ') : 'none');
    set('workers', s.workers);
    var d = document.getElementById('dropped');
    d.hidden = !s.dropped;
    if (s.dropped) {
      d.textContent = 'This page did ' + s.dropped + ' more things than Watchkeeper could count. ' +
        'Pages sometimes flood like this to hide what they do; the sites listed above are still complete.';
    }
  }

  setInterval(function () {
    invoke('snapshot').then(render).catch(function () { });
  }, 500);
})();
