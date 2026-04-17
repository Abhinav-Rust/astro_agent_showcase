# Astro Agent — Multi-Agent LLM Pipeline (Showcase)

> **⚠️ Domain IP Redacted** — All proprietary astrological logic (master prompts, Dasha calculations, Yoga detection, and the Vedic rules engine) has been stripped from this repository. Function signatures and data structures are preserved as stubs to demonstrate the architecture. This repo is published as a **structural showcase only**.

---

## What This Is

A **Rust-based, high-performance, fault-tolerant multi-agent LLM pipeline** built to orchestrate complex, multi-step AI workflows against rate-limited APIs. The system was originally designed for production Vedic astrology readings but the patterns generalize to any domain requiring:

- **Multi-agent orchestration** — Agent 1 extracts structured parameters from natural language; Agent 2 generates long-form analytical output conditioned on deterministic data.
- **Resilient API communication** — Custom exponential backoff with dynamic rate-limit parsing directly from error message bodies, `Retry-After` header respect, and configurable retry ceilings.
- **Connection lifecycle management** — Deliberate connection tearing via `pool_idle_timeout`, `pool_max_idle_per_host`, and disabled TCP keepalive to survive long inter-request cooldowns on free-tier APIs.
- **Zero-copy prompt pipelines** — Multi-stage prompt assembly with data anonymization layers before API submission.

---

## Architecture

```
┌──────────────┐     ┌─────────────────┐     ┌────────────────────┐
│  CLI / TUI   │────▶│   Math Engine   │────▶│  Rules Engine      │
│  (main.rs)   │     │   (math.rs)     │     │  (rules.rs)        │
│              │     │   [STUBBED]     │     │  [STUBBED]         │
└──────┬───────┘     └─────────────────┘     └────────┬───────────┘
       │                                              │
       │         ┌──────────────────┐                 │
       │         │  Dasha Engine    │◀────────────────┘
       │         │  (dasha.rs)      │
       │         │  [STUBBED]       │
       │         └────────┬─────────┘
       │                  │
       ▼                  ▼
┌──────────────────────────────────────────────────────────────────┐
│                     API Layer (api.rs)                           │
│  • Gemini API integration with structured request/response      │
│  • 10-retry loop with 3-tier backoff strategy                   │
│  • Dynamic rate-limit parsing from error message bodies         │
│  • Retry-After header detection                                 │
│  • Exponential jitter fallback (30s floor, 120s cap)            │
└──────────────────────────────┬───────────────────────────────────┘
                               │
                     ┌─────────▼─────────┐
                     │   Geo Resolver    │
                     │   (geo.rs)        │
                     │   Nominatim +     │
                     │   tzf-rs offline  │
                     │   TZ resolution   │
                     └───────────────────┘
```

---

## Key Engineering Patterns

### 1. Custom Rate-Limit Backoff (3-Tier)

The API layer (`api.rs`) implements a production-grade retry strategy:

| Priority | Source | Mechanism |
|----------|--------|-----------|
| 1st | Error message body | Regex-parses `"retry in Xs"` / `"retry after Xs"` from Gemini's JSON error payload |
| 2nd | HTTP header | Reads the standard `Retry-After` response header |
| 3rd | Exponential jitter | `base_ms * 2^attempt` with random jitter, floored at 30s, capped at 120s |

### 2. Connection Tearing

Free-tier APIs enforce per-minute rate limits. Holding idle connections across 30–60s cooldowns causes `connection reset` errors. The client is configured to aggressively recycle:

```rust
reqwest::Client::builder()
    .tcp_keepalive(None)           // Disable TCP keepalive
    .pool_idle_timeout(5s)         // Tear idle connections after 5s
    .pool_max_idle_per_host(1)     // Max 1 idle connection per host
    .timeout(120s)                 // Allow long generation times
```

### 3. Multi-Agent Pipeline

| Agent | Role | Model |
|-------|------|-------|
| Agent 1 | Structured data extraction (temporal target parsing) | `gemini-3.1-flash-lite` |
| Agent 2 | Long-form analytical generation conditioned on deterministic data | `gemini-3.1-flash-lite` |

A mandatory 31-second inter-agent cooldown prevents back-to-back rate limiting on free-tier quotas.

### 4. Data Anonymization Layer

Client PII is stripped from the prompt payload before API submission — names are replaced with generic identifiers so the external LLM never sees real client data.

---

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (Edition 2024) |
| Async Runtime | Tokio |
| HTTP Client | Reqwest (with JSON, connection tuning) |
| LLM Provider | Google Gemini API (v1beta) |
| Database | SQLite via rusqlite (bundled) |
| Geocoding | Nominatim (OpenStreetMap) |
| Timezone Resolution | `tzf-rs` (offline, embedded TZ database) |
| Historical TZ Offsets | `chrono-tz` |
| TUI | `dialoguer` + `console` |
| Ephemeris | Swiss Ephemeris via `swiss-eph` (stubbed) |

---

## Project Structure

```
src/
├── main.rs       # CLI/TUI, orchestration, DB layer, presentation
├── api.rs        # Gemini API client, retry logic, backoff engine
├── math.rs       # [STUBBED] Planetary position calculations
├── rules.rs      # [STUBBED] Vedic astrology rules engine
├── dasha.rs      # [STUBBED] Vimshottari Dasha timeline generator
├── geo.rs        # Geocoding + historical timezone resolution
└── verify_db.rs  # Standalone DB inspection utility
```

---

## Running

```bash
# Set your Gemini API key
export GEMINI_API_KEY="your-key-here"

# Build and run
cargo run --bin astro_agent
```

> **Note:** The stubbed math/rules/dasha modules return dummy data. The pipeline will execute end-to-end but the generated readings will lack real astronomical input.

---

## License

This repository is published for portfolio/showcase purposes only. The architecture, patterns, and non-redacted code are available for reference. The redacted domain IP remains proprietary.
