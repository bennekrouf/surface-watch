# Changelog

What changed in each release of **SurfaceWatch**, the client-side attack
surface monitor for web pentest engagements, and of its command-line edition,
`surfacewatch-cli` (called Surface Watch and `surfacewatch` before 0.1.16).

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Use from Claude**: SurfaceWatch can now be connected to Claude Desktop,
  Claude Code, Codex or any other MCP app, from the button at the top right.
  Ask about your engagements: what changed between two builds, which endpoints
  are flagged, which findings are still open, and the evidence behind them.
  Claude only reads: captures, retests and findings are still made in the app.
  On a machine with only the command line, `surfacewatch-cli mcp` serves the
  same tools.

## [0.1.20] - 2026-10-28

### Added

- SurfaceWatch now shows the occasional message from mayorana.ch, under the bar
  at the top, until you close it.

### Changed

- The update banner and the occasional message from mayorana.ch now sit just
  under the bar at the top of the window instead of covering it, and look the
  same in every mayorana app. Download opens the new build in your browser and
  shows its SHA-256 when one is published, to compare with the file you
  downloaded.
- The Pro window looks and reads the same in every mayorana app. Once you're Pro
  it names your plan and email, and pressing Enter in the licence number field
  activates it.

## [0.1.19] - 2026-10-28

### Changed

- The bar across the top of the window now looks and works the same in every
  mayorana app: the name and version on the left, and on the right Feedback, the
  light/dark switch and the Pro badge in the corner.
- The name, version, Feedback and Pro moved there from the top of the
  engagements list, and the light/dark switch is part of the bar instead of
  floating over it.

## [0.1.18] - 2026-10-28

### Changed

- Setting `DISABLE_UPDATE_CHECK` now turns the update check off, as it does
  in the other mayorana apps.
- To try a staging `latest.json`, set `MAYORANA_UPDATE_URL` (it was
  `SURFACE_WATCH_UPDATE_URL`).

## [0.1.17] - 2026-10-27

### Changed

- Build and packaging only — no user-visible change.

## [0.1.16] - 2026-10-25

### Changed

- Surface Watch is now called SurfaceWatch, written as one word like the other
  Mayorana apps. Your engagements, captures and licence carry over: they stay
  in the same folder. On macOS the update installs as SurfaceWatch.app; once it
  runs, you can delete Surface Watch.app from Applications. On Windows the
  update replaces the old version and its Start menu entry.
- The command line is now `surfacewatch-cli` (`surfacewatch-cli.exe` on
  Windows), so that the desktop app can be `surfacewatch`, as GitAgent is
  `gitagent`. Scripts and cron jobs that run `surfacewatch` need the new name;
  the commands and their options are unchanged.

## [0.1.15] - 2026-10-25

### Changed

- Activating a licence now shows a small spinner while it is checked, then
  a window that says you're Pro, with the AI credits left and the renewal
  date in small type below. Removing the licence and checking it again no
  longer sit beside it; Manage subscription is a small link. Press Esc or
  the × to close it.

### Fixed

- Installing silently, as IT does for an unattended update with `/VERYSILENT`,
  no longer hangs when Surface Watch is already installed: setup waited on a
  question nobody could see. It now replaces the installed version without
  asking.

## [0.1.14] - 2026-10-23

### Changed

- The new-version banner and the usage-statistics notice now look the same
  as in GitAgent: a blue bar across the top, or the bottom for the notice,
  with a Download link and a × to hide it.

## [0.1.13] - 2026-10-23

### Changed

- Dialogs now have a title bar with a × to close them, and text fields have
  the same size and typeface as in GitAgent.
- An open dialog now dims the light/dark switch in the corner along with the
  rest of the window, instead of leaving it lit on top.
- The buttons along the top of the window now all have the same height,
  borders included, and the light/dark switch matches them.

## [0.1.12] - 2026-10-23

### Changed

- Buttons now have the same size, rounded corners and typeface as in
  GitAgent, so they look alike across the Mayorana apps.

### Fixed

- The PRO button at the top of the sidebar is no longer cut off: Feedback
  and PRO move to their own line under the app name.

## [0.1.11] - 2026-10-22

### Added

- The window title now shows your licence status: it reads Pro when Pro is
  active, and Pro inactive when Pro has lapsed or could not be checked. The
  title updates as soon as the licence changes, so you can see your status
  without opening the Pro window.

## [0.1.10] - 2026-10-22

### Changed

- One licence number for every Mayorana app on this computer. Paste it in any
  of them and the others are Pro too the next time they start, with nothing to
  paste again; a number already pasted in one app is shared the same way.
  Removing the licence in the Pro window removes it from all of them.

## [0.1.9] - 2026-10-21

### Added

- Free AI credits every month, without Pro. Enter your email under Get free
  AI credits in the Pro window and the licence number arrives by email; paste
  it in the same window. The window then shows how many credits are left this
  month. The free version's limits stay as they are. With the command-line
  edition, run `surfacewatch licence --free <email>`.

## [0.1.8] - 2026-10-21

### Changed

- Surface Watch Pro now comes with a monthly plan on mayorana.ch instead of a
  one-time licence key. Paste the licence number from the plan's email (like
  ABCDE-FGHJK-LMNPQ-RSTUV) in the Pro window, or run
  `surfacewatch licence <number>`. Surface Watch checks it when it starts and
  once a day, and Pro keeps working for a week without a connection.
- The Pro window shows your plan, the AI credits left this month and when the
  plan renews. Manage subscription opens the page where you change or cancel
  it. `surfacewatch licence` shows the same.

## [0.1.7] - 2026-10-20

### Added

- A Feedback link beside the version number opens the contact form with
  Surface Watch and your version already filled in, so an idea or a problem
  reaches us in two clicks.

## [0.1.6] - 2026-10-19

### Added

- Surface Watch now includes a script to check for dynamic C runtime
  dependencies in Windows binaries, ensuring better compatibility across
  different systems.

## [0.1.5] - 2026-10-16

### Added

- Surface Watch Pro. The free version keeps one engagement, with every tab,
  the command line and scheduled watches; Pro keeps any number, and is the
  licence for commercial use. Engagements already on your computer stay usable
  either way. Buy it on [Surface Watch's page](https://mayorana.ch/en/apps/surface-watch),
  then click **Get Pro…** next to the app's name, or run
  `surfacewatch licence <key>`. The key is checked on your computer: no
  account, nothing sent anywhere.

### Fixed

- On Windows and Linux, the Surface Watch window now shows its icon in the
  title bar, the taskbar and the app switcher, instead of a blank or generic
  one.

## [0.1.4] - 2026-10-16

### Added

- A sun/moon button at the top right switches between light and dark. Surface
  Watch still starts in the same mode as your computer and follows it when it
  changes, until you pick one with the button.

## [0.1.3] - 2026-10-16

### Added

- **Export to api0** on the Surface tab, or `surfacewatch export-api0`, writes
  the API calls a frontend was seen making as an api0 catalog, ready to turn
  into MCP tools: one group per target, with each endpoint's method, path,
  path parameters, query parameters and request body fields. It combines the
  latest crawl with every HAR import, and leaves out static files, websockets
  and calls that come and go between runs. api0's identity headers stay off
  unless you tick **My own backend**. A warning reminds you that
  `api0 api upload` replaces every API in the workspace it uploads to.

### Changed

- **Import HAR** now keeps the names of the fields a request body sent (for
  example `email` and `password` on a login), never their values. Cookies,
  header values, body values and query values are still not kept. HAR files
  imported before this version have no field names; import them again to add
  them.

## [0.1.2] - 2026-10-14

### Added

- **Suggest pages** in the Watch tab finds the pages a target's own code
  declares, including hidden ones no link points to, such as an admin or
  accounting area. It opens the target once in a background browser, lists
  what it found, and adds the pages you tick to the crawl. Pages that need a
  value, like an order ID, are flagged instead of added. Also available as
  `surfacewatch suggest-pages`. With REA 6.4.0 it finds only a few routes;
  the next REA release finds them all.

## [0.1.1] - 2026-10-13

### Added

- Downloads for macOS (Apple Silicon, signed and notarized), Windows
  (installer) and Linux (x86_64 tarball with a setup script), plus the
  `surfacewatch` command line for each.
- When a newer version is out, a banner at the top of the window says so,
  with a **Download** button for your system and the published SHA-256 so you
  can check the file. Set `DISABLE_UPDATE_CHECK` to turn the check off.
- The version you are running is shown next to the app's name, top left.
- Surface Watch is free for personal, research and non-profit use under the
  PolyForm Noncommercial licence; commercial use, including client work,
  needs a licence from Mayorana. The Windows installer shows the licence
  before installing.
- First version. `surfacewatch` captures a web target through REA, crawling
  its pages in small batches, and shows its attack surface: endpoints with
  their parameters, statuses and security headers, plus the third parties the
  page talks to.
- `surfacewatch diff` compares two captures and lists what actually changed:
  new and removed endpoints, changed parameters, statuses and headers, and
  third-party origins. Renamed script chunks, images and timing differences no
  longer show up as changes. Give it a second capture of the same build with
  `--repeat` and endpoints that come and go between runs are set aside and
  listed as unstable.
- The **Watch** tab captures the selected target with **Capture now** and
  compares any two of its captures, by default the first one (the baseline)
  against the latest. Changes are grouped by kind, each with what it means for
  your testing: new endpoints to test, removed ones to check directly, new
  parameters to try for injection. **Stability check** captures the target a
  second time so endpoints that come and go between runs are set aside instead
  of showing up as changes. Captures are refused outside the engagement's test
  window.
- The **Surface** tab shows what a capture saw of a target: every endpoint
  with its statuses and parameters, the scripts, and the third parties the
  page loads from. Entries worth a look are flagged and listed first: server
  errors, admin or debug paths that answer without a login, object IDs in the
  path to test for access control, and pages served without a
  Content-Security-Policy. Filter by path or show flagged entries only, and
  pick an older capture to see what it held. **Promote** turns any entry into
  a finding with a retest check, so issues that were there from the start no
  longer need a change to be recorded.
- **New engagement** in the sidebar opens a form for the client, the test
  window, who authorised the test, and the first target: its origin, the
  pages to visit, and crawl settings. **+ Add target** adds more targets to an
  engagement. The origin is checked as you type and shows exactly what is in
  scope; paths go in the pages list. An authorisation note is required, and
  the same rules apply to `surfacewatch engagement add`, which now needs
  `--auth`.
- The **Captures** tab lists every capture of a target, newest first: when
  it was taken, whether it is a crawl, a stability repeat or a HAR import,
  how many endpoints and flagged entries it holds, the tools and versions
  behind it, its evidence, and which findings rely on it. **Delete** removes
  a capture and its stability repeats after you confirm, and says which
  findings will lose the raw data behind their evidence.
- **Import HAR** in the Watch tab, or `surfacewatch import-har`, adds a
  recording from your browser's DevTools, Caido, ZAP or mitmproxy as a
  capture. It covers what the crawl cannot reach: logged-in pages, forms, a
  chat message sent. Cookies, header values, request and response bodies and
  query values are not kept; the capture cites the original file by its
  SHA-256, and the file stays where it is. The capture is dated by when it was
  recorded.
- Comparisons stay crawl against crawl by default. Pick a HAR to compare it,
  and a note warns that a HAR and a crawl cover different ground. Retests use
  the newest capture of each kind that covers the finding, so a HAR of one
  flow does not turn a finding into "unknown".
- **Promote to finding** on each change in the Watch tab turns it into a
  finding that cites the REA evidence of both captures.
- The **Findings** tab lists an engagement's findings by severity. Edit the
  title, severity, description, remediation and CVSS vector in place; every
  change is saved as you go. **Retest** checks a finding against the target's
  latest capture and records the result with what the capture showed: fixed,
  still present, or unknown when the crawl did not reach it. An endpoint the
  crawl no longer sees is never reported as fixed, because it may still answer
  on the server. Findings no capture can confirm get their status set by hand.
- **Export Markdown** writes a report of the engagement's findings, with their
  evidence, the tool versions behind each capture, and the retest history.
  `surfacewatch findings <engagement> --retest --export` does the same from
  the terminal or a scheduled job.
- `surfacewatch engagement add`, `target add`, `import` and `watch` create
  engagements and targets, bring in existing REA captures, and capture a
  target and compare it with its baseline from the terminal or a scheduled
  job.
- The Readiness tab checks Node.js, REA and your browser, and lists the known
  REA issues with the workaround applied for each. The app opens on it until
  you have a target to watch.
