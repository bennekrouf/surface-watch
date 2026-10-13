**Title:** [Bug] analyze_web_bundle reports form fields and translation keys as `observed` endpoints, which agents then treat as API surface

**Template:** Bug report · Area: MCP or agent integration · Interface: MCP and CLI

---

### Before submitting

- [x] I searched the open and closed issues for a related report.
- [x] I removed credentials, tokens, private paths, and sensitive target data.

### Observed behavior

On OWASP Juice Shop v20.3.0, `analyze_web_bundle` returns **52 endpoint
candidates, and none of them is a URL or a path**:

| Values | Mechanism | What they actually are |
|---|---|---|
| `GET`, `POST`, `DELETE`, `HEAD`, `JSONP`, `OPTIONS`, `PATCH`, `PUT` | `call:request` | HTTP method names inside Angular's HttpClient |
| `TaskTrackingZone` | `call:Zone.current.get` | zone.js property lookup |
| `caption`, `image`, `passwordControl`, `initialTokenControl` | `call:form.get`, `call:t.form.get`, `call:twoFactorSetupForm.get`, `call:twoFactorDisableForm.get` | Angular `FormGroup.get(controlName)` |
| `addressId`, `entity`, `id` | `call:t.get` | route parameter / map lookups on a minified receiver |
| `redirectUrl` | `call:route.snapshot.queryParamMap.get` | query parameter name |
| `ADDRESS_ADDED`, `BASKET_ADD_PRODUCT`, `CHALLENGE_SOLVED`, … (20+) | `call:translate.get`, `call:translateService.get` | ngx-translate keys |
| `cookieconsent_status`, `welcomebanner_status`, `language`, `token`, … | `call:cookieService.get`, `call:cookieService.put` | cookie names |
| `Apply changes now`, `Force page reload`, `X` | `call:snackBar.open` | snackbar message and action text |
| `confirmBar`, `errorBar` | `call:snackBarHelperService.open` | snackbar style names |

At the same time, **none of the 42 real API paths** in `main.js`
(`/rest/user/login`, `/rest/admin/application-configuration`, `/api/Users`,
`/rest/chat`, …) is reported. Juice Shop builds them by concatenation:

```js
host=this.hostServer+`/rest/chat`;  …  this.http.post(this.host, …)
```

The concatenation part is covered by the documented limitation ("String-built
routes and endpoints … may remain unknown"). The false positives are not.

### Why this matters more through MCP

Each record carries `confidence: "observed"`. An agent using
`analyze_web_bundle` to map an app's API surface receives
`passwordControl` and `TaskTrackingZone` as observed endpoints, while every real
API path is missing. The agent cannot tell a URL from a form-control name
without re-deriving the analysis. In our test, a downstream consumer would have
built its whole endpoint list on values that are not endpoints.

### Reproduction

1. `docker run --rm -p 127.0.0.1:3011:3000 bkimminich/juice-shop:v20.3.0`
2. Open `http://127.0.0.1:3011/` in a Chrome-family browser with
   `--remote-debugging-port=9333`.
3. `rea analyze-web-bundle http://127.0.0.1:9333 <targetId> --observation-ms 3000 --format json`
   (or call the `analyze_web_bundle` MCP tool with the same target).
4. Read `normalized_result.observations.endpoints`.

### Expected behavior

1. **Precision.** Calls whose first argument is not URL-like should not become
   endpoint observations when the receiver is a known keyed collection or
   service. `endpointArgument` already excludes keyed-collection receivers for
   `get`/`delete` via `isKeyedCollectionReceiver`. Angular `FormGroup` /
   `AbstractControl`, ngx-translate `TranslateService`, `CookieService`,
   `Zone.current` and `MatSnackBar.open` are common enough to cover, or the
   rule could be tightened so `get`/`post`/`put`/`delete`/`request` only
   produce an endpoint when the argument is URL-like (`/…`, `http(s)://…`, a
   relative path with a slash). The `request` case is caught by HttpClient
   switching on method names.
2. **Recall, as an inference.** String literals that look like API paths
   (start with `/` and contain a path segment) and flow into a `+` or a
   template literal could be emitted as `inferences` with lower confidence,
   rather than not at all. That would have surfaced all 42 Juice Shop paths
   while keeping `observations` strict.

### Impact and scope

- Affects every Angular app (HttpClient, forms, ngx-translate are standard),
  and likely any app that uses `get(key)` APIs on non-HTTP objects.
- Evidence integrity: records are labelled `observed` for values that are not
  endpoints, so downstream consumers, especially agents over MCP, are misled.
- No process-isolation impact.

### REA version

6.4.0

### Runtime and platform

macOS 26 (arm64), Node.js 26.7.0, Chrome 155.0.8059.40

### Target and artifact context

OWASP Juice Shop v20.3.0 (`bkimminich/juice-shop:v20.3.0`), run locally.

### Interface

MCP `analyze_web_bundle`; also CLI `analyze-web-bundle` (same path).

### Earliest failure stage

Static inspection of captured scripts (`inspectCall` → `endpointArgument`).

### Analysis or suspected cause

`endpointArgument` in `src/domain/javascript/javascriptStaticAnalysisHelpers.ts`
returns the first string argument for any callee ending in
`.get`/`.post`/`.put`/`.patch`/`.delete`/`.request`, filtered only by
`isKeyedCollectionReceiver` for `get`/`delete`. `.open` calls go through
`xhrOpenUrl`, which appears to accept `MatSnackBar.open(message, action)`
arguments, judging by the values above.
Requiring a URL-like value (as `isRequestUrlLiteral` already checks in one
branch) for every method branch would remove most of the false positives
listed above.

### Additional context

Related: morluto/rea#2805 (routes declared as template literals are
skipped). Both were found on the same build while evaluating REA as a capture
source; I can test fixes against it.

Filed as https://github.com/morluto/rea/issues/2806 on 2026-10-13.
