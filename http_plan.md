# Plan: Network TLE Retrieval (`--tle-url`) and CelesTrak Usage

This document states the engineering approach for adding **live HTTP retrieval** of
Two-Line Element (TLE) data to the Ephemerust CLI, the **operational constraints** imposed by
public data providers (principally CelesTrak), and the **acceptance criteria** under which the
feature is considered fit for release. It is written for maintainers and reviewers; it does
not constitute legal advice.

---

## 1. Purpose and scope

The Ephemerust binary exposes a Cargo feature, **`network`**, which enables the `track`
subcommand flag **`--tle-url`**. At present, that path returns a deliberate stub error. The
objective of the work described herein is to replace the stub with a **bounded, cache-aware
HTTPS client** that retrieves element-set text from a user-supplied URL and feeds it into the
existing **`Tle::parse`** pipeline, without altering default builds that omit the `network`
feature.

Out of scope for the initial delivery unless explicitly added later:

- Authenticated access to [Space-Track](https://www.space-track.org/) (account credentials,
  token refresh, and redistribution obligations differ from anonymous CelesTrak file fetch).
- A background daemon or scheduled poller inside Ephemerust (the CLI performs **on-demand**
  fetch only; caching policy is documented for integrators who wrap the tool).

---

## 2. Data source: CelesTrak over HTTPS

### 2.1 Intended use

CelesTrak publishes NORAD GP / legacy TLE products over **HTTPS** for use by the satellite
community, including application software. The project shall treat **`https://celestrak.org/`**
as the **canonical origin** for public element-set downloads referenced in documentation and
examples.

### 2.2 Domain and redirects

CelesTrak migrated the primary site to **`celestrak.org`**. Requests to legacy **`celestrak.com`**
hosts may receive **HTTP 301** responses. The client implementation **must** follow redirects
(or prefer `.org` URLs exclusively in user-facing examples). Failure to handle **301** has been
observed in the wild to cause automated clients to retry rapidly and trigger **rate-based
blocking**; the implementation shall not enter a tight retry loop on non-success status codes.

### 2.3 Operational expectations (provider documentation)

CelesTrak’s published guidance (including the GP / data-format and operational FAQ material)
states, in substance, the following expectations for automated clients:

| Topic | Required behavior |
|--------|-------------------|
| **Polling interval** | Public GP data updates on the order of **approximately two hours**; clients **should not** re-fetch the same resource more frequently than the data refresh cycle without cause. |
| **Caching** | Clients **should** retain the last successfully retrieved file and compare freshness (e.g., age or provider metadata) before issuing a new download. |
| **Error handling** | On **403**, **404**, **301** (when misconfigured), or **5xx**, the client **must not** assume that repeating the request will succeed; repeated errors contribute to **IP-level firewalling** documented by the provider. |
| **Request scope** | Clients **should** request **only the smallest group or resource** required (e.g., stations bulletin for ISS) rather than enumerating every hosted group. |
| **Identification** | The HTTP client **should** send a descriptive **`User-Agent`** string identifying Ephemerust and a contact or repository URL. |
| **Bandwidth** | The provider has documented **bandwidth and per-update download policies** for high-traffic catalogs; the implementation shall document that end users who script aggressive downloads may be blocked regardless of Ephemerust correctness. |

The engineering team shall treat these items as **non-functional requirements** alongside
correctness of parsing and propagation.

### 2.4 Redistribution and lineage

CelesTrak’s element sets derive from **U.S. Government** space-surveillance redistribution
arrangements summarized in CelesTrak’s own system notices. Ephemerust **does not** re-host
CelesTrak files; it **fetches on behalf of the end user** for local use. Any future feature that
**mirrors or republishes** bulk element sets shall undergo a separate compliance review; it is
outside the initial `--tle-url` scope.

---

## 3. Technical design

### 3.1 Dependency gating

The HTTP/TLS stack **shall** be linked **only** when the `network` feature is enabled (e.g.,
optional **`reqwest`** with **`blocking`** and **`rustls-tls`**, or an equivalent minimal
blocking client). Default **`cargo build`** / **`cargo install ephemerust`** without features
**shall not** pull TLS and HTTP dependencies.

### 3.2 Request path

When `network` is enabled and `--tle-url` is the sole TLE source:

1. Validate the URL scheme (**HTTPS only** for the default policy; `file:` and `http:` **may**
   be rejected explicitly to reduce attack surface).
2. Perform a **GET** with a **connection and overall timeout**, a **maximum response body
   size**, and redirect handling consistent with §2.2.
3. Decode the body as **UTF-8**; on failure, return a structured error suitable for CLI display.
4. Pass the resulting string to **`Tle::parse`** (or a thin preprocessor if the response is not
   a single element set — see §3.3).

### 3.3 Multi-object responses

Many CelesTrak URLs return **concatenated** 2-line or 3-line sets (e.g., the full stations
bulletin). **`Tle::parse`** expects **one** object. The plan therefore requires **one** of the
following before release:

- **Additional CLI parameters** (e.g., substring match on the name line, or NORAD catalog ID),
  **or**
- **Documentation** that restricts `--tle-url` to URLs returning **exactly one** element set.

The engineering team shall select the option that minimizes user error and support burden.

### 3.4 Security notes

The URL is user-controlled (SSRF considerations). The initial implementation **may** restrict
schemes and **may** optionally block RFC1918 targets; at minimum, the document shall warn
operators that **`--tle-url` is powerful** and should not be pointed at untrusted internal
services without review.

---

## 4. Testing strategy

| Layer | Approach |
|-------|-----------|
| **Unit / component** | Mock HTTP server (e.g., `wiremock` or equivalent) returns a fixed TLE body; asserts successful parse and error paths for timeout and 403. |
| **Integration** | Existing `network` feature tests shall be updated: stub message removed; success path uses mock base URL or local fixture server. |
| **Live / manual** | Optional `#[ignore]` or environment-gated test that performs a single HTTPS GET against CelesTrak **shall not** run in default CI; if enabled, it **must** respect caching guidance. |

Default **`cargo test`** **shall remain** deterministic and **offline**.

---

## 5. Documentation and release artifacts

The following shall be updated before the feature is advertised as non-experimental:

- **`readme.md`** — how to build with `--features network`, example CelesTrak URL, caching
  reminder, and pointer to **http_plan.md**.
- **`CHANGELOG.md`** — user-visible addition under `[Unreleased]` or the next version.
- **`docs/architecture.md`** — dependency row for the optional HTTP client when `network` is
  enabled.
- **`docs/satellite-tracking-plan.md`** or successor milestone note — closure of the
  `--tle-url` stub item.

---

## 6. Risks and dependencies

| Risk | Mitigation |
|------|------------|
| **CelesTrak policy or endpoint change** | Pin examples to stable query patterns; monitor CelesTrak notices; degrade gracefully with clear errors. |
| **Client IP blocking due to user misuse** | Document rate and caching guidance prominently; avoid internal polling loops. |
| **TLS / MSRV drift** | Re-validate `rust-version` when upgrading the HTTP stack. |
| **Format drift (GP vs TLE)** | Parser or preprocessor limited to TLE/3LE text; document GP JSON/CSV as out of scope unless explicitly added. |

---

## 7. Acceptance criteria

The `--tle-url` implementation **shall** be considered complete when:

1. With **`--features network`**, a **GET** of a documented CelesTrak HTTPS resource yields a
   parsed **`Tle`** and successful `track` output for a representative case (e.g., ISS from the
   stations bulletin when selection logic from §3.3 is applied).
2. **HTTPS**, **timeouts**, **body size limits**, and **non-success HTTP status** paths return
   actionable errors and **do not** tight-loop retries.
3. **Default** builds without `network` **do not** register `--tle-url` on the CLI (existing
   behavior preserved).
4. **Automated tests** cover success and failure without requiring network access in the
   default configuration.
5. **Documentation** reflects CelesTrak usage expectations summarized in §2.3.

---

## 8. Revision history

| Revision | Summary |
|----------|---------|
| 1.0 | Initial plan: CelesTrak operational requirements and technical scope for `--tle-url`. |
| 1.1 | Renamed document from `plan.md` to `http_plan.md`. |
