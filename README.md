# xatan

A CLI written in rust that gives you a branched [Xata](https://xata.io) database for any git branch.

Basically, instead of manual branch naming and configuration, `xatan` maps your database branches to your local Git branch or Jujutsu (jj) revision, while also automatically prefixing them with your developer identity to keep everyone's branches in your team isolated.

## Installation

The recommended way to install `xatan` is with [mise](https://mise.jdx.dev):

```bash
mise use github:fiws/xatan
```

Alternatively, you can build from source:

```bash
cargo install --git https://github.com/fiws/xatan
```

## Setup & Configuration

Configure `xatan` via environment variables (ideal for CI/CD or `.env` files):

```bash
export XATA_API_KEY="xau_..."
export XATA_ORG_ID="your-org"
export XATA_PROJECT_ID="your-project-id"
export XATA_DATABASE_NAME="your-db"
```

If you prefer a file-based configuration, run:

```bash
xatan init
```

This will walk you through a quick interactive setup and write a `.xatanrc` to your repository root.

## Commands

- **`whoami`**: Prints your resolved developer identity prefix (e.g., `jane-doe`).
- **`url [NAME]`**: Prints the Postgres connection URL for your branch. If the branch doesn't exist, `xatan` automatically creates it first. Pass `--no-create` to skip auto-creation.
- **`create <NAME>`**: Creates an isolated database branch prefixed with your identity.
- **`recreate [NAME]`**: Tears down and re-clones your branch from a parent (defaults to `main`), resetting your test data state.
- **`delete [NAME]`**: Deletes your developer branch safely.
- **`list`**: Lists database branches. Shows only your own by default. Use `--all` to view other developers' branches.
- **`shell [NAME]`**: Launches an interactive `psql` connection targeting your branch, with full Unix signal forwarding.
- **`prune`**: Automatically identifies and deletes remote database branches that no longer have a local Git branch or Jujutsu revision equivalent.

## Automated Post-Creation Hooks

Whenever `xatan` creates (or recreates) a branch, it can automatically run a script to seed your database or run migrations. You have two options here:

1. **Zero-Config**: If an executable or script is found at `.xata/post-create` (or with common extensions like `.sh`, `.bat`, `.ps1`), `xatan` will automatically run it.
2. **Explicit Command**: Alternatively, you can specify a custom command in your `.xatanrc` (`"postCreate": "npm run db:seed"`) or via the `XATAN_POST_CREATE` environment variable.

The hook script executes with the following environment variables automatically injected:

- `DATABASE_URL` (the connection string for the newly created branch)
- `XATAN_BRANCH_NAME` (the resolved branch name)
- `XATAN_PARENT_BRANCH` (the parent branch name, e.g., `main`)

_Note: The script's stdout is redirected to `stderr` of the parent `xatan` process. This keeps logs visible in your terminal but avoids polluting standard output, ensuring dynamic evaluation chains like `DATABASE_URL=$(xatan url)` continue working perfectly._

To temporarily bypass hooks, pass `--skip-post-create` to `url`, `create`, or `recreate`.

## Integration with mise

Integrating `xatan` with `mise` gives you a completely automated local development environment.

### Auto-inject dynamic `DATABASE_URL`

Configure `mise` to dynamically resolve your isolated developer branch and inject its connection string on directory entry. Because `xatan` caches URLs locally, this is virtually instantaneous:

```toml
# mise.toml
[env]
DATABASE_URL = "{{ exec(command='xatan url') }}"
```

Now, any tool, framework, or ORM (like Prisma, Drizzle, or `psql`) automatically targets your isolated sandbox with zero manual setup.

### Define convenient tasks

```toml
# mise.toml
[tasks."db:shell"]
description = "Open a psql shell to your isolated branch"
run = "xatan shell"

[tasks."db:recreate"]
description = "Reset your database branch and seed it from main"
run = "xatan recreate -y"
```

## Exit Codes

If you are scripting `xatan`, you can rely on these exit codes:

- `0` — Success
- `1` — Failure, aborted prompt, or general system/network error
- `2` — Branch not found (e.g. calling `url` with `--no-create`)
- `3` — Missing credentials or required configuration
