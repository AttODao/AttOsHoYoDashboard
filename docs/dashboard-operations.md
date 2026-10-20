# Dashboard operations

This document is the deployment and recovery contract for the Dashboard web session service. The user has authorized real HoYoLAB access and manual and scheduled reward receipt for hobby use. These are normal features when `HOYOLAB_COOKIE_KEY` encryption is configured and existing link verification succeeds; automatic rewards default OFF and users enable them through the existing UI.

## Before serving

1. Provision a dedicated PostgreSQL database.
2. Generate `HOYOLAB_COOKIE_KEY` outside the repository as 32 random bytes encoded as 64 hex characters. Keep it in secret storage, not in repository files, shell history, logs, or commits.
3. Configure `DATABASE_URL`, `PUBLIC_ORIGIN`, optional `RP_ID`, `BIND_ADDR`, `SESSION_TTL_DAYS`, `TIMEZONE_OFFSET`, `HOYOLAB_COOKIE_KEY`, and optional `DASHBOARD_UI_DIR` in the process environment or private deployment secret store.
4. Build the Dioxus Web UI with `wasm32-unknown-unknown` and a compatible `dx` 0.7.10 CLI. Install via `cargo install dioxus-cli --version 0.7.10 --locked` or use an official matching prebuilt verified against upstream checksums.
5. Run migrations before starting the server:

```console
rustup target add wasm32-unknown-unknown
dx build --package attos-dashboard-ui --platform web --release
cargo run -- migrate
cargo run -- serve
```

For first setup, create the initial administrator only while the `users` table is empty. Pass the temporary password on standard input so it is not visible in process arguments:

```bash
read -r -s -p 'Temporary password: ' temporary_password
printf '\n'
printf '%s' "$temporary_password" | cargo run -- bootstrap-admin admin
unset temporary_password
```

The default `DASHBOARD_UI_DIR` is `target/dx/attos-dashboard-ui/release/web/public`. Deployment should copy or mount only that public directory; do not commit generated bundles or expose the whole `target/dx` tree/source directory. Startup fails if `DASHBOARD_UI_DIR/index.html` is absent.

## Public origin, TLS, and cookies

- Use an HTTPS reverse proxy for real deployment. `PUBLIC_ORIGIN` must be the exact browser origin and must match state-changing request `Origin` headers.
- WebAuthn requires a DNS RP ID. `http://localhost` is allowed for local development; production should use HTTPS DNS names, not IP literals.
- HTTPS deployments use the `__Host-attos_session` HttpOnly SameSite=Strict Secure cookie. Local non-secure localhost uses `attos_session` without the `__Host-` prefix.
- Sessions have a fixed configured lifetime, default 7 days. Access does not extend them; password rotation revokes other sessions and preserves the current session deadline.
- The Dioxus browser client uses same-origin credentials and must not store secrets in localStorage or set Cookie/Origin headers manually.
- The Dioxus HTML shell is served with CSP for DX 0.7.10's external module loader, using `script-src 'self' 'wasm-unsafe-eval'` without `'unsafe-inline'` or general `'unsafe-eval'`; `style-src 'self'`, `connect-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, and `base-uri 'none'` remain required. The release bundle/CSP has passed a Chromium check against local synthetic API fixtures.

## Secrets, backups, and key changes

- HoYoLAB Cookie plaintext is never returned by API responses. Stored credentials are AES-256-GCM sealed with AAD including owner and verified UID context.
- Back up PostgreSQL and the cookie key consistently. Restoring the database without the matching key, or changing AAD/key material, fails closed because stored cookies cannot be opened.
- Rotate the key offline: stop the server, back up the database, then run `attoshoyodashboard rekey` with the current key in `HOYOLAB_COOKIE_KEY` and the new one in `HOYOLAB_COOKIE_KEY_NEW` (for Compose: `docker compose run --rm -e HOYOLAB_COOKIE_KEY_NEW=… app rekey`). Every stored Cookie is re-encrypted in one transaction; any row the current key cannot open aborts the rotation unchanged. Then set `HOYOLAB_COOKIE_KEY` to the new key and start the server. Decoy passkey-login credential IDs derive from this key and change with it.
- Users can remove their stored Cookie from the HoYoLAB link page; the ciphertext is deleted and the link waits for a Cookie from the same HoYoLAB account.
- Do not read production `.env` or dump secrets for troubleshooting. Use sanitized error codes and local synthetic tests.

## Logging, health, and metrics

- `RUST_LOG` selects log levels (default `info`, e.g. `info,attoshoyodashboard=debug`); `LOG_FORMAT=json` writes one JSON object per line. Logs never include Cookie values, credentials, request payloads, or raw upstream errors.
- `GET /health` returns `{"status":"ok","worker":"ok"}`. `worker` is `disabled` without `HOYOLAB_COOKIE_KEY`; if the reward worker has not ticked for 120 s the response is 503 with `{"status":"degraded","worker":"stalled"}`.
- `GET /metrics` exists only when `METRICS_TOKEN` (16+ visible ASCII characters) is set, and requires `Authorization: Bearer <METRICS_TOKEN>`. It reports user, link, reward-run and reward-setting counts and the worker heartbeat age in Prometheus text format.
- Schedules use `TIMEZONE` (IANA name, DST aware; a time skipped by DST runs at the first minute after the gap, a repeated time at its first occurrence) when set, otherwise the fixed `TIMEZONE_OFFSET`.
- Tuning: `REWARD_HISTORY_DAYS` (1–90, default 5), `STATUS_CACHE_TTL_MINUTES` (1–60, default 5), `REWARD_MAX_ATTEMPTS` (1–4, default 4), `REWARD_RETRY_DELAY_SECONDS` (1–600, default 10).

## Availability and shutdown

- The Dashboard is a single-process, single-instance service. There is no HA or distributed job queue; deployment downtime during update is accepted.
- SIGINT/SIGTERM stop new operations, mark HoYoLAB work as closing, and drain bounded in-flight work/confirmation before the process exits.
- Disabling a user blocks new API access/jobs and revokes sessions. Re-enabling does not revive old sessions and turns automatic rewards off.

## HoYoLAB availability and evidence limits

- Normal integration and manual/scheduled rewards retain fresh account/community identity, Cookie-claim and roles consistency checks, same-UID update checks, verified-link requirements, authentication, authorization, and account lifecycle checks. Cookie UID alone is not ownership proof.
- Owner serialization, transaction/row locks, durable may-have-sent recording **before** POST, and daily GET `is_sign` confirmation remain required. An uncertain POST is never resent; use GET-only confirmation and otherwise retain unknown status. Do not clear guards to force another POST.
- User-approved availability is not scientific proof of live token-to-UID binding, roles principal consistency, or the full reward contract. Only one real account is available; synthetic multiple identities cannot establish live binding.
- The user reported the current read-only GET suite passed (1 passed, 0 failed, 0 ignored; 3.72s). It checks accounts and both games' daily_info/daily_home/index/daily_note access, not account_identity/community_identity or reward POSTs. No live POST result has been reported. Additional evidence collection must stay within its approved scope; do not log Cookie values, credentials, payloads, or raw upstream errors.

## Verification commands

Use commands appropriate to the change under review, for example:

```console
cargo fmt --all -- --check
cargo check --locked
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=2
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo check --locked -p attos-dashboard-ui --target wasm32-unknown-unknown
cargo clippy --locked -p attos-dashboard-ui --target wasm32-unknown-unknown -- -D warnings
dx build --package attos-dashboard-ui --platform web --release
```

The final local workspace suite passed **181 tests, one ignored opt-in live test**; native/WASM strict Clippy, backend build, and DX release build passed. Run the compiled UI check with `node dashboard-ui/tests/browser.mjs` after the release build, with Playwright and Chromium installed. If reusing a tool installed outside the repository, set `ATTOS_TEST_PLAYWRIGHT_MODULE` to its module file (for example `/tmp/ui-check/node_modules/playwright/index.js`); missing browser/runtime dependencies fail the check rather than skip it. This fixture tests real browser cookies, serialized auth workflows, failed logout/retry/reload, virtual WebAuthn registration/assertion, ignored unused mutation success bodies with authoritative typed reloads, UID filtering, exactly one public-docs OpenAPI GET without authenticated API calls, mobile docs, and the actual release CSP. Replies are synthetic on implemented API paths, not evidence of backend authorization, physical user verification, or upstream contracts.

Dashboard DB tests require local PostgreSQL tools and isolated temporary clusters. If PostgreSQL tools are outside PATH, set `ATTOS_TEST_PG_BIN` to the directory that contains `initdb` and `pg_ctl`. Browser WebAuthn with a real authenticator, production TLS/reverse-proxy behavior, physical deployment paths, and live upstream contracts remain unverified. They must not be inferred from the synthetic checks.
