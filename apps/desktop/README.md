# SafiData Desktop (Tauri 2.2)

The desktop shell loads the same React bundle as the website, but from the
Tauri asset origin. Account requests therefore need an explicit HTTPS origin;
the installed app never falls back to `http://localhost`.

## Build configuration

| Variable            | Required            | Purpose                                                            |
| ------------------- | ------------------- | ------------------------------------------------------------------ |
| `SAFI_API_BASE_URL` | Release builds, yes | Account-service origin, for example `https://api.safidata.example` |

Rules enforced by both the Rust build and the web client:

- `https://` only, with a multi-label DNS host.
- No path, trailing slash, query, fragment, credentials, wildcard, IP address,
  or `localhost`.
- The default port `443` is removed; any other port is preserved.

The origin is added only to the `connect-src` directive of the Tauri CSP and is
injected into the embedded bundle. The rest of the CSP is unchanged, and `https:`
is never opened as a wildcard.

## Development

```bash
# terminal 1: account API on 127.0.0.1:8080
cargo run -p sf-server

# terminal 2: web UI on 127.0.0.1:1420 (Vite proxies /v1 to the server)
pnpm -r --filter @safi/web run dev
```

Run the shell with `cargo run --manifest-path apps/desktop/src-tauri/Cargo.toml`.
In development the bundle uses the same-origin relative `/v1/auth` path, so
`SAFI_API_BASE_URL` is optional. When supplied, it is still validated.

## Release

```bash
SAFI_API_BASE_URL=https://api.safidata.example \
  cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release
```

A release build without a valid origin fails before the webview is packaged.
The same value must be present when the embedded web UI is built, because the
bundle and the CSP must agree on the origin.

## Server CORS

The server accepts account requests only from these origins:

- `tauri://localhost`
- `http://tauri.localhost`
- `https://tauri.localhost`
- `http://localhost:1420`
- `http://127.0.0.1:1420`

Only `GET`, `POST`, `DELETE`, and `OPTIONS` are allowed, with `Accept`,
`Authorization`, and `Content-Type`. Requests carry no cookies or credentials.

## Session handling

The session token stays in React memory for the lifetime of the page. It is not
written to local storage, IndexedDB, cookies, or application files, and it is
discarded when the window closes.
