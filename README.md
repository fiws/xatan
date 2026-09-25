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

### 1. Authentication (`XATA_API_KEY`)

All commands interacting with the Xata API require authentication via the `XATA_API_KEY` environment variable. The API key must include the `credentials:read` scope. `xatan` retrieves connection strings from Xata's branch credentials endpoint, adds `sslmode=require` when the endpoint omits an SSL mode, and preserves other URL parameters.

Because the API key is a secret credential, **never commit it to version control**. Recommend storing it securely using one of the following methods:

- **In `mise.local.toml`** (local per-repo config, gitignored):
  ```toml
  # mise.local.toml
  [env]
  XATA_API_KEY = "xau_..."
  ```
- **Via a secret manager or password manager CLI** in `mise.local.toml`:
  ```toml
  # mise.local.toml
  [env]
  # 1Password CLI
  XATA_API_KEY = "{{ exec(command='op read op://private/xata/credential') }}"
  # Or Bitwarden / pass / gcloud / etc.
  # XATA_API_KEY = "{{ exec(command='bw get password xata-api-key') }}"
  # XATA_API_KEY = "{{ exec(command='pass show xata/api-key') }}"
  ```
- **In user-level shell configuration or secret store** (e.g., `~/.zshrc`, `~/.bashrc`):
  ```bash
  export XATA_API_KEY="xau_..."
  ```

### 2. Workspace & Database Configuration

Configure project settings via environment variables (ideal for CI/CD or `.env` files):

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

The default parent branch is `main`. Configure another default with `"defaultParent": "develop"` in `.xatanrc`/`xatan.json` or with `XATAN_DEFAULT_PARENT=develop`. The `--parent` flag for `url` and `create`, and `--from` for `recreate`, override the configured default.
## Commands

- **`whoami`**: Prints your resolved developer identity prefix (e.g., `jane-doe`).
- **`url [NAME]`**: Prints the Postgres connection URL for your branch. Without `NAME`, it uses the current Git branch or Jujutsu revision; if neither can be determined, it warns on `stderr` and uses the `nobranch` suffix. If the database branch doesn't exist, `xatan` automatically creates it first and prunes unneeded remote branches in the background (enabled by default; configurable via `autoPrune`/`XATAN_AUTO_PRUNE`). Pass `--no-create` to skip auto-creation, or `--no-prune` (`--skip-prune`) to skip background pruning.
- **`create <NAME>`**: Creates an isolated database branch prefixed with your identity.
- **`recreate [NAME]`**: Tears down and re-clones your branch from a parent (defaults to `main`), resetting your test data state.
- **`delete [NAME]`**: Deletes your developer branch safely.
- **`list`**: Lists database branches. Shows only your own by default. Use `--all` to view other developers' branches.
- **`psql [--branch NAME] [PSQL_ARGS]...`**: Runs native `psql` against your authenticated branch connection. Every trailing argument is forwarded unchanged; use `--branch` to target a non-default branch. The existing **`shell [NAME]`** command remains available for interactive use.
- **`prune`**: Automatically identifies and deletes remote database branches that no longer have a local Git branch or Jujutsu revision equivalent.

For example, native `psql` flags work directly:

```bash
xatan psql -c "SELECT c_defaults FROM user_info WHERE c_uid = 'testuser'"
xatan psql --branch feature -f schema.sql
```

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

Configure `mise` to dynamically resolve your isolated developer branch and inject its connection string on directory entry. When referencing `xatan` (installed via mise tools) in `[env]`, set `tools = true` so mise loads the tools on the `PATH` before evaluating the template; otherwise, the command will fail:

```toml
# mise.toml or mise.local.toml
[env]
DATABASE_URL = { value = "{{ exec(command='xatan url') }}", tools = true }
```

Now, any tool, framework, or ORM (like Prisma, Drizzle, or `psql`) automatically targets your isolated sandbox with zero manual setup.
### Define convenient tasks

```toml
# mise.toml
[tasks."db:psql"]
description = "Open psql for the isolated branch"
run = "xatan psql"

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
