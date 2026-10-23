# Watchkeeper

Watchkeeper is a tiny browser that shows you who's watching: a side panel lists the other sites a page tells about your visit, the cookies it leaves, its storage writes and its fingerprinting reads. It runs on the system web engines (WebView2, WKWebView, WebKitGTK) through Tauri 2.

Domain: watchkeeper.net
Code repository: softdevstudio/watchkeeper
Site repository: photozeitgeist/watchkeeper.net
Language: Rust, Tauri 2
Code license: Apache 2.0
Contact: info@watchkeeper.net

Family: Unweb (unweb.org, softdevstudio/unweb) is a browser where every visit is a first visit, built later on this project's core crate. Watchkeeper Plus is a later browser an AI assistant drives while you watch, on Chromium, reusing the detection script, profile handling and log format; its language is decided when it starts. Neither is started.

## Low maintenance workflow

YK supplies the go-ahead, language choice, repositories and access. The agent handles code, tests, documentation, CI, automated releases, site work and upkeep. Answer questions without treating them as instructions to implement. Update Next only when YK approves a new step; record actual progress without inventing approval.

Work on main only; main is the default branch. This CLAUDE.md is the sole plan and session handoff file. Do not create ROADMAP.md, RESUME.md or another status file. Session notes go in Where we are, never as files in docs/; docs/ holds reference material only. Pause and report after each session and wait for YK's go-ahead before the next.

README.md is the manual and plain-language explainer. Reference documentation belongs in docs/ as Markdown. No Word or PDF. Keep LICENSE, NOTICE and THIRD_PARTY_LICENSES.md current (regenerate the last with scripts/third-party-licenses.py when dependencies change). Watchkeeper is a separate project: never mix in code, names or content from earlier projects. Do not discuss domain sale or domain value anywhere in project materials.

Write in plain words with varied sentence length. Avoid em dashes, formulaic contrasts, decorative vocabulary and forced lists of three.

## Screening record

Agreed on October 23, 2026.

Money: the browser is free and open source. Later paths are Watchkeeper Plus, the AI-driven version for teams, and paid exportable reports for site owners and agencies. No revenue model is approved.

Real problem: tracking is invisible. People can't see it, and site owners often don't know what their own pages leak, especially before the cookie banner is answered.

Plain-language technology: a tiny browser that shows you who's watching. Every site you open, the side panel lists the companies it's telling about you.

Differentiation: existing tools are browser extensions, developer tools or paid web scanners. Watchkeeper is a standalone, tiny, open-source browser where the watching is the main feature, readable by non-experts, on all three desktop OSes. The site must say plainly what it cannot see.

## Design

One window, three webviews: toolbar (top, 48 px), page (below, left) and panel (right, 320 px, at most 40 percent of the width). Tauri's multi-webview support needs the "unstable" feature. On Linux, Tauri 2.12 stacks child webviews in a vertical GTK box and ignores positions, so src/linux_layout.rs rearranges the GTK widgets directly (toolbar fixed height, then a horizontal box with the page and a fixed-width panel). On Windows and macOS the app sets each webview's position and size on every resize.

The page webview is incognito (WebView2 InPrivate, WKWebView non-persistent store, WebKitGTK ephemeral session): it starts empty at launch and is gone when the app closes. New-window requests open in the page instead; downloads are refused for now.

Security boundary: app commands get generated permissions (build.rs AppManifest) and capabilities/ui.json grants them only to the toolbar and panel webviews. The page webview is granted nothing. on_navigation (page_may_open) keeps the page on http, https, about, blob and data, and refuses tauri.localhost and ipc.localhost, where Windows serves the app's own pages. Back and forward use each engine's native call (src/history.rs: WebKitGTK go_back, WebView2 GoBack, WKWebView goBack) and reload uses Webview::reload, so page code cannot trap them. Tauri still injects its IPC bridge into the page, so a site can tell it is in a Tauri shell; its calls are refused. Removing the bridge from the page is an open point.

Collection protocol (crates/watchkeeper-core, Session): the detection script holds a numbered buffer in the top frame. The collector thread calls d(secret, doc, ack) every 400 ms through eval_with_callback; the script drops only events up to ack for its own document id and returns the rest (at most 2,000 per call) with its document id, address and a dropped count. A lost answer means the events come again and are skipped by number, which also covers WebKitGTK replaying early evals with no callback. Answers from a document already superseded are ignored, so a late answer from the previous page never lands on the next site. The report carries on across documents of the same site and resets on a new site. Keys: a token in the collector's name (findable by the page), a secret needed to drain (never visible to the page) and a frame key for sub-frame messages (a page reading those can add false events but not read or remove any). Cookies (including HttpOnly) are read from the engine's store every third round on the collector thread, never the main thread.

crates/watchkeeper-core holds the detection script (assets/detect.js), Keys, Session and SiteReport. It has no Tauri dependency so Unweb can share it; it moves to its own repository when Unweb starts. A site is a registrable domain (psl crate); IP addresses and localhost are their own sites. Limits per site: 5,000 distinct addresses, 2,000 other sites (kept separately so an address flood cannot hide a site), 1,000 script cookie names and 1,000 storage keys; text clipped at 512 bytes; batches over 4 MiB refused. Anything over a limit counts as dropped and the panel says so.

The detection script captures every built-in it uses (Reflect, Proxy, URL getters, String methods, PerformanceObserver and entry getters, Storage length and key, addEventListener, setInterval) before page code runs, encodes JSON itself and stores events in null-prototype objects, so page code that replaces built-ins changes nothing. Wrappers are Proxies with null-prototype handlers, so names, new rules and subclassing are kept. It records resource timing entries; fetch, XHR, sendBeacon, WebSocket and EventSource addresses; Worker and SharedWorker starts; cookie names written by script (the Document.prototype setter is wrapped); storage keys (setItem, indexedDB.open, and a key sweep for direct assignment: at each drain in the top frame, every second in sub-frames); and canvas, WebGL (unmasked renderer and the debug extension) and offline audio reads. Addresses are kept as origin plus path, no query or fragment, at most 512 characters; names only, never values. Events carry the origin of the frame that made them. All events, the top frame's own and sub-frames', pass through accept() in the top frame, which checks field types, clips, deduplicates and encodes them; sub-frames post plain fields, and the origin used is the engine's MessageEvent.origin. Messages from the top page's own origin or an opaque origin (sandboxed frames) cannot claim engine confirmation, and the Rust side honours a claim from a sub-frame (events carry m) only when the frame's origin is on another site. jstr writes lone surrogates as U+FFFD so every answer is valid JSON; skip drops only events up to the end of the latest answer. Seen-sets are counted in three classes (engine-confirmed and fingerprinting, each host's reserve, everything else), 60,000 each, so one class cannot use up another's room. Wrappers convert the address to a string once and pass that string on, so a read-twice object cannot show one address and connect to another. Lanes in the buffer: 5,000 normal; up to 10,000 for the reserve (each host's first 50 addresses, fingerprinting reads); up to 15,000 for addresses the engine's resource list confirmed, within each host's reserve. Each drain returns up to 10,000 events or 1,000,000 characters. A pageshow with persisted (back-forward cache) gives the document a new id, resets deduplication and replays the engine's resource list, so a restored page is watched as the page it is. Rust side: other sites past MAX_SITES are still admitted when engine-confirmed (up to another MAX_SITES); each host keeps its first 50 addresses past MAX_ADDRESSES (total under 4 × MAX_ADDRESSES); after two unreadable answers in a row the drain is sent with skip, and the page drops what it holds and counts it.

Fingerprinting attribution: detect.js reads each line of a fresh Error stack at the frame-address position (last balanced brackets after "at " in V8, after the last "@" in JSC), skips frames with no address, and sends "?" for a data:/blob: frame first, no placeable frame, or a page-set Error.prepareStackTrace (data property anywhere up Error's prototype chain). A lowered writable Error.stackTraceLimit is raised for the capture and restored. Rust credits the script's site only when the address is in confirmed_scripts (engine-confirmed, type script); otherwise the frame's site, and "?" shows as "by unknown". The site named is the nearest script on the stack, not proof it chose to read (stated in the README).

Per-site history (watchkeeper_core::history, src/recorder.rs): history.json in the app data folder (the self-test's own folder in a self-test; --no-history keeps nothing). Per visit: UTC date, counts, names of engine-confirmed other sites (up to 300, at most 253 bytes each), fingerprinting "api by site". 20 visits per site, 2,000 sites, about 8 MiB (oldest visits dropped first), files over 32 MiB refused. The collector records each round; the file is written every 12 rounds when changed and on exit, outside the recorder lock, one write at a time (Shared.saving). forget_site and forget_all are granted to the panel only (capabilities/panel.json) and suppress the visit in progress. A damaged file is set aside as history.json.bad-<secs>; a stale .tmp is removed on load.

Saved reports (watchkeeper_core::report, Shared::save_report): the panel's Save report (the async save_report command, granted to the panel only, no arguments) writes <stem>.html and <stem>.json to the Downloads folder (app data/reports when there is none; <selftest dir>/reports in a self-test). The stem is "watchkeeper-<site>-<UTC date>-<hhmmss>", keeping only [a-z0-9.-] from the site. Files are opened create_new, a taken stem gets -2, -3, and a failed write removes both. The JSON has format "watchkeeper-report", format_version 1. The HTML escapes everything (C1 controls and characters that reorder or hide text become U+FFFD), has no scripts or links, and carries a CSP meta default-src 'none'. A report lists the sites the panel counts, with their addresses. Addresses on sites past the limit are counted as unlisted_addresses. Cookies are named only for this site and engine-confirmed sites; the rest are counted as earlier_cookies. The panel's third-party cookie count uses engine-confirmed sites too.

Known gaps, stated in the README: requests from inside workers and service workers; beacons sent while leaving a page; some details of cross-origin frames on some engines.

## Code and completion standard

Tests for every operation, plus failure tests aimed at the main promise: nothing the page does gets missed, and the page cannot hide from or tamper with Watchkeeper. The end-to-end self-test (watchkeeper --selftest, src/selftest.rs) runs the real app on the OS's own engine against a local site and a local tracker on another site; the servers' request logs are the ground truth. Phase 1, an ordinary page: every tracker request except the one from inside a worker must be in the report, plus WebSockets (which resource timing does not show, so they prove the wrappers work), the worker start, cookies, storage writes credited to the right site (including a cross-site iframe's direct assignment), fingerprinting reads, wrapped classes behaving like the originals, the layout before and after a resize, and refused app commands. Phase 2, a hostile page: it finds the collector and tries to drain it, floods 8,000 addresses and 300 cookies, replaces JSON, URL, Reflect, String and Array built-ins, then loads a tracker that must still be fully seen, with the flood reported as dropped. It also learns the sub-frame key from a same-site frame and sends 4,200 forged messages, has a sandboxed frame claim 4,200 confirmed sites, writes a storage key with a lone surrogate (must be recorded as U+FFFD with collection carrying on), makes 61,000 calls that go nowhere, and names 2,100 made-up sites before contacting a real new one at [::1] (skipped where the machine has no IPv6 loopback). Phase 3: from a page on another site that replaced history.back, native Back must go back without calling it, and the page restored from the back-forward cache must be the report's site again with its post-Back calls seen. The ordinary page also uses a read-twice WebSocket address. Cookie floods stay small because WebKitGTK writes cookies slowly once a few hundred exist (about 40 ms each); the Rust caps are covered by unit tests in crates/watchkeeper-core/tests/report.rs. Record results with scripts/record-results.sh, which replaces test/results/latest/<os>; git history keeps earlier runs. Every number on the site comes from recorded results with its conditions.

CI runs on every push to main on Linux, macOS and Windows: rustfmt, clippy with warnings denied, all tests, and the self-test (under Xvfb on Linux), uploading the results. Toolchain pinned to Rust 1.97.0. Before calling a release complete, get an independent review from a separate agent that has not seen the implementation, fix findings, rerun checks and record again.

Releases go out through an automated workflow after CI succeeds on the exact commit, building installers for Windows (.msi), macOS (.dmg) and Linux (AppImage and .deb). Early versions ship unsigned, with install instructions on the site; signing certificates are YK's later decision. Never ask YK to publish a release.

## Site standard

photozeitgeist/watchkeeper.net uses the unchanged Hugo Launch template: keep themes/ and archetypes/ byte for byte, content and configuration values only, no new layouts, CSS, _index.md files or public/ folder. Pages: about, contact (info@watchkeeper.net), quick start, download and reference. Posts: launch, a plain-language explainer of web tracking, how it was tested and the name. Post front matter: quoted title in normal title case with no special characters, date only, draft: false, tags; no description or categories; never repeat the title in the body; featured_image plus the same image inline after the second or third paragraph when a post has one. Images drawn for the project live in static/images. YK connects the domain later.

## Development plan

Agreed with YK on October 23, 2026, as the plan; YK approved sessions 1 and 2. Each later session needs YK's go-ahead. Pause and report after each session.

1. Foundation and layout prototype: repository, license files, CI on three OSes, core crate, detection script, page plus panel in one window, end-to-end self-test.
2. Version 0.1: browser window with address bar, back, forward and reload, and the live panel per site (other sites contacted, cookies, storage writes); disposable session per launch; release workflow and installers.
3. Version 0.2: fingerprinting detection in the panel, per-site history ("12 companies last visit, 19 today").
4. Version 0.3: a Report button exporting a single-file HTML and a JSON of everything seen on a site.
5. Site: watchkeeper.net on the Launch template, numbers from recorded results.

Named tracker lists are an optional later add-on, only after checking each list's license against Apache 2.0.

## Where we are

October 23, 2026, session 1 (foundation and layout prototype) done:

- Cargo workspace with crates/watchkeeper-core (detection script, Event, SiteReport, Summary, site_of_host via psl) and crates/watchkeeper (Tauri 2.12.5 app with the "unstable" feature, toolbar.html, panel.html, capabilities/ui.json, icons, src/main.rs, src/linux_layout.rs, src/selftest.rs).
- The layout prototype found the Linux problem above (three equal rows, positions ignored) and src/linux_layout.rs fixes it. On Linux the self-test passes: toolbar 1200x48, page 880x752, panel 320x752, and after a resize to 900x600 the page shrinks to 580 wide while the panel keeps 320.
- Self-test on WebKitGTK 2.52.6 (Ubuntu 24.04, Xvfb): every one of the 10 tracker requests seen (style, script, pixel, fetch, XHR, beacon, iframe, a call from inside the iframe, the fingerprint report and a call 1.5 s late); script cookies, all four storage writes, canvas, WebGL and audio reads seen; the HttpOnly server cookie read from the engine; the page's call to an app command refused. WebKitGTK did not store the tracker's third-party cookie, as expected for that engine.
- Results for Windows and macOS come from CI.

October 23, 2026, session 2 (version 0.1) done, at YK's word "Go":

- App: address bar (full address shown there only, never recorded), native back and forward (src/history.rs), Webview::reload, command-line start address (watchkeeper example.com), on_navigation filter, panel with addresses, cookies, storage writes (and by other sites), fingerprinting reads, workers and a line when the page did more than could be counted.
- Release workflow (.github/workflows/release.yml): after CI on main, for a version with no tag (0.0.x never released) builds .deb and AppImage (ubuntu-22.04), .dmg (macos-latest arm64, macos-15-intel), .msi (windows-latest) with tauri-cli 2.12.4, runs the self-test with each release build on its own runner, then tags and publishes "Watchkeeper vX.Y.Z" with SHA256SUMS. Installer names carry no version.
- Independent review in three rounds by one reviewer agent that had not seen the implementation, each with probe pages against a scratch copy. Round 1: IPC boundary held; found the drain name readable and drainable, built-ins replaceable to blind the detector, Back trappable, event caps burnable and hidden, silent buffer loss, worker and unload gaps, iframe direct storage missed, wrappers breaking subclassing, no URL bounds, early drains lost on WebKitGTK, README claims ahead of the repo. Round 2: forged sub-frame text stalling collection, oversized batches, back-forward cache pages invisible (and wrong site after a cross-site Back), read-twice addresses, made-up site floods. Round 3: lone surrogates breaking JSON and forcing skip, sandboxed frames claiming confirmation, the 60,000 seen cap. All fixed as described in Design and covered by the self-test; the final round reported no regressions. Two residual cases accepted and stated in the README: a frame on a second domain the page controls can still claim confirmation for its own site's addresses only; a WebSocket, EventSource or worker start after roughly 100,000 calls can be dropped (counted).
- Self-test now 21 checks; passes on Linux (WebKitGTK 2.52.6, local, repeated runs), and in CI on ubuntu-latest, macos-latest and windows-latest (WebView2 155). Edge's engine sometimes sends an unmarked request to a server root by itself; the test lists it as the engine's own.
- CI annotations carry each OS's self-test lines, readable with gh api check-runs annotations (job logs and artifacts are not downloadable from the cloud session).

October 23, 2026, session 3 (version 0.2) done, at YK's word "Go on":

- Panel: fingerprinting reads with the site whose script made them ("canvas by tracker.net", "by unknown" when unplaced); a Last visit section comparing with the previous visit (date, count then and now, new sites) with Forget this site and Forget all.
- Per-site history and attribution as described in Design. README sections "Last visit" and "Who fingerprints".
- Self-test now 25 checks, adding: fingerprinting credited to the script's site; page cannot hide or fake who fingerprints (stack limit 0, a data: script naming an unloaded script, a frame rewriting its stack); history kept per site (names only, no made-up sites); only the panel can forget history (the page's forget calls refused, the visit in progress stays forgotten). Passes on Linux (WebKitGTK 2.52.6) and in CI on ubuntu-latest, macos-latest and windows-latest.
- Independent review in two rounds by the same reviewer agent, with probe pages. Round 1: data: scripts and unsent calls crediting reads to any site, V8 prepareStackTrace, stackTraceLimit 0 laundering, history file outgrowing the loader, typed text in made-up names, read-record flooding. Round 2: a tracker's reads run from text shifting onto the page (data: scripts now "unknown"; string timers and handlers still credited to the page on WebKitGTK, which gives them the document's address, stated in the README); a page steering credit to a library it loads (stated in the README). Round 3 confirmed the fixes with no regressions. Windows tests had picked the same temporary file; fixed.

October 23, 2026, session 4 (version 0.3) done, at YK's word "Go":

- The panel's Save report writes an HTML and a JSON report of the site in the window, as described in Design. README section "Saving a report".
- Self-test now 27 checks, adding a saved report after the ordinary page and after the hostile one. The page's save must be refused. The JSON must parse, list every tracker address the report holds, and list the same sites the panel counts. The HTML must have no script or image elements and no links, even with a storage key built to break it. scripts/record-results.sh keeps the ordinary page's report as a sample in test/results.
- Independent review in three rounds by the same reviewer agent. Round 1: earlier sites' cookies named in reports, the site list disagreeing with the panel's count after a flood, the fingerprinting card measuring something else, bidi and invisible characters, half-written files. Round 2: a page naming a site in an unsent call brought that site's cookies back, unlisted addresses not said, a few more invisible characters. Round 3: all fixed, no regressions.

October 23, 2026, session 5 (site) done, at YK's word "Go":

- photozeitgeist/watchkeeper.net held a copy of the Launch template with an earlier project's content. Its pages, posts and images were removed; themes/, archetypes/, README.md (the theme guide), .gitignore and content/archive/_index.md are unchanged. config.toml has Watchkeeper's values: hero, four figures from test/results/latest/linux (14 of 14 tracker addresses, 73,827 flood events counted, 28 checks, Apache 2.0), a Reference menu item and a default share image.
- Pages: about, contact, quick start (/quick-start/), download (/download/, latest-release installer links and unsigned install steps) and reference (/reference/: panel lines, attribution, limits, command line, files kept, report format, what it can't see). Posts dated 2026-10-23: the launch, how web tracking works, how it was tested, the name. Images in static/images: a real screenshot of the app on a made-up news page served locally (second visit, with Last visit), and three drawn images.
- Hugo could not be installed in the cloud session, so the site was checked by parsing front matter against the rules and checking every internal link; it was not built locally.
- The screenshot showed the address bar empty at start (autofocus set the toolbar's editing flag). Fixed in toolbar.js: only typing pauses the bar, and an untouched focused bar keeps its text selected. Self-test check "address bar shows the page" added (28 checks); the reviewer confirmed it fails on the old code. Released as 0.3.1.

October 23, 2026, after session 5, at YK's word ("the domain is connected. add case studies, use examples, list and add notes"):

- watchkeeper.net is live on its domain. Added three case studies (tag "case studies": a shop page before its cookie banner is answered, a recipe blog adding trackers between visits, a landing page report) and two use examples (tag "use examples": fingerprinting on a quiz, a staging site on localhost), each with a real screenshot, plus a Case Studies menu item (/tags/case-studies/). They are framed as made-up pages and state only what the pages and screenshots show.
- examples/case-studies/server.py (Python, stdlib only) serves the made-up sites; README.md there gives the hosts line. The site's screenshots were taken from it.
- softdevstudio.com lists Watchkeeper as the twenty-eighth project (data/studio.toml) with a note on 0.1.0 to 0.3.1; its CONTENT-PLAN.md has the brief, names, tag, footer, feeds row and session log row 2n.

## Next

Nothing approved beyond this. All five plan items are done. Waiting for YK.
