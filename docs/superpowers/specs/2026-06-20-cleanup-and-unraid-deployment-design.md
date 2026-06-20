# img_server — Cleanup & Unraid Deployment Design

**Date:** 2026-06-20  
**Status:** Approved

## Overview

Clean up the repository and establish a CI/CD pipeline that builds and publishes a Docker image to GitHub Container Registry (GHCR) on every relevant code change. The published image is then pulled by the Unraid server via Compose Manager and served on the local network for the ESP32 e-ink weather display.

## Scope

- Repository housekeeping (`.gitignore`, README, pending git state)
- GitHub Actions workflow: build + push to GHCR
- Docker Compose restructure: production file references GHCR image; dev file overrides with local build
- README update: add deployment section for Unraid

Out of scope: changes to the server logic, BMP output format, or SVG template.

---

## Section 1 — Repository Cleanup

### Pending git state
The following changes are already correct and just need to be committed:
- `src/svg-process.rs` deleted (hyphen → underscore rename)
- `src/svg_process.rs` added
- `Cargo.toml`, `Cargo.lock`, `main.rs` modified

### `.gitignore`
Current file only has `target/`. Add:
- `plot.svg` — test output artifact generated locally
- `.venv/` — leftover Python virtual environment from `scripts/convert.py` dev work

### README
- Fix stale filename reference: `src/svg-process.rs` → `src/svg_process.rs` in the Project Structure section
- Add a **Deployment** section documenting the Docker / Unraid Compose Manager workflow (see Section 3)

### `scripts/convert.py`
Keep as-is. It is a useful local dev utility for converting server BMP output to the 800×480 1-bit monochrome format used by the e-ink display. The `.venv/` directory it uses is covered by the updated `.gitignore`.

---

## Section 2 — GitHub Actions CI

**File:** `.github/workflows/docker.yml`

### Triggers

| Event | Condition | Effect |
|-------|-----------|--------|
| `push` to `main` | Path filter (see below) | Build + push `latest` tag |
| `push` tag `v*.*.*` | — | Build + push versioned tags |

**Path filter** (skip rebuild on doc-only changes):
```
src/**
Cargo.toml
Cargo.lock
Dockerfile
.dockerignore
icons/**
weather_template.svg
descriptions.json
```

### Steps

1. `actions/checkout@v4`
2. `docker/setup-buildx-action@v3`
3. `docker/login-action@v3` — authenticates to `ghcr.io` using the built-in `GITHUB_TOKEN` (no manual secrets needed)
4. `docker/metadata-action@v5` — derives image tags and OCI labels from the git ref
5. `docker/build-push-action@v6` — builds with the existing multi-stage `Dockerfile` and pushes

### Image tags produced

| Git event | Tags pushed |
|-----------|-------------|
| Push to `main` | `ghcr.io/pavolmisenko/img_server:latest` |
| Push tag `v1.2.3` | `ghcr.io/pavolmisenko/img_server:1.2.3`, `:1.2`, `:1`, `:latest` |

Unraid can pin to `latest` for automatic updates or to a specific version (e.g. `1.2.3`) for stability.

### Infrastructure
GitHub's free `ubuntu-latest` runners. No self-hosted runners or additional billing required.

---

## Section 3 — Docker Compose

### `docker-compose.yml` (production / Unraid)

References the published GHCR image. This is the file Unraid Compose Manager uses.

```yaml
services:
  img_server:
    image: ghcr.io/pavolmisenko/img_server:latest
    container_name: img_server
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      - RUST_LOG=img_server=info
    healthcheck:
      test: ["CMD", "curl", "-sf", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 15s
```

### `docker-compose.dev.yml` (local development override)

Overrides the image with a local build. Used only on the development machine.

```yaml
services:
  img_server:
    build: .
    image: img_server:dev
```

**Local dev usage:**
```bash
docker compose -f docker-compose.yml -f docker-compose.dev.yml up --build
```

### Unraid Compose Manager setup
1. Install the **Compose Manager** plugin via Community Applications
2. Create a new stack named `img_server`
3. Paste the contents of `docker-compose.yml` (or point to the raw GitHub URL)
4. Pull → Start

The container binds port 3000 on the Unraid host. The ESP32 reaches it at:
```
http://<unraid-ip>:3000/fetch_bitmap?lat=<lat>&lng=<lng>&location=<name>
```

---

## Files Changed

| File | Action |
|------|--------|
| `.gitignore` | Add `plot.svg`, `.venv/` |
| `README.md` | Fix filename, add Deployment section |
| `docker-compose.yml` | Replace `build: .` with GHCR image reference |
| `docker-compose.dev.yml` | New — local build override |
| `.github/workflows/docker.yml` | New — CI workflow |
| `src/svg-process.rs` | Delete (already renamed) |
| `src/svg_process.rs` | Already exists, commit it |
