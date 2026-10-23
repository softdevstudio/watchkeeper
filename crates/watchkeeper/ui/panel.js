(function () {
  'use strict';
  var invoke = window.__TAURI_INTERNALS__.invoke;
  function set(id, v) { document.getElementById(id).textContent = String(v); }

  function list(id, items) {
    var ul = document.getElementById(id);
    ul.textContent = '';
    items.forEach(function (name) {
      var li = document.createElement('li');
      li.textContent = name;
      ul.appendChild(li);
    });
  }
  function plural(n, one, many) { return n + ' ' + (n === 1 ? one : many); }

  var site = '';
  var last = '';
  function render(s) {
    var key = JSON.stringify(s);
    if (key === last) return;
    last = key;
    site = s.site;
    set('site', s.site || 'No page yet');
    document.getElementById('hint').hidden = !!s.site;
    set('sites', s.third_party_sites.length);
    list('site-list', s.third_party_sites);
    set('addresses', s.addresses);
    set('third-addresses', s.third_party_addresses);
    set('first-cookies', s.first_party_cookies);
    set('third-cookies', s.third_party_cookies);
    set('storage', s.storage_writes);
    set('third-storage', s.third_party_storage_writes);
    set('fp', s.fingerprinting.length ? s.fingerprinting.join(', ') : 'none');
    list('fp-list', s.fingerprinting_by.map(function (f) { return f.api + ' by ' + f.by; }));
    set('workers', s.workers);
    renderHistory(s);
    var d = document.getElementById('dropped');
    d.hidden = !s.dropped;
    if (s.dropped) {
      d.textContent = 'This page did ' + s.dropped + ' more things than Watchkeeper could count. ' +
        'Pages sometimes flood like this to hide what they do; the sites listed above are still complete.';
    }
  }

  function renderHistory(s) {
    var box = document.getElementById('history');
    box.hidden = !s.history || !s.site;
    var lv = s.last_visit;
    var p = document.getElementById('last-visit');
    if (lv) {
      p.textContent = plural(lv.other_site_count, 'other site', 'other sites') + ' on ' + lv.date +
        ', ' + s.third_party_sites.length + ' now.' +
        (lv.new_sites.length ? ' New this time:' : ' No new ones this time.');
      list('new-sites', lv.new_sites);
    } else {
      p.textContent = 'This is your first visit to this site that Watchkeeper has recorded.';
      list('new-sites', []);
    }
    var n = document.getElementById('history-note');
    n.hidden = !s.history_note;
    n.textContent = s.history_note || '';
  }

  document.getElementById('forget-site').addEventListener('click', function () {
    if (site) invoke('forget_site', { site: site }).catch(function () { });
  });
  document.getElementById('forget-all').addEventListener('click', function () {
    invoke('forget_all').catch(function () { });
  });

  setInterval(function () {
    invoke('snapshot').then(render).catch(function () { });
  }, 500);
})();
