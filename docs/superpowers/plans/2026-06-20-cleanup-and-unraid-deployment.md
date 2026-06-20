# Cleanup and Unraid Deployment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up the repository and publish a Docker image to GHCR via GitHub Actions so the server can be deployed on Unraid using Compose Manager.

**Architecture:** A GitHub Actions workflow builds the existing multi-stage Dockerfile and pushes to `ghcr.io/pavolmisenko/img_server` on every code-affecting push to `main` and on `workflow_dispatch`. `docker-compose.yml` references the published image for Unraid; a `docker-compose.dev.yml` override restores local builds for development.

**Tech Stack:** Rust/Axum (existing), Docker (multi-stage Dockerfile, existing), GitHub Actions (`docker/build-push-action@v6`, `docker/metadata-action@v5`), GHCR (`ghcr.io`), Unraid Compose Manager plugin.

## Global Constraints

- GHCR image: `ghcr.io/pavolmisenko/img_server`
- Server port: `3000`
- `GITHUB_TOKEN` is used for GHCR auth — no manual secrets needed
- Path filter on main-branch pushes must cover: `src/**`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `.dockerignore`, `icons/**`, `weather_template.svg`, `descriptions.json`
- `workflow_dispatch` trigger must also be present as a manual escape hatch

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `.gitignore` | Modify | Add `plot.svg` and `.venv/` |
| `README.md` | Modify | Fix stale filename; add Deployment section |
| `docker-compose.yml` | Modify | Reference GHCR image instead of local build |
| `docker-compose.dev.yml` | Create | Local build override for development |
| `.github/workflows/docker.yml` | Create | CI: build and push to GHCR |

Files staged but not yet committed (handled in Task 1): `src/svg_process.rs` (new), `src/svg-process.rs` (deleted), `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `.dockerignore`, `Dockerfile`, `TODO.txt`, `scripts/`.

---

## Task 1: Repository housekeeping

**Files:**
- Modify: `.gitignore`
- Modify: `README.md:38`
- Stage: all pending tracked/untracked files except `plot.svg`, `.venv/`, `docker-compose.yml`

**Interfaces:**
- Produces: clean git state; `docker-compose.yml` left unstaged (modified in Task 2)

- [ ] **Step 1: Update `.gitignore`**

Open `.gitignore`. Current contents:
```
/target
```

Replace the entire file with:
```
/target
plot.svg
.venv/
```

- [ ] **Step 2: Verify `plot.svg` is now ignored**

```bash
git status
```

Expected: `plot.svg` no longer appears in the output (it is now ignored).

- [ ] **Step 3: Fix stale filename in `README.md`**

In `README.md` line 38, change:
```
│   └── svg-process.rs   # Weather fetching, SVG templating, BMP rendering
```
to:
```
│   └── svg_process.rs   # Weather fetching, SVG templating, BMP rendering
```

- [ ] **Step 4: Stage all files except `docker-compose.yml`**

```bash
git add .gitignore README.md
git add src/svg_process.rs src/main.rs Cargo.toml Cargo.lock
git add .dockerignore Dockerfile TODO.txt scripts/
git rm src/svg-process.rs
```

- [ ] **Step 5: Verify staging is correct**

```bash
git status
```

Expected output should show:
- `deleted: src/svg-process.rs`
- `new file: src/svg_process.rs`
- `modified: Cargo.lock`, `Cargo.toml`, `src/main.rs`
- `new file: .dockerignore`, `Dockerfile`, `README.md`, `TODO.txt`, `scripts/convert.py`
- `modified: .gitignore`
- `?? docker-compose.yml` (still untracked — intentional)

- [ ] **Step 6: Commit**

```bash
git commit -m "chore: repository housekeeping

- Rename svg-process.rs -> svg_process.rs (underscore convention)
- Add Dockerfile, docker-compose scaffold, README, scripts
- Ignore plot.svg and .venv/ artifacts

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Docker Compose restructure

**Files:**
- Modify: `docker-compose.yml`
- Create: `docker-compose.dev.yml`

**Interfaces:**
- Produces:
  - `docker-compose.yml` — references `ghcr.io/pavolmisenko/img_server:latest`
  - `docker-compose.dev.yml` — override that adds `build: .` and sets `image: img_server:dev`
  - Local dev command: `docker compose -f docker-compose.yml -f docker-compose.dev.yml up --build`

- [ ] **Step 1: Rewrite `docker-compose.yml`**

Replace the entire file:
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

- [ ] **Step 2: Create `docker-compose.dev.yml`**

```yaml
services:
  img_server:
    build: .
    image: img_server:dev
```

- [ ] **Step 3: Validate both compose files**

```bash
docker compose config
```

Expected: merged config printed with no errors. Image shown as `ghcr.io/pavolmisenko/img_server:latest`.

```bash
docker compose -f docker-compose.yml -f docker-compose.dev.yml config
```

Expected: merged config printed with `build` section present and `image: img_server:dev`.

- [ ] **Step 4: Commit**

```bash
git add docker-compose.yml docker-compose.dev.yml
git commit -m "chore: restructure compose files for GHCR deployment

Production compose references ghcr.io/pavolmisenko/img_server:latest.
Dev override restores local build via docker-compose.dev.yml.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: GitHub Actions CI workflow

**Files:**
- Create: `.github/workflows/docker.yml`

**Interfaces:**
- Consumes: existing `Dockerfile`, `GITHUB_TOKEN` (built-in, no configuration needed)
- Produces:
  - On push to `main` (code paths changed): pushes `ghcr.io/pavolmisenko/img_server:latest`
  - On `workflow_dispatch`: same as above (manual escape hatch)
  - On push of tag `v*.*.*`: pushes `latest` + semver tags (`1.2.3`, `1.2`, `1`)

- [ ] **Step 1: Create workflow directory and file**

```bash
mkdir -p .github/workflows
```

Create `.github/workflows/docker.yml`:
```yaml
name: Build and push Docker image

on:
  push:
    branches: [main]
    tags: ['v*.*.*']
    paths:
      - 'src/**'
      - 'Cargo.toml'
      - 'Cargo.lock'
      - 'Dockerfile'
      - '.dockerignore'
      - 'icons/**'
      - 'weather_template.svg'
      - 'descriptions.json'
  workflow_dispatch:

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/pavolmisenko/img_server
          tags: |
            type=raw,value=latest,enable={{is_default_branch}}
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=semver,pattern={{major}}

      - name: Build and push
        uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 2: Validate YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/docker.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

- [ ] **Step 3: Commit and push**

```bash
git add .github/workflows/docker.yml
git commit -m "ci: add GitHub Actions workflow to build and push to GHCR

Triggers on push to main (code path filter) and workflow_dispatch.
Pushes latest tag on main; semver tags on v*.*.* tag pushes.
Uses built-in GITHUB_TOKEN — no manual secrets required.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
git push origin main
```

- [ ] **Step 4: Verify CI run on GitHub**

Open `https://github.com/pavolmisenko/img_server/actions` in a browser.

Expected: a workflow run named "Build and push Docker image" appears and completes with a green checkmark. This takes ~5–10 minutes (Rust compile in CI).

- [ ] **Step 5: Verify the image is published**

Open `https://github.com/pavolmisenko/img_server/pkgs/container/img_server`.

Expected: package page shows `latest` tag with a recent push timestamp.

If the package shows as private, go to the package settings and set visibility to **Public**.

---

## Task 4: README — add Deployment section

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: GHCR image URL and Unraid workflow confirmed in Task 3

- [ ] **Step 1: Add Deployment section to README**

Append the following section after the existing **Dependencies** table in `README.md`:

```markdown
## Deployment

### Docker (local)

```bash
# Run the published image
docker compose up -d

# Build and run locally (development)
docker compose -f docker-compose.yml -f docker-compose.dev.yml up --build
```

### Unraid (Compose Manager)

1. In Unraid, install the **Compose Manager** plugin via Community Applications.
2. Go to **Docker → Compose → Add Stack**, name it `img_server`.
3. Paste the contents of `docker-compose.yml` into the editor.
4. Click **Pull** then **Start**.

The server binds port `3000` on the Unraid host. The ESP32 display calls:

```
http://<unraid-ip>:3000/fetch_bitmap?lat=<lat>&lng=<lng>&location=<name>
```

### Releases

The CI workflow in `.github/workflows/docker.yml` builds and pushes
`ghcr.io/pavolmisenko/img_server:latest` automatically on every push to
`main` that changes source files. Push a `v*.*.*` tag to also produce
pinned semver image tags (e.g. `1.2.3`).

To trigger a build manually (e.g. without a code change), use
**Actions → Build and push Docker image → Run workflow** on GitHub.
```

- [ ] **Step 2: Commit and push**

```bash
git add README.md
git commit -m "docs: add Docker and Unraid deployment instructions

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
git push origin main
```

---

## Verification Checklist

After all tasks are complete:

- [ ] `git status` shows a clean working tree
- [ ] `docker compose config` runs without errors
- [ ] GitHub Actions tab shows a green workflow run
- [ ] `ghcr.io/pavolmisenko/img_server:latest` is visible and public on the GitHub packages page
- [ ] `docker pull ghcr.io/pavolmisenko/img_server:latest` succeeds from any machine (no auth prompt)
- [ ] On Unraid: Compose Manager stack starts, container health check passes, port 3000 is reachable
- [ ] ESP32 successfully fetches a BMP: `curl -o test.bmp "http://<unraid-ip>:3000/fetch_bitmap?lat=49.1952&lng=16.608&location=Brno"`
