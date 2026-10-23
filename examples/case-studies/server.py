#!/usr/bin/env python3
"""Made-up sites for Watchkeeper's case studies and use examples.

Every site here is served by this one script on this machine. Nothing is
fetched from the internet. The names end in .example, a domain reserved
for examples, so they need one line in your hosts file (see README.md).

    python3 server.py            # serves on port 8080

Then open, in Watchkeeper:

    http://shop.example:8080/       a product page behind a cookie banner
    http://recipes.example:8080/    a blog that adds trackers on a later visit
    http://landing.example:8080/    a landing page to save a report from
    http://quiz.example:8080/       a quiz with its own canvas and a vendor's
    http://localhost:8090/          a staging copy of a site (second port)
"""

import http.server
import socketserver
import threading

GIF = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;"
PORT = 8080
STAGING_PORT = 8090
VISITS = {"recipes": 0}

STYLE = b"""<style>
body{margin:0;font:16px/1.6 Georgia,serif;color:#222;background:#fff}
header{background:#1f2a24;color:#fff;padding:14px 28px;font:700 24px Georgia}
main{max-width:640px;margin:24px auto;padding:0 20px}
h1{font-size:28px;line-height:1.2}
.box{background:#f4f1ea;border:1px solid #ddd5c6;padding:16px;margin:18px 0}
.banner{position:fixed;left:0;right:0;bottom:0;background:#fff;border-top:3px solid #222;
padding:18px 28px;font:15px system-ui,sans-serif;box-shadow:0 -6px 20px rgba(0,0,0,.15)}
.banner button{font:600 14px system-ui;padding:8px 14px;margin-right:8px;border:1px solid #222;background:#fff}
.banner button.yes{background:#222;color:#fff}
button.buy{font:600 16px system-ui;padding:10px 18px;background:#1f2a24;color:#fff;border:0}
</style>"""


def shop():
    return b"""<!doctype html><html><head><meta charset=utf-8><title>Linen Shirt | Shop</title>""" + STYLE + b"""
<link rel=stylesheet href="http://fonts-cdn.example:8080/serif.css"></head>
<body><header>Harbour &amp; Loom</header><main>
<h1>Washed Linen Shirt</h1><p>Soft, loose and made to be worn for years. Machine washable.</p>
<div class=box>EUR 79 &middot; sizes S to XL &middot; free returns</div>
<p><button class=buy>Add to basket</button></p>
<p>Woven in Portugal from European flax.</p>
</main>
<div class=banner><b>We value your privacy.</b> We and our partners use cookies to improve your experience.
<br><br><button class=yes>Accept all</button><button>Reject all</button><button>Settings</button></div>
<script>document.cookie='basket=empty; path=/';</script>
<script src="http://analytics.example:8080/a.js"></script>
<script src="http://adpixel.example:8080/p.js"></script>
<iframe src="http://chat-widget.example:8080/widget" style="position:fixed;right:16px;bottom:120px;border:0;width:64px;height:64px"></iframe>
</body></html>"""


def recipes():
    VISITS["recipes"] += 1
    later = VISITS["recipes"] > 1
    extra = b""
    if later:
        extra = (b'<script src="http://video-ads.example:8080/v.js"></script>'
                 b'<img src="http://data-broker.example:8080/sync.gif" width=1 height=1 alt="">')
    return b"""<!doctype html><html><head><meta charset=utf-8><title>Weeknight Lentil Soup</title>""" + STYLE + b"""</head>
<body><header>Small Kitchen</header><main>
<h1>Weeknight Lentil Soup</h1><p>Forty minutes, one pot, and it tastes better the next day.</p>
<div class=box>Serves 4 &middot; 40 minutes &middot; vegetarian</div>
<p>Soften an onion, two carrots and a stick of celery in olive oil. Add garlic, cumin and red lentils.</p>
<p>Pour in a litre of stock and simmer until the lentils fall apart.</p>
</main>
<script src="http://analytics.example:8080/a.js"></script>
<script src="http://adpixel.example:8080/p.js"></script>
""" + extra + b"""</body></html>"""


def landing():
    return b"""<!doctype html><html><head><meta charset=utf-8><title>Try Ledgerly Free</title>""" + STYLE + b"""</head>
<body><header>Ledgerly</header><main>
<h1>Bookkeeping that does itself</h1><p>Connect your bank and Ledgerly sorts every payment by morning.</p>
<div class=box>30 days free &middot; no card needed</div>
<p><button class=buy>Start free trial</button></p>
</main>
<script src="http://heatmap.example:8080/h.js"></script>
<script src="http://ab-test.example:8080/ab.js"></script>
<img src="http://adpixel.example:8080/conversion.gif" width=1 height=1 alt="">
<img src="http://social-ads.example:8080/tr.gif" width=1 height=1 alt="">
<script>localStorage.setItem('trial_step','landing');</script>
</body></html>"""


def quiz():
    return b"""<!doctype html><html><head><meta charset=utf-8><title>Which Coast Suits You</title>""" + STYLE + b"""</head>
<body><header>Weekend Quiz</header><main>
<h1>Which coast suits you?</h1><p>Five questions. Your answer comes with a chart you can save.</p>
<canvas id=chart width=300 height=120 style="border:1px solid #ddd"></canvas>
<p><a id=save href="#">Save the chart as an image</a></p>
</main>
<script>
var c=document.getElementById('chart'),g=c.getContext('2d');
g.fillStyle='#2b6a8a';g.fillRect(20,40,60,70);g.fillRect(100,20,60,90);g.fillRect(180,60,60,50);
document.getElementById('save').href=c.toDataURL();
</script>
<script src="http://fp-vendor.example:8080/fp.js"></script>
</body></html>"""


def staging():
    return b"""<!doctype html><html><head><meta charset=utf-8><title>Staging: Harbour &amp; Loom</title>""" + STYLE + b"""</head>
<body><header>Harbour &amp; Loom (staging)</header><main>
<h1>New product page</h1><p>Checking what this build sends out before it goes live.</p>
</main>
<script src="http://analytics.example:8080/a.js"></script>
<script src="http://adpixel.example:8080/p.js"></script>
<script src="http://session-replay.example:8080/rec.js"></script>
</body></html>"""


SCRIPTS = {
    ("analytics.example", "/a.js"): b"""(function(){var T='http://analytics.example:8080';
try{localStorage.setItem('visitor_id','v'+Date.now());}catch(e){}
navigator.sendBeacon(T+'/collect','pageview');})();""",
    ("adpixel.example", "/p.js"): b"""(function(){var T='http://adpixel.example:8080';
new Image().src=T+'/px.gif';
try{var c=document.createElement('canvas');var g=c.getContext('2d');g.font='14px Arial';g.fillText('Cwm fjord',2,14);c.toDataURL();}catch(e){}
fetch('http://sync.partner-ads.example:8080/match',{mode:'no-cors'});})();""",
    ("video-ads.example", "/v.js"): b"""(function(){fetch('http://video-ads.example:8080/vast',{mode:'no-cors'});})();""",
    ("heatmap.example", "/h.js"): b"""(function(){var T='http://heatmap.example:8080';
try{var c=document.createElement('canvas');c.getContext('2d').fillText('hm',1,9);c.toDataURL();}catch(e){}
try{var g=document.createElement('canvas').getContext('webgl');if(g){var x=g.getExtension('WEBGL_debug_renderer_info');if(x)g.getParameter(x.UNMASKED_RENDERER_WEBGL);}}catch(e){}
try{var A=window.OfflineAudioContext||window.webkitOfflineAudioContext;if(A)new A(1,44100,44100);}catch(e){}
try{new WebSocket('ws://heatmap.example:8080/live');}catch(e){}
fetch(T+'/session',{method:'POST',mode:'no-cors',body:'clicks'});})();""",
    ("ab-test.example", "/ab.js"): b"""(function(){try{localStorage.setItem('ab_variant','B');}catch(e){}
fetch('http://ab-test.example:8080/assign',{mode:'no-cors'});})();""",
    ("fp-vendor.example", "/fp.js"): b"""(function(){var T='http://fp-vendor.example:8080';
try{var c=document.createElement('canvas');var g=c.getContext('2d');g.textBaseline='top';g.font='14px Arial';g.fillText('fingerprint',2,2);c.toDataURL();}catch(e){}
try{var w=document.createElement('canvas').getContext('webgl');if(w){var x=w.getExtension('WEBGL_debug_renderer_info');if(x)w.getParameter(x.UNMASKED_RENDERER_WEBGL);}}catch(e){}
try{var A=window.OfflineAudioContext||window.webkitOfflineAudioContext;if(A)new A(1,44100,44100);}catch(e){}
navigator.sendBeacon(T+'/id','hash');})();""",
    ("session-replay.example", "/rec.js"): b"""(function(){try{new WebSocket('ws://session-replay.example:8080/stream');}catch(e){}
fetch('http://session-replay.example:8080/start',{method:'POST',mode:'no-cors',body:'{}'});})();""",
    ("chat-widget.example", "/widget"): b"""<!doctype html><body style="margin:0"><div style="width:56px;height:56px;border-radius:50%;background:#1f2a24"></div>
<script>try{localStorage.setItem('chat_seen','1');}catch(e){}fetch('/hello',{mode:'no-cors'});</script></body>""",
    ("fonts-cdn.example", "/serif.css"): b"body{}",
}


class Handler(http.server.BaseHTTPRequestHandler):
    def reply(self):
        host = self.headers.get("Host", "").split(":")[0]
        path = self.path.split("?")[0]
        port = self.server.server_address[1]
        pages = {"shop.example": shop, "recipes.example": recipes,
                 "landing.example": landing, "quiz.example": quiz}
        if port == STAGING_PORT and path == "/":
            body, kind = staging(), "text/html"
        elif host in pages and path == "/":
            body, kind = pages[host](), "text/html"
        elif (host, path) in SCRIPTS:
            body = SCRIPTS[(host, path)]
            kind = ("text/css" if path.endswith(".css") else
                    "text/html" if path == "/widget" else "text/javascript")
        else:
            body, kind = GIF, "image/gif"
        self.send_response(200)
        self.send_header("Content-Type", kind)
        if host == "shop.example" and path == "/":
            self.send_header("Set-Cookie", "session=s1; Path=/; HttpOnly")
        self.end_headers()
        self.wfile.write(body)

    do_GET = reply
    do_POST = reply

    def log_message(self, *args):
        pass


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    staging_server = Server(("127.0.0.1", STAGING_PORT), Handler)
    threading.Thread(target=staging_server.serve_forever, daemon=True).start()
    print(f"Serving on http://127.0.0.1:{PORT} and http://localhost:{STAGING_PORT}")
    Server(("127.0.0.1", PORT), Handler).serve_forever()
