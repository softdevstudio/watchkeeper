// Watchkeeper detection script.
//
// Injected into every frame before the page's own scripts run. It records
// what the page does: network requests, cookies written by script, storage
// writes and reads of browser features used for fingerprinting. It never
// records cookie or storage values, only names.
//
// The top frame keeps a buffer that the app drains with eval. Sub-frames
// pass their events to the top frame with postMessage. Nothing here calls
// into the app, so the page gets no channel back to native code.
//
// __WK_TOKEN__ is replaced with a random token for each app launch.
(function () {
  'use strict';
  var TOKEN = '__WK_TOKEN__';
  var NAME = '__wk_' + TOKEN;
  if (Object.prototype.hasOwnProperty.call(window, NAME)) return;

  var isTop = false;
  try { isTop = window === window.top; } catch (e) { isTop = false; }

  var MAX = 5000;
  var buf = [];
  var seen = Object.create(null);
  var origin = '';
  try { origin = location.origin; } catch (e) { origin = ''; }

  function push(ev) {
    ev.o = origin;
    if (isTop) {
      if (buf.length < MAX) buf.push(ev);
    } else {
      try { window.top.postMessage({ __wk: TOKEN, ev: ev }, '*'); } catch (e) { /* detached */ }
    }
  }

  function abs(u) {
    try { return new URL(String(u), location.href).href; } catch (e) { return null; }
  }

  function res(u, t) {
    var a = abs(u);
    if (!a || !/^(https?|wss?):/i.test(a)) return;
    var key = t + ' ' + a;
    if (seen[key]) return;
    seen[key] = 1;
    push({ k: 'res', u: a, t: t });
  }

  var stored = Object.create(null);
  function store(kind, name) {
    var key = kind + ' ' + name;
    if (stored[key]) return;
    stored[key] = 1;
    push({ k: 'store', s: kind, n: String(name) });
  }

  var fpSeen = Object.create(null);
  function fp(api) {
    if (fpSeen[api]) return;
    fpSeen[api] = 1;
    push({ k: 'fp', a: api });
  }

  // Sweep storage for keys written by direct assignment (localStorage.x = 1),
  // which does not go through setItem.
  function sweepStorage() {
    [['localStorage', 'localStorage'], ['sessionStorage', 'sessionStorage']].forEach(function (p) {
      try {
        var s = window[p[0]];
        for (var i = 0; i < s.length; i++) store(p[1], s.key(i));
      } catch (e) { /* storage blocked */ }
    });
  }

  if (isTop) {
    Object.defineProperty(window, NAME, {
      value: Object.freeze({
        drain: function () {
          sweepStorage();
          var out = JSON.stringify(buf);
          buf = [];
          return out;
        }
      }),
      enumerable: false, configurable: false, writable: false
    });
    window.addEventListener('message', function (m) {
      var d = m.data;
      if (!d || d.__wk !== TOKEN || !d.ev || typeof d.ev.k !== 'string') return;
      if (buf.length < MAX) buf.push(d.ev);
    }, true);
  } else {
    Object.defineProperty(window, NAME, { value: true, enumerable: false });
  }

  // Everything the page loads: scripts, images, pixels, iframes, styles,
  // fetch and XHR, as the engine itself records them.
  try {
    new PerformanceObserver(function (list) {
      list.getEntries().forEach(function (e) { res(e.name, e.initiatorType || 'other'); });
    }).observe({ type: 'resource', buffered: true });
  } catch (e) { /* no resource timing */ }

  // Calls that may never show up in resource timing.
  var F = window.fetch;
  if (F) {
    window.fetch = function (input) {
      try { res(input && input.url ? input.url : input, 'fetch'); } catch (e) { }
      return F.apply(this, arguments);
    };
  }
  try {
    var XO = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function (method, u) {
      res(u, 'xhr');
      return XO.apply(this, arguments);
    };
  } catch (e) { }
  if (navigator.sendBeacon) {
    var SB = navigator.sendBeacon;
    navigator.sendBeacon = function (u) {
      res(u, 'beacon');
      return SB.apply(navigator, arguments);
    };
  }
  var WS = window.WebSocket;
  if (WS) {
    var W = function (u, p) {
      res(u, 'websocket');
      return p === undefined ? new WS(u) : new WS(u, p);
    };
    W.prototype = WS.prototype;
    ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'].forEach(function (k) { W[k] = WS[k]; });
    window.WebSocket = W;
  }

  // Cookies written by script. Names only.
  try {
    var cd = Object.getOwnPropertyDescriptor(Document.prototype, 'cookie');
    if (cd && cd.set && cd.get) {
      Object.defineProperty(document, 'cookie', {
        configurable: true, enumerable: true,
        get: function () { return cd.get.call(document); },
        set: function (v) {
          try {
            var n = String(v).split(';')[0].split('=')[0].trim();
            if (n) push({ k: 'cookie', n: n });
          } catch (e) { }
          cd.set.call(document, v);
        }
      });
    }
  } catch (e) { }

  // Storage writes. Keys only.
  try {
    var SI = Storage.prototype.setItem;
    Storage.prototype.setItem = function (k) {
      var kind = 'storage';
      try { kind = this === window.localStorage ? 'localStorage' : 'sessionStorage'; } catch (e) { }
      store(kind, k);
      return SI.apply(this, arguments);
    };
  } catch (e) { }
  try {
    var IO = IDBFactory.prototype.open;
    IDBFactory.prototype.open = function (n) {
      store('indexedDB', n);
      return IO.apply(this, arguments);
    };
  } catch (e) { }

  // Browser features read back for fingerprinting.
  function wrap(proto, name, api, test) {
    if (!proto || typeof proto[name] !== 'function') return;
    var orig = proto[name];
    proto[name] = function () {
      try { if (!test || test.apply(this, arguments)) fp(api); } catch (e) { }
      return orig.apply(this, arguments);
    };
  }
  var canvasProto = window.HTMLCanvasElement && HTMLCanvasElement.prototype;
  wrap(canvasProto, 'toDataURL', 'canvas');
  wrap(canvasProto, 'toBlob', 'canvas');
  wrap(window.CanvasRenderingContext2D && CanvasRenderingContext2D.prototype, 'getImageData', 'canvas');
  var unmasked = function (p) { return p === 0x9245 || p === 0x9246; };
  wrap(window.WebGLRenderingContext && WebGLRenderingContext.prototype, 'getParameter', 'webgl', unmasked);
  wrap(window.WebGL2RenderingContext && WebGL2RenderingContext.prototype, 'getParameter', 'webgl', unmasked);
  var debugExt = function (n) { return n === 'WEBGL_debug_renderer_info'; };
  wrap(window.WebGLRenderingContext && WebGLRenderingContext.prototype, 'getExtension', 'webgl', debugExt);
  wrap(window.WebGL2RenderingContext && WebGL2RenderingContext.prototype, 'getExtension', 'webgl', debugExt);
  ['OfflineAudioContext', 'webkitOfflineAudioContext'].forEach(function (n) {
    var C = window[n];
    if (typeof C !== 'function') return;
    var Wrapped = function () {
      fp('audio');
      var args = Array.prototype.slice.call(arguments);
      return new (Function.prototype.bind.apply(C, [null].concat(args)))();
    };
    Wrapped.prototype = C.prototype;
    window[n] = Wrapped;
  });
})();
