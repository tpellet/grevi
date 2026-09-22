# Security Policy

## Supported Use

`davinci-resolve-mcp` is a local stdio MCP server for controlling DaVinci
Resolve Studio through the official Resolve Scripting API. It is intended to run
under the same local user account that operates Resolve.

The default stdio server does not expose a network listener, remote shell, or
multi-user authentication surface. Access control is delegated to the MCP client
that launches the stdio process and to the local operating-system user session.

Two opt-in surfaces DO open local HTTP listeners, and both are hardened the same
way:

- **The control panel** (`resolve_control action=open_control_panel`,
  `python -m src.control_panel`) — a single-user browser UI on
  `127.0.0.1:8765` by default.
- **The networked MCP transport** (`--transport sse|streamable-http`) — a
  second MCP instance for remote clients, on `127.0.0.1:8000` by default.

Their posture:

- **Loopback only.** The panel refuses any bind host other than
  `127.0.0.1` / `localhost` / `::1` — the bind address is not a tool parameter
  an AI can widen. The transport defaults to loopback and logs a loud warning
  if `DAVINCI_MCP_HOST` points elsewhere. On a non-loopback bind its
  DNS-rebinding allowlist is the bind host plus loopback, extended by
  `DAVINCI_MCP_ALLOWED_HOSTS` (comma-separated names clients will use); on a
  wildcard bind (`0.0.0.0` / `::`) with that variable unset the Host check is
  off and the bearer token is the only gate, and the log says so.
- **Bearer token on every request.** Each panel launch generates a fresh
  `secrets.token_urlsafe(32)` token, passed to the child via environment (not
  argv) and delivered to the browser in the URL fragment (`#token=…`), which
  never reaches the server or its logs. Every route except the static shell at
  `/` returns 401 without it (`Authorization: Bearer …`, or the HttpOnly,
  `SameSite=Strict` session cookie the panel exchanges it for so image loads
  work). The transport requires `Authorization: Bearer <token>` on every request.
- **DNS-rebinding and CSRF guards.** The panel rejects any request whose
  `Host` header is not a loopback host, any request carrying a non-loopback
  `Origin`, and any `POST` that is not `Content-Type: application/json`. It
  never answers a CORS preflight, so no third-party page can call it.
- **Secrets on disk are private.** The panel's pidfile (token + pid + URL) and
  the transport's state file (token + URL) live under
  `~/.davinci-resolve-mcp/` (0700) and are written 0600 — never in a shared
  temp directory or `~/Documents`. Those are the only on-disk copies: neither
  token is ever written to `logs/server.log`, which the server appends to with
  the default file mode and never clears. The transport logs the state file's
  path, not the token, and echoes a generated token only to an interactive
  stderr.

If you find a route that can be reached without the token, or a way to satisfy
the Host/Origin checks from a non-loopback page, that is a security bug — please
report it (see below).
