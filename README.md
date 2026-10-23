# Watchkeeper

A tiny browser that shows you who's watching.

You browse normally. A panel on the side tells you what the site is doing behind your back: which other sites it tells about your visit, the cookies it leaves, what it writes to your computer, and whether it reads the features of your device that are used to tell one visitor from another.

Watchkeeper uses the web engine your system already has: WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux. That keeps the app small.

## Status

Version 0.1. Installers for Windows, macOS and Linux are on the [releases page](https://github.com/softdevstudio/watchkeeper/releases/latest). Every build passes an end-to-end self-test on its own system before it is published.

The builds are not signed yet. On Windows, SmartScreen warns you: choose More info, then Run anyway. On macOS, right-click Watchkeeper and choose Open the first time. On Linux, make the AppImage executable and run it, or install the .deb.

You can also open a site straight from the command line: `watchkeeper example.com`.

## What the panel shows

For the site in the window:

- **Other sites told about your visit.** Every request the page makes is recorded. Requests to a different site (a different registrable domain, so `cdn.example.com` counts as part of `example.com`) are listed by site.
- **Addresses requested**: distinct addresses, without query strings, in total and on other sites.
- **Cookies** for this site, and for the other sites it contacted, read from the engine's own cookie store. That includes cookies a page script cannot see. The store covers your whole session, so cookies for another site may include ones it set on an earlier page.
- **Storage writes**: keys written to localStorage, sessionStorage and IndexedDB, and how many came from frames belonging to other sites.
- **Fingerprinting reads**: the page read back a drawn canvas, asked WebGL for your graphics hardware, or created an offline audio context. These are the usual building blocks of device fingerprinting. Some sites use them for ordinary reasons, so a read is a sign, not proof.
- **Workers started.** A page can run code in a worker, out of the page's own reach; Watchkeeper notes that one started.

If a page does more than Watchkeeper can count, the panel says how much it left out. The list of other sites is kept apart from everything else, so a flood of requests or cookies cannot push a site off it.

Watchkeeper records names and addresses without query strings, never values. It does not keep cookie contents, storage contents or anything you type. The page's full address is shown in the address bar and nowhere else.

## Your session

The page runs in a private session that starts empty every time you open Watchkeeper and is deleted when you close it. No history, no saved logins, no cookies carried over.

## How it works

A small script runs in every frame of the page before the page's own scripts. It notes what the page loads (from the engine's own resource list), calls made with fetch, XMLHttpRequest, beacons, WebSockets and EventSource, workers started, cookies written by script, storage writes and fingerprinting reads. The top frame keeps a numbered buffer; Watchkeeper collects it a few times a second and confirms what it received, so nothing is lost if an answer goes astray. Cookies come from the engine's cookie store.

The page runs in the same JavaScript world as that script, so a page could try to fool it. The script takes its own copies of every built-in function it uses before the page runs, so a page that later replaces JSON, URL, string or array functions changes nothing for Watchkeeper. Its wrappers keep the originals' names, `new` rules and subclassing. A page can find the collector, but draining it needs a secret the page never sees.

The page cannot reach Watchkeeper either. Its webview is granted no app commands, it cannot open Watchkeeper's own pages, and Back, Forward and Reload use the engine's own controls, so a page cannot trap them. The self-test checks all of this, including a hostile page that tries every trick above.

## What it can't see

Being honest about the gaps matters more than the counts:

- **Each OS's engine behaves a little differently.** Safari's engine on macOS and WebKitGTK on Linux block some third-party cookies by themselves, so the same site can leave fewer cookies there than on Windows. That is a real difference in what you're exposed to, not a counting error.
- **Workers.** Watchkeeper sees a worker start, but not the requests the worker makes. The same goes for service workers.
- **Leaving a page.** A beacon sent as you leave a page can be missed: the page is gone before Watchkeeper collects it.
- **Inside other sites' iframes**, the script runs and reports what it sees, but an engine can hold back some details of cross-origin frames.
- **A determined site can notice Watchkeeper.** It can find the collector by name, and Tauri's bridge is visible in the page (its calls are refused). It can add false events about itself. It cannot read or remove what Watchkeeper collected, and cannot see or change anything outside its own page.
- **Floods.** A page that makes thousands of requests in an instant overflows the buffer. Each site keeps a reserve for its first 50 addresses and the list of sites is never crowded out, but some addresses are dropped, and the panel says how many.
- Storage written by direct assignment (`localStorage.x = 1`) is caught when Watchkeeper sweeps the keys, about once a second.

## Build from source

You need Rust 1.90 or newer.

- **Windows:** the WebView2 runtime (built into Windows 10 and 11).
- **macOS:** Xcode command line tools.
- **Linux:** WebKitGTK 4.1 and GTK 3 development packages. On Debian or Ubuntu:

  ```
  sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
  ```

Then:

```
cargo build --release
./target/release/watchkeeper
```

## Self-test

```
watchkeeper --selftest results-dir
```

This runs the real app against local test sites. A page on `127.0.0.1` pulls in a "tracker" from `localhost`, which counts as a different site. The test servers log every request they receive, and those logs are the ground truth.

1. An ordinary page. The tracker loads a script, a stylesheet, a pixel and an iframe, calls fetch, XMLHttpRequest, a beacon and two WebSockets, sets cookies, writes storage (also from inside its iframe), reads canvas, WebGL and audio, starts a worker and makes a call after a delay. Every request the tracker server saw must be in the report, except the one made from inside the worker. Wrapped classes must behave like the originals, the layout must tile the window before and after a resize, and the page must not be able to call app commands.
2. A hostile page. It looks for the collector and tries to drain it, floods 8,000 addresses and 300 cookies, replaces JSON, URL, Reflect and several string and array functions, then loads a tracker. Every tracker address must still be seen, nothing may be drained, and the flood must show as left out.
3. The hostile page replaced `history.back`. Watchkeeper's Back must still go back without calling it.

Results are written to `results-dir/selftest.json`; the exit code is 0 only if every check passed. CI runs it on Windows, macOS and Linux on every push and posts the results on each run. Results recorded on the development machine (Linux) are in [test/results/latest](test/results/latest).

## Repository

- `crates/watchkeeper-core`: the detection script and site reports. No window code, so it can be shared with Unweb later.
- `crates/watchkeeper`: the app, built with Tauri 2.
- `scripts/record-results.sh`: runs every check and records the results.
- `scripts/third-party-licenses.py`: regenerates THIRD_PARTY_LICENSES.md.

## License

Apache License 2.0. See LICENSE, NOTICE and THIRD_PARTY_LICENSES.md.

Contact: info@watchkeeper.net
