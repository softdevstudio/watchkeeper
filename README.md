# Watchkeeper

A tiny browser that shows you who's watching.

You browse normally. A panel on the side tells you what the site is doing behind your back: which other sites it tells about your visit, the cookies it leaves, what it writes to your computer, and whether it reads the features of your device that are used to tell one visitor from another.

Watchkeeper uses the web engine your system already has: WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux. That keeps the app small.

## Status

Early. The window layout, the detection script and the panel work, and an end-to-end self-test checks them on each OS's engine. There is no release to download yet; you can build it from source. Version 0.1 is next.

## What the panel shows

For the site in the window:

- **Other sites told about your visit.** Every request the page makes is recorded. Requests to a different site (a different registrable domain, so `cdn.example.com` counts as part of `example.com`) are listed by site.
- **Requests**, in total and to other sites.
- **Cookies** set by this site, and by the other sites it contacted, read from the engine's own cookie store. That includes cookies a page script cannot see.
- **Storage writes**: keys written to localStorage, sessionStorage and IndexedDB.
- **Fingerprinting reads**: the page read back a drawn canvas, asked WebGL for your graphics hardware, or created an offline audio context. These are the usual building blocks of device fingerprinting. Some sites use them for ordinary reasons, so a read is a sign, not proof.

Watchkeeper records names, never values. It does not keep cookie contents, storage contents or anything you type.

## Your session

The page runs in a private session that starts empty every time you open Watchkeeper and is deleted when you close it. No history, no saved logins, no cookies carried over.

## How it works

A small script runs in every frame of the page before the page's own scripts. It notes what the page loads (from the engine's own resource list), calls made with fetch, XMLHttpRequest, beacons and WebSockets, cookies written by script, storage writes and fingerprinting reads. The top frame keeps a buffer; Watchkeeper collects it a few times a second. Cookies come from the engine's cookie store.

The page cannot reach Watchkeeper. The page's webview is granted no app commands, and the script never calls into the app; Watchkeeper reads the buffer from outside. The self-test checks that a page trying to call an app command is refused.

## What it can't see

Being honest about the gaps matters more than the counts:

- **Each OS's engine behaves a little differently.** Safari's engine on macOS and WebKitGTK on Linux block some third-party cookies by themselves, so the same site can leave fewer cookies there than on Windows. That is a real difference in what you're exposed to, not a counting error.
- **Inside other sites' iframes**, the script runs and reports what it sees, but an engine can hold back some details of cross-origin frames.
- **Service workers** run outside the page; their requests may not be seen.
- **A determined site can notice Watchkeeper**, for example by checking whether built-in functions have been wrapped, and can feed the script false events about itself. It cannot see or change anything outside its own page.
- Storage written by direct assignment (`localStorage.x = 1`) is caught when Watchkeeper sweeps the keys, a few times a second.

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

This runs the real app against two local test sites. A page on `127.0.0.1` pulls in a "tracker" from `localhost`, which counts as a different site. The tracker sets cookies, writes storage, reads canvas, WebGL and audio, sends a beacon, loads an iframe and makes a call after a delay. The tracker server logs every request it receives, and the test passes only if Watchkeeper saw every one of them, plus the cookies, storage writes and fingerprinting reads. It also checks that the toolbar, page and panel tile the window before and after a resize, and that the page cannot call app commands. Results are written to `results-dir/selftest.json`; the exit code is 0 only if every check passed.

CI runs it on Windows, macOS and Linux on every push. Recorded results are in [test/results/latest](test/results/latest).

## Repository

- `crates/watchkeeper-core`: the detection script and site reports. No window code, so it can be shared with Unweb later.
- `crates/watchkeeper`: the app, built with Tauri 2.
- `scripts/record-results.sh`: runs every check and records the results.
- `scripts/third-party-licenses.py`: regenerates THIRD_PARTY_LICENSES.md.

## License

Apache License 2.0. See LICENSE, NOTICE and THIRD_PARTY_LICENSES.md.

Contact: info@watchkeeper.net
