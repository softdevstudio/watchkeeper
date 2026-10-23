// Watchkeeper detection script.
//
// Injected into every frame before the page's own scripts run. It records
// what the page does: network requests, cookies written by script, storage
// writes, workers started and reads of browser features used for
// fingerprinting. It records names and addresses without query strings,
// never cookie or storage values.
//
// The page runs in the same JavaScript world as this script, so the page
// can replace any built-in function after we start. Everything this script
// relies on is therefore captured here, before the page runs, and only the
// captured copies are used later. Wrappers are Proxies, so wrapped
// functions and classes keep their names, `new` rules and subclassing.
//
// The top frame keeps a numbered buffer. The app collects it with
// d(SECRET, doc, ack, skip): events are only removed once the app confirms
// it has them, so a lost answer just means they come again next time; skip
// gives up on what is held if the app cannot read an answer. The secret
// exists only inside this script; a page can find the collector by name but
// cannot drain it. Sub-frames post plain fields to the top frame, which
// checks and encodes them itself.
//
// __WK_TOKEN__, __WK_SECRET__ and __WK_FRAMEKEY__ are filled in per launch.
(function () {
  'use strict';
  var NAME = '__wk___WK_TOKEN__';
  var SECRET = '__WK_SECRET__';
  var FRAMEKEY = '__WK_FRAMEKEY__';

  // ---- Captured built-ins -------------------------------------------------
  var R_apply = Reflect.apply;
  var R_construct = Reflect.construct;
  var R_defineProperty = Reflect.defineProperty;
  var R_getOwnPropertyDescriptor = Reflect.getOwnPropertyDescriptor;
  var R_getPrototypeOf = Reflect.getPrototypeOf;
  var O_create = Object.create;
  var O_freeze = Object.freeze;
  var P_Proxy = Proxy;
  var S_String = String;
  var S_fromCharCode = String.fromCharCode;
  var S_charCodeAt = String.prototype.charCodeAt;
  var S_indexOf = String.prototype.indexOf;
  var S_slice = String.prototype.slice;
  var M_random = Math.random;
  var M_floor = Math.floor;
  var D_now = Date.now;
  var W = window;
  var hasOwn = Object.prototype.hasOwnProperty;

  function getter(proto, name) {
    var d = proto && R_getOwnPropertyDescriptor(proto, name);
    return d && d.get;
  }
  function call(fn, self, args) { return R_apply(fn, self, args || []); }

  var U_URL = W.URL;
  var U_href = getter(U_URL.prototype, 'href');
  var U_protocol = getter(U_URL.prototype, 'protocol');
  var U_host = getter(U_URL.prototype, 'host');
  var U_origin = getter(U_URL.prototype, 'origin');
  var U_pathname = getter(U_URL.prototype, 'pathname');
  var Req_url = W.Request && getter(W.Request.prototype, 'url');
  var Loc_href = getter(W.location, 'href') || getter(R_getPrototypeOf(W.location), 'href');
  var locObj = W.location;
  var postMessage = W.postMessage;
  var addEventListener = W.EventTarget && EventTarget.prototype.addEventListener;
  var ME_data = W.MessageEvent && getter(MessageEvent.prototype, 'data');
  var setIntervalF = W.setInterval;

  function str(s) {
    try { return typeof s === 'string' ? s : S_String(s); } catch (e) { return ''; }
  }
  function cut(s, n) { return s.length > n ? call(S_slice, s, [0, n]) : s; }
  function idx(s, c) { return call(S_indexOf, s, [c]); }
  function trim(s) {
    var a = 0, b = s.length;
    while (a < b && call(S_charCodeAt, s, [a]) <= 32) a++;
    while (b > a && call(S_charCodeAt, s, [b - 1]) <= 32) b--;
    return call(S_slice, s, [a, b]);
  }
  // JSON string encoder that uses only captured functions.
  // Surrogate pairs are kept; a lone surrogate, which no JSON reader
  // accepts, becomes U+FFFD.
  function jstr(s) {
    var out = '"';
    var h = '0123456789abcdef';
    for (var i = 0; i < s.length; i++) {
      var c = call(S_charCodeAt, s, [i]);
      if (c === 34) out += '\\"';
      else if (c === 92) out += '\\\\';
      else if (c >= 0xd800 && c <= 0xdbff) {
        var d = i + 1 < s.length ? call(S_charCodeAt, s, [i + 1]) : 0;
        if (d >= 0xdc00 && d <= 0xdfff) { out += S_fromCharCode(c) + S_fromCharCode(d); i++; }
        else out += '\\ufffd';
      } else if (c >= 0xdc00 && c <= 0xdfff) out += '\\ufffd';
      else if (c < 32 || c === 0x2028 || c === 0x2029) {
        out += '\\u' + h[(c >> 12) & 15] + h[(c >> 8) & 15] + h[(c >> 4) & 15] + h[c & 15];
      } else out += S_fromCharCode(c);
    }
    return out + '"';
  }
  function encodeEvent(ev) {
    // Every event is a flat record of strings with a fixed set of fields.
    var out = '{"k":' + jstr(ev.k) + ',"o":' + jstr(ev.o);
    if (ev.c) out += ',"c":"1"';
    if (ev.m) out += ',"m":"1"';
    if (ev.u !== undefined) out += ',"u":' + jstr(ev.u);
    if (ev.t !== undefined) out += ',"t":' + jstr(ev.t);
    if (ev.n !== undefined) out += ',"n":' + jstr(ev.n);
    if (ev.s !== undefined) out += ',"s":' + jstr(ev.s);
    if (ev.a !== undefined) out += ',"a":' + jstr(ev.a);
    return out + '}';
  }

  // ---- Frame identity ------------------------------------------------------
  if (call(hasOwn, W, [NAME])) return;
  var isTop = false;
  try { isTop = W === W.top; } catch (e) { isTop = false; }
  var origin = '';
  try { origin = str(call(U_origin, new U_URL(call(Loc_href, locObj)))); } catch (e) { origin = ''; }
  var ME_origin = W.MessageEvent && getter(MessageEvent.prototype, 'origin');
  var PTE_persisted = W.PageTransitionEvent && getter(PageTransitionEvent.prototype, 'persisted');
  var perf = W.performance;
  var P_getEntriesByType = W.Performance && Performance.prototype.getEntriesByType;

  // An address reduced to what matters: origin and path, no query or
  // fragment, at most 512 characters. Only http(s) and ws(s).
  var base = '';
  try { base = call(Loc_href, locObj); } catch (e) { base = ''; }
  function clean(u) {
    try {
      var x = new U_URL(str(u), base);
      var p = call(U_protocol, x);
      if (p !== 'http:' && p !== 'https:' && p !== 'ws:' && p !== 'wss:') return null;
      return { url: cut(p + '//' + call(U_host, x) + call(U_pathname, x), 512), host: call(U_host, x) };
    } catch (e) { return null; }
  }
  function newDoc() {
    var d = '';
    for (var i = 0; i < 4; i++) d += S_String(M_floor(M_random() * 2147483647));
    return d + '-' + S_String(D_now());
  }

  // ---- Buffer and acceptance (top frame) ------------------------------------
  // Every event, from this frame or a sub-frame, passes through accept():
  // its fields are checked and clipped, it is deduplicated and its urgency
  // is decided here, then it is encoded here. Nothing a page sends is ever
  // passed on as text.
  var MAX_PENDING = 5000;
  var MAX_HOSTS = 20000;   // hosts tracked for the reserve lane
  var PER_HOST_RESERVE = 50;
  var MAX_BATCH_CHARS = 1000000;
  var buf = O_create(null);
  var head = 1;            // seq of the oldest event still held
  var next = 1;            // seq the next event gets
  var dropped = 0;         // events lost: buffer full or over a limit
  var DOC = newDoc();
  var lastEnd = 1;         // one past the last event in the latest answer
  var seen, seenHost, nHost, nHostConf, nSeen, nSeenConf, nSeenReserve;
  function resetSeen() {
    seen = O_create(null); seenHost = O_create(null);
    nHost = 0; nHostConf = 0; nSeen = 0; nSeenConf = 0; nSeenReserve = 0;
  }
  resetSeen();

  // urgent: 0 normal, 1 reserve lane, 2 the engine's own record of a new
  // host, which has a lane of its own because a page cannot fake it.
  function enqueue(encoded, urgent) {
    var limit = urgent === 2 ? MAX_PENDING * 3 : urgent ? MAX_PENDING * 2 : MAX_PENDING;
    if (next - head >= limit) { dropped++; return; }
    buf[next] = encoded;
    next++;
  }

  function field(v, n) { return typeof v === 'string' ? cut(v, n) : ''; }

  // o is the origin of the frame the event came from, as the engine
  // reports it, never as a page claims it.
  function accept(k, a, b, c, o, fromMsg) {
    var ev, key, urgent = 0;
    if (k === 'res') {
      var cl = clean(a);
      if (!cl) return;
      var t = field(b, 32) || 'other';
      var conf = c === '1' ? '1' : '';
      // A frame can only vouch for addresses on its own host.
      if (conf && fromMsg) {
        var oc = clean(o);
        if (!oc || oc.host !== cl.host) conf = '';
      }
      // Each host's first addresses travel in the reserve lane, so one
      // host's flood cannot crowd out another's. Hosts the engine really
      // contacted are counted apart from hosts only named in calls.
      key = 'r ' + t + ' ' + conf + ' ' + cl.url;
      if (seen[key]) return;
      var hk = (conf ? 'c ' : 'n ') + cl.host;
      var count = seenHost[hk];
      if (count === undefined) {
        if (conf && nHostConf < MAX_HOSTS) { count = 0; nHostConf++; }
        else if (!conf && nHost < MAX_HOSTS) { count = 0; nHost++; }
      }
      if (count !== undefined && count < PER_HOST_RESERVE) {
        seenHost[hk] = count + 1;
        urgent = conf ? 2 : 1;
      }
      ev = { k: 'res', o: o, u: cl.url, t: t, c: conf, m: fromMsg ? '1' : '' };
    } else if (k === 'cookie') {
      var n = field(a, 256);
      if (!n) return;
      key = 'c ' + o + ' ' + n;
      if (seen[key]) return;
      ev = { k: 'cookie', o: o, n: n };
    } else if (k === 'store') {
      var kind = a === 'localStorage' || a === 'sessionStorage' || a === 'indexedDB' ? a : '';
      if (!kind) return;
      var nm = field(b, 256);
      key = 's ' + o + ' ' + kind + ' ' + nm;
      if (seen[key]) return;
      ev = { k: 'store', o: o, s: kind, n: nm };
    } else if (k === 'fp') {
      var api = a === 'canvas' || a === 'webgl' || a === 'audio' ? a : '';
      if (!api) return;
      key = 'f ' + o + ' ' + api;
      if (seen[key]) return;
      urgent = 1;
      ev = { k: 'fp', o: o, a: api };
    } else {
      return;
    }
    // Requests the engine confirmed and fingerprinting reads are counted
    // apart, so a flood of calls cannot use up the room they need.
    if (ev.k === 'fp' || (ev.k === 'res' && ev.c && !fromMsg)) {
      if (nSeenConf >= 60000) { dropped++; return; }
      nSeenConf++;
    } else if (urgent) {
      // A host's first addresses: their own count too.
      if (nSeenReserve >= 60000) { dropped++; return; }
      nSeenReserve++;
    } else {
      if (nSeen >= 60000) { dropped++; return; }
      nSeen++;
    }
    seen[key] = 1;
    enqueue(encodeEvent(ev), urgent);
  }

  // Sub-frames send plain fields; the top frame does the rest.
  function submit(k, a, b, c) {
    if (isTop) { accept(k, a, b, c, origin, false); return; }
    try {
      var msg = O_create(null);
      R_defineProperty(msg, 'wk', { value: FRAMEKEY, enumerable: true });
      R_defineProperty(msg, 'k', { value: k, enumerable: true });
      R_defineProperty(msg, 'a', { value: a, enumerable: true });
      R_defineProperty(msg, 'b', { value: b, enumerable: true });
      R_defineProperty(msg, 'c', { value: c, enumerable: true });
      call(postMessage, W.top, [msg, '*']);
    } catch (e) { /* detached or blocked */ }
  }

  // ---- Recorders -----------------------------------------------------------
  // Local deduplication first, so a busy frame posts each thing once.
  var mine = O_create(null), nMine = 0;
  function once(key) {
    if (mine[key]) return false;
    if (nMine < 60000) { mine[key] = 1; nMine++; }
    return true;
  }
  function res(u, t, confirmed) {
    var cl = clean(u);
    if (!cl && t === 'worker') cl = clean(origin + '/(worker from a blob)');
    if (!cl) return;
    if (!once('r ' + t + ' ' + (confirmed ? 1 : 0) + ' ' + cl.url)) return;
    submit('res', cl.url, t, confirmed ? '1' : '');
  }
  function cookie(v) {
    var s = str(v);
    var semi = idx(s, ';');
    if (semi >= 0) s = call(S_slice, s, [0, semi]);
    var eq = idx(s, '=');
    if (eq >= 0) s = call(S_slice, s, [0, eq]);
    s = cut(trim(s), 256);
    if (!s || !once('c ' + s)) return;
    submit('cookie', s, '', '');
  }
  function store(kind, name) {
    var n = cut(str(name), 256);
    if (!once('s ' + kind + ' ' + n)) return;
    submit('store', kind, n, '');
  }
  function fp(api) {
    if (!once('f ' + api)) return;
    submit('fp', api, '', '');
  }

  // Storage objects and their key listing, captured now.
  var St_length = W.Storage && getter(Storage.prototype, 'length');
  var St_key = W.Storage && Storage.prototype.key;
  var local = null, session = null;
  try { local = W.localStorage; } catch (e) { local = null; }
  try { session = W.sessionStorage; } catch (e) { session = null; }
  function sweep() {
    // Catches keys written by assignment (localStorage.x = 1).
    var pairs = [[local, 'localStorage'], [session, 'sessionStorage']];
    for (var p = 0; p < 2; p++) {
      var st = pairs[p][0];
      if (!st) continue;
      try {
        var n = call(St_length, st);
        for (var i = 0; i < n && i < 5000; i++) store(pairs[p][1], call(St_key, st, [i]));
      } catch (e) { /* storage blocked */ }
    }
  }

  // ---- Resource timing --------------------------------------------------------
  var E_name = W.PerformanceEntry && getter(PerformanceEntry.prototype, 'name');
  var E_init = W.PerformanceResourceTiming && getter(PerformanceResourceTiming.prototype, 'initiatorType');
  function fromEntries(entries) {
    for (var i = 0; i < entries.length; i++) {
      var e = entries[i];
      var t = 'other';
      try { t = str(call(E_init, e)) || 'other'; } catch (x) { }
      // Seen by the engine itself, so the address was really contacted.
      res(call(E_name, e), t, true);
    }
  }

  // ---- A page restored by Back or Forward -------------------------------------
  // The engine may bring back a whole document from its cache. It gets a
  // new id, so the app treats it as the page it now is, and what it already
  // loaded is reported again from the engine's own list.
  if (addEventListener && PTE_persisted) {
    call(addEventListener, W, ['pageshow', function (ev) {
      var persisted = false;
      try { persisted = call(PTE_persisted, ev) === true; } catch (e) { }
      if (!persisted) return;
      mine = O_create(null); nMine = 0;
      if (isTop) { DOC = newDoc(); resetSeen(); }
      try { fromEntries(call(P_getEntriesByType, perf, ['resource'])); } catch (e) { }
      sweep();
    }, true]);
  }

  // ---- Collector (top frame only) -------------------------------------------
  if (isTop) {
    var collector = O_create(null);
    R_defineProperty(collector, 'd', {
      value: function (secret, doc, ack, skip) {
        if (secret !== SECRET) return null;
        if (doc === DOC && typeof ack === 'number') {
          while (head <= ack && head < next) { delete buf[head]; head++; }
        }
        if (skip === true && doc === DOC) {
          // The app could not read the last answers: give up on what they
          // held, nothing newer.
          while (head < lastEnd && head < next) { delete buf[head]; head++; dropped++; }
        }
        sweep();
        var from = head;
        var href = '';
        try { href = call(Loc_href, locObj); } catch (e) { href = ''; }
        var c = clean(href);
        var out = '{"doc":' + jstr(DOC) + ',"url":' + jstr(c ? c.url : cut(str(href), 512)) +
          ',"from":' + S_String(from) + ',"dropped":' + S_String(dropped) + ',"ev":[';
        var i = from;
        for (; i < next && i < from + 10000; i++) {
          if (i > from && out.length + buf[i].length > MAX_BATCH_CHARS) break;
          out += (i > from ? ',' : '') + buf[i];
        }
        lastEnd = i;
        return out + ']}';
      }
    });
    O_freeze(collector);
    R_defineProperty(W, NAME, { value: collector, enumerable: false, configurable: false, writable: false });

    // Events from sub-frames. A page that reads these messages learns the
    // frame key and can add events under its own origin, but cannot read,
    // remove or corrupt any.
    if (addEventListener) {
      call(addEventListener, W, ['message', function (m) {
        var d, o;
        try { d = call(ME_data, m); o = str(call(ME_origin, m)); } catch (e) { return; }
        if (!d || typeof d !== 'object') return;
        try {
          if (d.wk !== FRAMEKEY) return;
          // Opaque origins (sandboxed frames) and the page's own origin can
          // be driven by the page, so they may not claim the engine
          // confirmed an address. The app also refuses the claim from any
          // origin on the page's own site.
          if (o === 'null') o = '';
          var conf = o && o !== origin && d.c === '1' ? '1' : '';
          accept(d.k, d.a, d.b, conf, cut(o, 512), true);
        } catch (e) { /* hostile object */ }
      }, true]);
    }
  } else {
    R_defineProperty(W, NAME, { value: true, enumerable: false });
    if (setIntervalF) {
      try { call(setIntervalF, W, [sweep, 1000]); } catch (e) { }
    }
  }

  // ---- What the engine itself records ----------------------------------------
  try {
    var PO = W.PerformanceObserver;
    var getEntries = W.PerformanceObserverEntryList && PerformanceObserverEntryList.prototype.getEntries;
    var observer = R_construct(PO, [function (list) { fromEntries(call(getEntries, list)); }]);
    call(PO.prototype.observe, observer, [{ type: 'resource', buffered: true }]);
  } catch (e) { /* no resource timing */ }

  // ---- Wrappers ------------------------------------------------------------------
  function handler(trap, fn) {
    var h = O_create(null);
    h[trap] = fn;
    return h;
  }
  function wrapCall(obj, name, before) {
    try {
      var orig = obj && obj[name];
      if (typeof orig !== 'function') return;
      var p = new P_Proxy(orig, handler('apply', function (target, self, args) {
        try { before(self, args); } catch (e) { }
        return R_apply(target, self, args);
      }));
      var d = R_getOwnPropertyDescriptor(obj, name) || { writable: true, enumerable: false, configurable: true };
      R_defineProperty(obj, name, { value: p, writable: d.writable !== false, enumerable: !!d.enumerable, configurable: d.configurable !== false });
    } catch (e) { }
  }
  function wrapNew(obj, name, before) {
    try {
      var orig = obj[name];
      if (typeof orig !== 'function') return;
      var p = new P_Proxy(orig, handler('construct', function (target, args, newTarget) {
        try { before(args); } catch (e) { }
        return R_construct(target, args, newTarget);
      }));
      var d = R_getOwnPropertyDescriptor(obj, name) || { writable: true, enumerable: false, configurable: true };
      R_defineProperty(obj, name, { value: p, writable: d.writable !== false, enumerable: !!d.enumerable, configurable: d.configurable !== false });
    } catch (e) { }
  }

  // Calls that may never show up in resource timing. The address is
  // converted to a string once, here, and that same string is what the
  // engine gets, so an object that answers differently the second time
  // cannot show us one address and connect to another.
  function fix(args, i) {
    if (args.length <= i) return undefined;
    var v = args[i];
    if (typeof v !== 'string') { v = str(v); args[i] = v; }
    return v;
  }
  wrapCall(W, 'fetch', function (self, args) {
    var input = args[0], u;
    // A Request's address cannot change once made, so it is read as is.
    if (Req_url) { try { u = call(Req_url, input); } catch (e) { u = undefined; } }
    if (u === undefined) u = fix(args, 0);
    res(u, 'fetch', false);
  });
  if (W.XMLHttpRequest) wrapCall(XMLHttpRequest.prototype, 'open', function (self, args) { res(fix(args, 1), 'xhr', false); });
  if (W.Navigator) wrapCall(Navigator.prototype, 'sendBeacon', function (self, args) { res(fix(args, 0), 'beacon', false); });
  wrapNew(W, 'WebSocket', function (args) { res(fix(args, 0), 'websocket', false); });
  wrapNew(W, 'EventSource', function (args) { res(fix(args, 0), 'eventsource', false); });
  // Workers run their own code where this script cannot reach; we record
  // that one was started and from where.
  wrapNew(W, 'Worker', function (args) { res(fix(args, 0), 'worker', false); });
  wrapNew(W, 'SharedWorker', function (args) { res(fix(args, 0), 'worker', false); });

  // Cookies written by script: the setter is wrapped where it lives, on
  // Document.prototype, so document itself gains no own property.
  try {
    var cd = R_getOwnPropertyDescriptor(Document.prototype, 'cookie');
    if (cd && cd.set && cd.get) {
      var setP = new P_Proxy(cd.set, handler('apply', function (target, self, args) {
        try { cookie(args[0]); } catch (e) { }
        return R_apply(target, self, args);
      }));
      R_defineProperty(Document.prototype, 'cookie', { get: cd.get, set: setP, enumerable: cd.enumerable, configurable: true });
    }
  } catch (e) { }

  // Storage writes. Keys only.
  if (W.Storage) wrapCall(Storage.prototype, 'setItem', function (self, args) {
    store(self === local ? 'localStorage' : 'sessionStorage', args[0]);
  });
  if (W.IDBFactory) wrapCall(IDBFactory.prototype, 'open', function (self, args) { store('indexedDB', args[0]); });

  // Browser features read back for fingerprinting.
  function isUnmasked(args) { return args[0] === 0x9245 || args[0] === 0x9246; }
  function isDebugExt(args) { return args[0] === 'WEBGL_debug_renderer_info'; }
  if (W.HTMLCanvasElement) {
    wrapCall(HTMLCanvasElement.prototype, 'toDataURL', function () { fp('canvas'); });
    wrapCall(HTMLCanvasElement.prototype, 'toBlob', function () { fp('canvas'); });
  }
  if (W.CanvasRenderingContext2D) wrapCall(CanvasRenderingContext2D.prototype, 'getImageData', function () { fp('canvas'); });
  var gl = [W.WebGLRenderingContext, W.WebGL2RenderingContext];
  for (var g = 0; g < gl.length; g++) {
    if (!gl[g]) continue;
    wrapCall(gl[g].prototype, 'getParameter', function (self, args) { if (isUnmasked(args)) fp('webgl'); });
    wrapCall(gl[g].prototype, 'getExtension', function (self, args) { if (isDebugExt(args)) fp('webgl'); });
  }
  wrapNew(W, 'OfflineAudioContext', function () { fp('audio'); });
  wrapNew(W, 'webkitOfflineAudioContext', function () { fp('audio'); });
})();
