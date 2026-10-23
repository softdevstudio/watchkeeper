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

Security boundary: app commands get generated permissions (build.rs AppManifest) and capabilities/ui.json grants them only to the toolbar and panel webviews. The page webview is granted nothing. The detection script never calls into the app; the collector thread pulls the script's buffer with eval_with_callback every 400 ms under a random per-launch name, and reads the engine's cookie store (including HttpOnly cookies) every third round. Tauri still injects its IPC bridge into the page, so a site can tell it is in a Tauri shell; its calls are refused. Removing the bridge from the page is an open point.

crates/watchkeeper-core holds the detection script (assets/detect.js) and SiteReport. It has no Tauri dependency so Unweb can share it; it moves to its own repository when Unweb starts. A site is a registrable domain (psl crate); IP addresses and localhost are their own sites. The report resets when the page moves to another site, and caps events per site at 20,000.

The detection script records resource timing entries, fetch, XHR, sendBeacon and WebSocket URLs, cookie names written by script, storage keys (setItem, a sweep for direct assignment, indexedDB.open), and canvas, WebGL (unmasked renderer and the debug extension) and offline audio reads. Names only, never values. Sub-frames post their events to the top frame.

## Code and completion standard

Tests for every operation, plus failure tests aimed at the main promise: nothing the page does gets missed. The end-to-end self-test (watchkeeper --selftest, src/selftest.rs) runs the real app on the OS's own engine against a local site and a local tracker on another site; the tracker server's request log is the ground truth and every request in it must appear in the report, along with the cookies, storage writes and fingerprinting reads the tracker reports making. It also checks the layout before and after a resize and that the page cannot call app commands. Record results with scripts/record-results.sh, which replaces test/results/latest/<os>; git history keeps earlier runs. Every number on the site comes from recorded results with its conditions.

CI runs on every push to main on Linux, macOS and Windows: rustfmt, clippy with warnings denied, all tests, and the self-test (under Xvfb on Linux), uploading the results. Toolchain pinned to Rust 1.97.0. Before calling a release complete, get an independent review from a separate agent that has not seen the implementation, fix findings, rerun checks and record again.

Releases go out through an automated workflow after CI succeeds on the exact commit, building installers for Windows (.msi), macOS (.dmg) and Linux (AppImage and .deb). Early versions ship unsigned, with install instructions on the site; signing certificates are YK's later decision. Never ask YK to publish a release.

## Site standard

photozeitgeist/watchkeeper.net uses the unchanged Hugo Launch template: keep themes/ and archetypes/ byte for byte, content and configuration values only, no new layouts, CSS, _index.md files or public/ folder. Pages: about, contact (info@watchkeeper.net), quick start, download and reference. Posts: launch, a plain-language explainer of web tracking, how it was tested and the name. Post front matter: quoted title in normal title case with no special characters, date only, draft: false, tags; no description or categories; never repeat the title in the body; featured_image plus the same image inline after the second or third paragraph when a post has one. Images drawn for the project live in static/images. YK connects the domain later.

## Development plan

Agreed with YK on October 23, 2026, as the plan; YK approved session 1. Each later session needs YK's go-ahead. Pause and report after each session.

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

## Next

Nothing approved beyond session 1. Waiting for YK.
