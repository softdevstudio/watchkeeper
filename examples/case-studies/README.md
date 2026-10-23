# Case study pages

Made-up sites used for the case studies and use examples on [watchkeeper.net](https://watchkeeper.net/). One Python script serves all of them on your own machine; nothing comes from the internet. The screenshots on the site were taken from these pages.

## Run them

The sites use names ending in `.example`, which is reserved for examples, so your computer needs to be told they live on your own machine. Add this line to your hosts file (`/etc/hosts` on Linux and macOS, `C:\Windows\System32\drivers\etc\hosts` on Windows):

```
127.0.0.1 shop.example recipes.example landing.example quiz.example analytics.example adpixel.example sync.partner-ads.example chat-widget.example fonts-cdn.example video-ads.example data-broker.example heatmap.example ab-test.example social-ads.example fp-vendor.example session-replay.example
```

Then start the server with Python 3 (no packages needed):

```
python3 server.py
```

and open the pages in Watchkeeper:

| Address | What it shows |
|---|---|
| `http://shop.example:8080/` | A product page that calls analytics, an ad pixel, a chat widget and a font server before its cookie banner is answered |
| `http://recipes.example:8080/` | A blog that adds a video ad network and a data broker from the second visit on, for the Last visit section |
| `http://landing.example:8080/` | A landing page with a heatmap script, an A/B test and two conversion pixels, to save a report from |
| `http://quiz.example:8080/` | A quiz that reads its own chart canvas for a good reason, next to a vendor's fingerprinting script |
| `http://localhost:8090/` | A staging copy of the shop on a second port |

Remove the hosts line when you're done.
