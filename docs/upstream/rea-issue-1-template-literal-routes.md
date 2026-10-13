**Title:** [Bug] analyze_web_bundle misses routes declared with template literals (`` path:`x` ``)

**Template:** Bug report · Area: Evidence, artifacts, or reconstruction · Interface: CLI and MCP

---

### Before submitting

- [x] I searched the open and closed issues for a related report.
- [x] I removed credentials, tokens, private paths, and sensitive target data.

### Observed behavior

`analyze-web-bundle` (and the `analyze_web_bundle` MCP tool, which shares the
same analysis path) reports **1 route** for OWASP Juice Shop v20.3.0, whose
`main.js` declares **45** Angular routes, including `administration`,
`accounting`, `score-board` and `web3-sandbox`.

The bundle (Angular, built with rolldown) emits every route path as a template
literal without expressions:

```js
{path:`score-board`,component:…}
```

The only route reported is `/`, which comes from `scripts.js`, where the path
is an ordinary string literal.

### Reproduction

1. `docker run --rm -p 127.0.0.1:3011:3000 bkimminich/juice-shop:v20.3.0`
2. Start a Chrome-family browser with `--remote-debugging-port=9333` and open
   `http://127.0.0.1:3011/`
3. `rea list-browser-targets http://127.0.0.1:9333`
4. `rea analyze-web-bundle http://127.0.0.1:9333 <targetId> --observation-ms 3000 --format json`
5. `normalized_result.observations.routes` has one entry (`"/"`,
   `property:path`, from `scripts.js`).

Ground truth: `curl -s http://127.0.0.1:3011/main.js | grep -oE 'path:\`[^\`]*\`' | sort -u | wc -l` → 45.

### Expected behavior

Route properties whose value is a template literal with no expressions are
reported like string literals: the same `property:path` mechanism and the same
location. REA already treats such literals as exact values elsewhere
(`readExactJavaScriptLiteral` in `javascriptAstValues`).

### Impact and scope

- Affects route discovery for any bundle whose minifier or bundler emits
  template literals for plain strings (rolldown/esbuild-style output, as in
  Angular 20 builds). Hidden or admin-only routes are exactly the ones a
  reverse engineer wants, and they are the ones missed.
- `completeness.status` stays `partial` and the limitations mention
  string-built routes, but nothing tells the user that **plain constant
  routes** were skipped, so an agent or user reads "1 route" as close to the
  truth.
- No evidence-integrity or isolation impact.

### REA version

6.4.0 (`npx rea-agents@6.4.0`)

### Runtime and platform

macOS 26 (arm64), Node.js 26.7.0, Chrome 155.0.8059.40

### Target and artifact context

OWASP Juice Shop v20.3.0 (Docker image `bkimminich/juice-shop:v20.3.0`): a
public, deliberately vulnerable app run locally. `main.js` is 1,242,633 bytes.

### Interface

CLI `analyze-web-bundle`; the MCP `analyze_web_bundle` tool goes through the
same `BrowserObservationService.analyzeWebBundle` path.

### Earliest failure stage

Static inspection of captured scripts (route property extraction).

### Relevant output or evidence

```json
"routes": [{ "value": "/", "mechanism": "property:path",
  "location": { "script_key": "scr_8809…", "line": 1, "column": 2611 } }]
```

### Analysis or suspected cause

`inspectRouteProperty` in `src/domain/webBundleAnalyzerInspection.ts` only
accepts a string literal:

```ts
if ((key === "path" || key === "route") && t.isStringLiteral(node.value))
```

`stringValue` (from `javascriptStaticAnalysisHelpers`, already imported in that
file and used for `route(...)` call arguments) accepts expression-free template
literals through `readExactJavaScriptLiteral`. Using it here would fix this:

```ts
const value = stringValue(node.value);
if ((key === "path" || key === "route") && value !== undefined)
  addFinding({ collection: accumulator.routes, script, rawValue: value,
               mechanism: `property:${key}`, node, accumulator });
```

`objectString` in `webBundleAnalyzerAst.ts` has the same
`t.isStringLiteral` restriction and may deserve the same change.

A regression fixture: `const routes=[{path:\`admin\`},{path:"login"}];`
should yield two routes.

### Additional context

Found while evaluating REA as a capture source for a client-side attack
surface monitor. Happy to test a fix against the same build.

Filed as https://github.com/morluto/rea/issues/2805 on 2026-10-13.
