# Watchkeeper

A tiny browser that shows you who's watching.

You browse normally. A panel on the side tells you what the site is doing behind your back: which other sites it tells about your visit, the cookies it leaves, what it writes to your computer, and whether it reads the features of your device that are used to tell one visitor from another.

Watchkeeper uses the web engine your system already has: WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux. That keeps the app small.

## Status

Version 0.2. Installers for Windows, macOS and Linux are on the [releases page](https://github.com/softdevstudio/watchkeeper/releases/latest). Every build passes an end-to-end self-test on its own system before it is published.

The builds are not signed yet. On Windows, SmartScreen warns you: choose More info, then Run anyway. On macOS, right-click Watchkeeper and choose Open the first time. On Linux, make the AppImage executable and run it, or install the .deb.

You can also open a site straight from the command line: `watchkeeper example.com`.

## What the panel shows

For the site in the window:

- **Other sites told about your visit.** Every request the page makes is recorded. Requests to a different site (a different registrable domain, so `cdn.example.com` counts as part of `example.com`) are listed by site.
- **Addresses requested**: distinct addresses, without query strings, in total and on other sites.
- **Cookies** for this site, and for the other sites the engine saw it contact, read from the engine's own cookie store. That includes cookies a page script cannot see. The store covers your whole session, so cookies for another site may include ones it set on an earlier page.
- **Storage writes**: keys written to localStorage, sessionStorage and IndexedDB, and how many came from frames belonging to other sites.
- **Fingerprinting reads**: the page read back a drawn canvas, asked WebGL for your graphics hardware, or created an offline audio context. These are the usual building blocks of device fingerprinting. Some sites use them for ordinary reasons, so a read is a sign, not proof. Each read names the site whose script made it ("canvas by tracker.net"). That is the nearest script on the engine's call stack when the read happened, and it counts only if the engine's own list of loaded resources shows that script. Code written into the page itself is credited to the page's site. Reads from a `data:` script, and reads made while the page tampers with how the engine reports errors, are shown as "by unknown".
- **Workers started.** A page can run code in a worker, out of the page's own reach; Watchkeeper notes that one started.

If a page does more than Watchkeeper can count, the panel says how much it left out. The list of other sites is kept apart from everything else, so a flood of requests or cookies cannot push a site off it.

Watchkeeper records names and addresses without query strings, never values. It does not keep cookie contents, storage contents or anything you type. The page's full address is shown in the address bar and nowhere else.

## Saving a report

**Save report** in the panel writes two files about the site in the window to your Downloads folder: an HTML page you can open in any browser and a JSON file for tools. Both are named after the site and the time, for example `watchkeeper-example.com-2026-10-23-140805.html`, and an existing file is never overwritten.

A report holds everything the panel counts, in full: each other site told about the visit with every address on it and how it was reached (after a flood past 2,000 other sites, addresses on the sites left out are counted, not listed), the cookies in the session's store for this site and the sites the engine saw it contact (every other cookie, such as those left by sites you visited earlier, is counted, never named), cookie names written by scripts, storage writes, fingerprinting reads and who made them, workers started, how much the page did that could not be counted, the last visit when history is kept, and what Watchkeeper cannot see. Like everything else in Watchkeeper it has names and addresses without query strings, never cookie or storage values.

Everything in a report came from the page, so the HTML shows it all as plain text, with characters that reverse or hide text replaced by a visible mark. It has no scripts and no links, and it tells the browser that opens it to load nothing at all. Only Watchkeeper's own panel can save a report; a web page cannot.

The JSON starts with `"format": "watchkeeper-report"` and a `format_version`, so tools can tell what they are reading.

## Your session

The page runs in a private session that starts empty every time you open Watchkeeper and is deleted when you close it. No saved logins, no cookies, no browsing history carried over.

## Last visit

To tell you how a site changed since you were last there ("12 other sites last visit, 19 today"), Watchkeeper keeps a short record on your computer. For each site you open it keeps the date of each visit (in UTC), how many other sites the page contacted, the names of up to 300 of them that the engine itself saw contacted, how many cookies and storage writes there were, and which fingerprinting reads were made by which site. It keeps the last 20 visits per site, at most 2,000 sites and about 8 MB in all, dropping the visits made longest ago first. It never keeps addresses, cookie names or contents, storage keys or what you type into forms. Site names are kept as the page used them, so a page that really sends text to a server named after that text could put it into a site name.

The panel shows the last visit's date and count next to today's, and lists the sites that are new since then. **Forget this site** removes that site's record, including the visit in progress. **Forget all** removes everything. Only Watchkeeper's own panel can do this; a web page cannot.

The record is one file, `history.json`, in Watchkeeper's data folder: `~/.local/share/net.watchkeeper.app` on Linux, `~/Library/Application Support/net.watchkeeper.app` on macOS and `%APPDATA%\net.watchkeeper.app` on Windows. Start Watchkeeper with `--no-history` to keep nothing at all. If the file is damaged, Watchkeeper sets it aside as `history.json.bad-` followed by the time, starts a new one and says so in the panel.

## How it works

A small script runs in every frame of the page before the page's own scripts. It notes what the page loads (from the engine's own resource list), calls made with fetch, XMLHttpRequest, beacons, WebSockets and EventSource, workers started, cookies written by script, storage writes and fingerprinting reads. The top frame keeps a numbered buffer; Watchkeeper collects it a few times a second and confirms what it received, so nothing is lost if an answer goes astray. Cookies come from the engine's cookie store.

The page runs in the same JavaScript world as that script, so a page could try to fool it. The script takes its own copies of every built-in function it uses before the page runs, so a page that later replaces JSON, URL, string or array functions changes nothing for Watchkeeper. Its wrappers keep the originals' names, `new` rules and subclassing, and turn an address into text once, passing that same text to the engine, so an address that answers differently the second time cannot show one thing and connect to another. A page can find the collector, but draining it needs a secret the page never sees. Sub-frames send plain fields that the top frame checks and encodes itself, so a forged message can add a false event under the sender's own origin but cannot stall or corrupt collection. Only a frame on another site may say the engine itself recorded a request, and only for its own site's addresses; the page's own frames, including sandboxed ones, cannot. When the engine brings a page back from its cache with Back or Forward, Watchkeeper treats it as the page it now is.

The page cannot reach Watchkeeper either. Its webview is granted no app commands, it cannot open Watchkeeper's own pages, and Back, Forward and Reload use the engine's own controls, so a page cannot trap them. The self-test checks all of this, including a hostile page that tries every trick above.

## What it can't see

Being honest about the gaps matters more than the counts:

- **Each OS's engine behaves a little differently.** Safari's engine on macOS and WebKitGTK on Linux block some third-party cookies by themselves, so the same site can leave fewer cookies there than on Windows. That is a real difference in what you're exposed to, not a counting error.
- **Workers.** Watchkeeper sees a worker start, but not the requests the worker makes. The same goes for service workers.
- **Leaving a page.** A beacon sent as you leave a page can be missed: the page is gone before Watchkeeper collects it.
- **Inside other sites' iframes**, the script runs and reports what it sees, but an engine can hold back some details of cross-origin frames.
- **A determined site can notice Watchkeeper.** It can find the collector by name, and Tauri's bridge is visible in the page (its calls are refused). It can add false events about itself. It cannot read or remove what Watchkeeper collected, and cannot see or change anything outside its own page.
- **Floods.** A page that makes thousands of requests in an instant overflows the buffer. Every host keeps a reserve for its first 50 addresses, and requests the engine itself recorded have a lane of their own, so a flood of requests, cookies, made-up sites or forged messages cannot push a site the page really contacted off the list. Some addresses are dropped, and the panel says how many. A WebSocket, EventSource or worker start (calls the engine does not list) made after a very large flood, around a hundred thousand calls, can still be among them.
- **Who fingerprints.** The site named for a read is the nearest script on the call stack, not proof that script chose to read. A page can hand a reading function to a library it loads (a loop helper that calls whatever it is given) and the read is then credited to the library's site. A script can hide behind the page by running its reads from text: a timer or event handler given as a string is credited to the page's own site on some engines (WebKitGTK does this), and a `data:` script shows as "by unknown". A page can make every read unplaced by making the stack useless; those show as "by unknown" too. A frame on a second domain the page controls can vouch for scripts on that domain only.
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
2. A hostile page. It looks for the collector and tries to drain it, learns the sub-frame key and sends 4,200 forged messages (broken text, wrong types, oversized names and made-up "confirmed" sites), has a sandboxed frame claim 4,200 more "confirmed" sites, writes a storage key containing a broken character, floods 8,000 addresses, 300 cookies, 2,100 made-up sites and 61,000 calls that go nowhere, replaces JSON, URL, Reflect and several string and array functions, then loads a tracker and contacts a real new site. Every tracker address and the new site must still be seen, nothing may be drained, and the flood must show as left out.
3. Back. From a page on another site that replaced `history.back`, Watchkeeper's Back must go back without calling it, and the page the engine brings back from its cache must be watched as its own site again, with what it does after coming back in the report.
4. Reports. After the ordinary page and again after the hostile one, the page tries to save a report (nothing may appear), then the panel saves one. The JSON must parse and list every tracker address the servers saw that the report holds, and the HTML must hold no script or image elements and no links, even though the hostile page writes a storage key built to break it open.
5. Last visit. Back on `127.0.0.1` for the second time, the panel must show the first visit. The history file must hold both sites, no addresses or cookie names, and none of the made-up sites from the hostile page. The page then tries to forget everything, which must change nothing; the panel forgets `localhost`, then everything, and the visit in progress must not come back.

The ordinary page's canvas, WebGL and audio reads are made by the tracker's script, so each must be credited to `localhost`, not to the page. The hostile page also attacks this: it sets the stack depth to zero before the tracker's script runs, names a script it never loads from inside a `data:` script, and has a frame rewrite its stacks to name the tracker's real script. The tracker's read must still name its script, and neither fake may be credited.

The ordinary page also uses the read-twice trick on a WebSocket: whichever address the engine connects to must be the one in the report.

Results are written to `results-dir/selftest.json`; the exit code is 0 only if every check passed. CI runs it on Windows, macOS and Linux on every push and posts the results on each run. Results recorded on the development machine (Linux) are in [test/results/latest](test/results/latest).

## Repository

- `crates/watchkeeper-core`: the detection script and site reports. No window code, so it can be shared with Unweb later.
- `crates/watchkeeper`: the app, built with Tauri 2.
- `scripts/record-results.sh`: runs every check and records the results.
- `scripts/third-party-licenses.py`: regenerates THIRD_PARTY_LICENSES.md.

## License

Apache License 2.0. See LICENSE, NOTICE and THIRD_PARTY_LICENSES.md.

Contact: info@watchkeeper.net
