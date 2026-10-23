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
// d(SECRET, doc, ack): events are only removed once the app confirms it has
// them, so a lost answer just means they come again next time. The secret
// exists only inside this script; a page can find the collector by name but
// cannot drain it. Sub-frames post their events to the top frame.
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
  function jstr(s) {
    var out = '"';
    for (var i = 0; i < s.length; i++) {
      var c = call(S_charCodeAt, s, [i]);
      if (c === 34) out += '\\"';
      else if (c === 92) out += '\\\\';
      else if (c < 32 || c === 0x2028 || c === 0x2029 || (c >= 0xd800 && c <= 0xdfff)) {
        var h = '0123456789abcdef';
        out += '\\u' + h[(c >> 12) & 15] + h[(c >> 8) & 15] + h[(c >> 4) & 15] + h[c & 15];
      } else out += S_fromCharCode(c);
    }
    return out + '"';
  }
  function encodeEvent(ev) {
    // Every event is a flat record of strings with a fixed set of fields.
    var out = '{"k":' + jstr(ev.k) + ',"o":' + jstr(ev.o);
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

  // A link to the URL's site-relevant part: origin and path, no query or
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

  // ---- Buffer (top frame) and posting (sub-frames) ------------------------
  var MAX_PENDING = 5000;
  var MAX_HOSTS = 2000;
  var PER_HOST_RESERVE = 50;
  var buf = O_create(null);
  var head = 1;            // seq of the oldest event still held
  var next = 1;            // seq the next event gets
  var dropped = 0;         // events lost because the buffer was full
  var DOC = '';
  for (var di = 0; di < 4; di++) DOC += S_String(M_floor(M_random() * 2147483647));
  DOC += '-' + S_String(D_now());

  // Holds encoded events. Urgent ones (a host not seen before, a
  // fingerprinting read) may use a reserve beyond the normal limit, so a
  // flood of repeats cannot push a new tracker out.
  function enqueue(encoded, urgent) {
    if (next - head >= MAX_PENDING && !urgent) { dropped++; return; }
    if (next - head >= MAX_PENDING * 2) { dropped++; return; }
    buf[next] = encoded;
    next++;
  }

  function push(ev, urgent) {
    if (isTop) {
      enqueue(encodeEvent(ev), urgent);
    } else {
      try {
        var msg = O_create(null);
        R_defineProperty(msg, 'wk', { value: FRAMEKEY, enumerable: true });
        R_defineProperty(msg, 'ev', { value: encodeEvent(ev), enumerable: true });
        R_defineProperty(msg, 'u', { value: urgent ? '1' : '', enumerable: true });
        call(postMessage, W.top, [msg, '*']);
      } catch (e) { /* detached or blocked */ }
    }
  }

  // ---- Recorders -----------------------------------------------------------
  var seenRes = O_create(null), nRes = 0;
  var seenHost = O_create(null), nHost = 0;
  function res(u, t) {
    var c = clean(u);
    if (!c && t === 'worker') {
      // A worker built from a blob: record it under the frame's origin.
      c = clean(origin + '/(worker from a blob)');
    }
    if (!c) return;
    // Each host's first addresses travel in the reserve lane, so one
    // host's flood cannot crowd out another's.
    var urgent = false;
    var count = seenHost[c.host];
    if (count === undefined && nHost < MAX_HOSTS) { count = 0; nHost++; }
    if (count !== undefined && count < PER_HOST_RESERVE) { seenHost[c.host] = count + 1; urgent = true; }
    var key = t + ' ' + c.url;
    if (seenRes[key]) return;
    if (nRes < 50000) { seenRes[key] = 1; nRes++; }
    push({ k: 'res', o: origin, u: c.url, t: t }, urgent);
  }
  var seenCookie = O_create(null), nCookie = 0;
  function cookie(v) {
    var s = str(v);
    var semi = idx(s, ';');
    if (semi >= 0) s = call(S_slice, s, [0, semi]);
    var eq = idx(s, '=');
    if (eq >= 0) s = call(S_slice, s, [0, eq]);
    s = cut(trim(s), 256);
    if (!s || seenCookie[s]) return;
    if (nCookie >= 5000) { dropped++; return; }
    seenCookie[s] = 1; nCookie++;
    push({ k: 'cookie', o: origin, n: s }, false);
  }
  var seenStore = O_create(null), nStore = 0;
  function store(kind, name) {
    var n = cut(str(name), 256);
    var key = kind + ' ' + n;
    if (seenStore[key]) return;
    if (nStore >= 5000) { dropped++; return; }
    seenStore[key] = 1; nStore++;
    push({ k: 'store', o: origin, s: kind, n: n }, false);
  }
  var seenFp = O_create(null);
  function fp(api) {
    if (seenFp[api]) return;
    seenFp[api] = 1;
    push({ k: 'fp', o: origin, a: api }, true);
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
      var s = pairs[p][0];
      if (!s) continue;
      try {
        var n = call(St_length, s);
        for (var i = 0; i < n && i < 5000; i++) store(pairs[p][1], call(St_key, s, [i]));
      } catch (e) { /* storage blocked */ }
    }
  }

  // ---- Collector (top frame only) -------------------------------------------
  if (isTop) {
    var collector = O_create(null);
    R_defineProperty(collector, 'd', {
      value: function (secret, doc, ack) {
        if (secret !== SECRET) return null;
        sweep();
        if (doc === DOC && typeof ack === 'number') {
          while (head <= ack && head < next) { delete buf[head]; head++; }
        }
        var from = head;
        var end = next - head > 2000 ? head + 2000 : next;
        var href = '';
        try { href = call(Loc_href, locObj); } catch (e) { href = ''; }
        var c = clean(href);
        var out = '{"doc":' + jstr(DOC) + ',"url":' + jstr(c ? c.url : cut(str(href), 512)) +
          ',"from":' + S_String(from) + ',"dropped":' + S_String(dropped) + ',"ev":[';
        for (var i = from; i < end; i++) out += (i > from ? ',' : '') + buf[i];
        return out + ']}';
      }
    });
    O_freeze(collector);
    R_defineProperty(W, NAME, { value: collector, enumerable: false, configurable: false, writable: false });

    // Events from sub-frames. A page that reads these messages learns the
    // frame key and can add events, but cannot read or remove any.
    if (addEventListener) {
      call(addEventListener, W, ['message', function (m) {
        var d;
        try { d = call(ME_data, m); } catch (e) { return; }
        if (!d || typeof d !== 'object') return;
        try {
          if (d.wk !== FRAMEKEY || typeof d.ev !== 'string' || d.ev.length > 4096) return;
          enqueue(d.ev, d.u === '1');
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
    var E_name = W.PerformanceEntry && getter(PerformanceEntry.prototype, 'name');
    var E_init = W.PerformanceResourceTiming && getter(PerformanceResourceTiming.prototype, 'initiatorType');
    var observer = R_construct(PO, [function (list) {
      var entries = call(getEntries, list);
      for (var i = 0; i < entries.length; i++) {
        var e = entries[i];
        var t = 'other';
        try { t = str(call(E_init, e)) || 'other'; } catch (x) { }
        res(call(E_name, e), t);
      }
    }]);
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

  // Calls that may never show up in resource timing.
  wrapCall(W, 'fetch', function (self, args) {
    var input = args[0], u = input;
    if (Req_url) { try { u = call(Req_url, input); } catch (e) { u = input; } }
    res(u, 'fetch');
  });
  if (W.XMLHttpRequest) wrapCall(XMLHttpRequest.prototype, 'open', function (self, args) { res(args[1], 'xhr'); });
  if (W.Navigator) wrapCall(Navigator.prototype, 'sendBeacon', function (self, args) { res(args[0], 'beacon'); });
  wrapNew(W, 'WebSocket', function (args) { res(args[0], 'websocket'); });
  wrapNew(W, 'EventSource', function (args) { res(args[0], 'eventsource'); });
  // Workers run their own code where this script cannot reach; we record
  // that one was started and from where.
  wrapNew(W, 'Worker', function (args) { res(args[0], 'worker'); });
  wrapNew(W, 'SharedWorker', function (args) { res(args[0], 'worker'); });

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
