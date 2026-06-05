# LLM Proxy Architecture Spec

## Goal

Build a Rust single-binary LLM proxy for OpenAI-compatible APIs that records usage, latency, generation speed, and full request/response payload archives. The proxy is primarily a monitoring tool, not a policy enforcement or billing system.

The binary must embed all web UI assets and templates. Runtime state lives outside the binary in a JSON config file, a SQLite operational database, and an encrypted/compressed payload archive directory.

## Supported API Surface

V1 supports OpenAI-compatible pass-through endpoints:

- `POST /v1/chat/completions`
- `POST /v1/responses`
- `GET /v1/models`

Streaming must be supported for completions and responses. The proxy streams upstream chunks to the client immediately while teeing data into metrics collection and payload archival.

`/v1/models` returns models from the default route's `/v1/models` plus explicitly configured model names from the JSON config. It requires a valid proxy API key.

## Process Layout

The binary runs two HTTP servers:

- Proxy server: defaults to `0.0.0.0:8080`
- Admin server and dashboard: defaults to `0.0.0.0:8081`

The default upstream route points to LM Studio:

- `http://localhost:1234`

TLS is out of scope for v1. Deployments that expose the admin or proxy ports beyond trusted networks should place the service behind a reverse proxy, tunnel, or VPN.

## Configuration

Configuration is JSON and is treated as deployment/configuration data. It is not edited by the dashboard except for triggering validation/reload.

Config discovery order:

1. Explicit `--config /path/to/config.json`
2. Existing `config.json` beside the executable
3. Existing `~/.config/llm-proxy/config.json`
4. Create `config.json` beside the executable if possible
5. Otherwise create `~/.config/llm-proxy/config.json`

If no config exists, first startup creates a usable default config non-interactively, creates missing data directories, initializes the database, and starts the service.
Relative filesystem paths inside the config are resolved from the directory containing the selected config file.

Example config:

```json
{
  "proxy_listen": "0.0.0.0:8080",
  "admin_listen": "0.0.0.0:8081",
  "database": "./data/llm-proxy.sqlite",
  "payload_dir": "./data/payloads",
  "master_key": "./data/master.key",
  "default_route": "local",
  "routes": {
    "local": {
      "base_url": "http://localhost:1234",
      "upstream_api_key": null
    },
    "openai": {
      "base_url": "https://api.openai.com",
      "upstream_api_key": "openai-prod"
    }
  },
  "models": {
    "gpt-5.5": {
      "route": "openai"
    },
    "fast-local": {
      "route": "local",
      "upstream_model": "llama-3.1-8b-instruct"
    }
  },
  "payload_capture": {
    "default_enabled": true,
    "compression": "zstd"
  },
  "logging": {
    "format": "json",
    "level": "info"
  }
}
```

Config validation is strict. On reload, reject the entire new config if it contains invalid listen addresses, invalid URLs, missing referenced routes, missing default route, malformed model mappings, or references to nonexistent upstream secret names. Keep the last valid runtime config active.

Config can be reloaded with `SIGHUP` and through an authenticated admin action.

## Routing

Route selection:

1. Read requested model from the OpenAI-compatible request body.
2. If the model exists in `models`, use its configured route.
3. Otherwise use `default_route`.
4. If `upstream_model` is configured, rewrite the model sent upstream.
5. Unknown models are forwarded unchanged to the default route.

Routing decisions are recorded in the operational database, including whether the match was explicit or fallback.

Upstream API keys are not stored in config or environment variables. Config references upstream secrets by logical name. Secret values are created and managed in the admin UI and stored encrypted in SQLite.

## Authentication

### Proxy API Keys

All `/v1/*` endpoints require proxy-owned API keys:

```http
Authorization: Bearer <proxy_api_key>
```

Proxy API keys are created in the dashboard, shown once, and stored hashed in SQLite. There are no per-key permissions, quotas, or model restrictions in v1.

### Admin Auth

The dashboard uses a single admin account.

First run with no admin account:

- Generate a bootstrap setup token.
- Print a setup URL/token to stdout.
- Require that token to set the admin password.

Admin sessions use `HttpOnly`, `SameSite=Lax` cookies backed by hashed session records in SQLite. Use `Secure` cookies only when served over HTTPS by a deployment layer.

Passwords are hashed with Argon2id.

## Secret Storage

Upstream API keys are encrypted at rest in SQLite.

Encryption requires a root secret outside the database. V1 should support:

- Default local mode: generate/store a master key file with restrictive permissions.
- Production override: explicit master-key file path or equivalent startup option.

The root secret is not an upstream API key. It exists only to decrypt local database secrets and payload archives.

## Operational Database

Use SQLite in WAL mode. The database stores operational data:

- Admin account and sessions
- Proxy API key hashes and labels
- Encrypted upstream API keys
- Request metrics and routing decisions
- Token accounting
- Payload archive pointers, sizes, hashes, and capture status
- Health/degradation state where useful

SQLite is single-instance only for v1. One process owns the database and payload directory.

Store normalized token fields on request records, including:

- `input_tokens`
- `output_tokens`
- `total_tokens`
- `cached_input_tokens`
- `reasoning_tokens`
- `accepted_prediction_tokens`
- `rejected_prediction_tokens`
- `token_source`
- provider usage JSON

Use `NULL` for missing detailed provider fields unless a provider explicitly reports zero.

## Payload Capture

Full payload capture is enabled by default and configurable globally. It is archival first, not search/index first.

Payloads are not stored as large SQLite blobs. Store them as encrypted, compressed files on disk and keep pointers in SQLite.

Recommended layout:

```text
data/
  llm-proxy.sqlite
  payloads/
    2026-04-30/
      14/
        req_<request_id>.json.zst.enc
        res_<request_id>.jsonl.zst.enc
```

For non-streaming responses, store request and response bodies as JSON payloads.

For streaming responses, store the raw upstream stream in a replayable format, such as raw SSE bytes or JSONL chunk records. Preserve enough data to support forensic replay/debugging later.

Do not build heavy full-text indexing in v1. If later forensic tooling is needed, it can index archives out of band.

Payload capture failures are non-fatal for proxy requests. Continue serving the client when possible, record capture failure status/error in SQLite, and surface it in the admin health/dashboard. If capture fails after streaming has started, do not convert the client response into an artificial failure.

Authenticated admins can view/download decrypted raw payload content from the dashboard. Large payloads may be download-only or previewed with a size cap.

## Streaming Behavior

The proxy must not buffer a full streaming response before returning it.

For streaming requests:

- Forward upstream chunks to the client as they arrive.
- Capture chunks to payload archive.
- Parse chunks opportunistically for first content token, usage blocks, and output text.
- Preserve raw chunks even if parsing fails.
- If the client disconnects, abort the upstream request immediately.
- Record early disconnects as a metric.

## Metrics

Record request metrics split by:

- Proxy API key
- Requested model
- Upstream route/provider
- Endpoint
- Streaming vs non-streaming
- HTTP status/error category

Core timing fields:

- request start time
- upstream first byte time
- time to first token
- total duration
- generation duration

For streaming, `time_to_first_token` means request start to first streamed content delta. `tokens_per_second` should primarily mean output tokens divided by generation duration after the first token. Total-token-per-total-duration can be stored separately if useful.

Token accounting:

1. Trust provider-reported usage when available.
2. Otherwise estimate with `ceil(chars / 4)`.
3. Include content and tool call metadata in estimated character counts.
4. Record token source.
5. Record cached-token or detailed usage fields when providers report them.

Cost estimation is out of scope for v1.

## Dashboard

Use server-rendered pages with lightweight vendored JavaScript charts. All assets are embedded in the binary via `include_bytes!()` or equivalent compile-time embedding.

V1 dashboard includes:

- First-run setup
- Login/logout/password change
- Overview graphs:
  - requests over time
  - input/output/total tokens over time
  - output tokens per second
  - time to first token
  - error rate
- Breakdowns by:
  - proxy API key
  - requested model
  - route/provider
  - endpoint
- Request table:
  - timestamp
  - proxy key label
  - model
  - route
  - endpoint
  - status
  - duration
  - time to first token
  - tokens per second
  - token counts
  - early disconnect marker
  - payload capture status
- Request detail:
  - metadata and timings
  - provider usage JSON
  - payload availability, byte sizes, hashes
  - decrypted raw payload view/download
- Proxy API key management:
  - create, label, revoke
  - show generated key once
- Upstream secret management:
  - create/update/delete logical secret names
- Config status:
  - current config path
  - validation/reload status
  - reload action
- Health page:
  - DB connectivity
  - payload archive writability
  - master-key/encryption state
  - recent internal operational failures

Do not implement per-key limits, quotas, pricing, alerts, multi-user admin, or advanced payload search in v1.

## Logging

Use structured JSON logs by default. Logs are for operation of the proxy process, not request/provider analytics.

Log:

- startup/shutdown
- config created/loaded/reloaded/rejected
- DB migration start/completion/failure
- payload directory unavailable
- admin setup token generation
- server bind failures
- background task failures
- encryption/master-key problems
- unexpected internal bugs/panics

Do not log:

- each request
- prompt/response bodies
- upstream 4xx/5xx as routine log events
- token metrics
- dashboard-visible provider failures unless they indicate an internal/systemic proxy problem

## CLI

Keep the CLI minimal:

```text
llm-proxy
llm-proxy --config /path/to/config.json
llm-proxy config validate --config /path/to/config.json
llm-proxy config print-default
llm-proxy admin reset-password --config /path/to/config.json
```

Do not require an interactive init command. First startup creates missing config/data paths non-interactively.

## Rust Implementation Direction

Recommended stack:

- `axum` and `tokio` for HTTP servers and async runtime
- `reqwest` for upstream HTTP and streaming
- `sqlx` for SQLite migrations/queries
- server-rendered templates, compiled into the binary
- vendored local charting asset embedded into the binary
- `tracing`/`tracing-subscriber` for JSON operational logs
- `argon2` for admin passwords
- secure random generation for API keys, setup tokens, and sessions
- authenticated encryption for upstream secrets and payload files
- `zstd` for payload compression

Exact crate versions and table/query details can be chosen during implementation based on current ecosystem quality and repo constraints.

## Failure Semantics

Authentication/config failures before routing should fail the request.

Monitoring failures after a request has started should not interrupt traffic where avoidable:

- Payload capture failure: non-fatal, record and show degraded capture status.
- Metrics write failure during/after request: continue serving if possible, log operational DB error, mark admin health degraded.
- DB unavailable for API key validation: fail proxy request because auth cannot be established.
- Config reload invalid: reject new config and keep existing active config.
- Client disconnect during stream: abort upstream and record early disconnect.

## Out Of Scope For V1

- Multi-provider request/response shape conversion beyond OpenAI-compatible pass-through
- Multi-instance deployment
- Built-in TLS
- Cost estimation
- Quotas/rate limits/per-key model permissions
- Automatic payload/metrics retention cleanup
- Advanced forensic indexing/search
- Multi-admin accounts or admin roles
- Hosted/CDN frontend assets
