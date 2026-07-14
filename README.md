# xatan

Developer-centric CLI helper for isolated, conflict-free Xata database branch orchestration.

---

## Features

- **VCs-Aware Suffix Resolution:** Automatically resolves target branch suffixes from active **Git branches** or **Jujutsu (jj) bookmarks/revisions**.
- **Collision-Free Developer Identity:** Generates unique, ASCII-safe prefixes using Git/OS metadata (includes domain for generic prefixes; hashes long domains with FNV-1a).
- **Pure Unix Signal Passing:** Launches interactive shells via `psql`.

---

## Quickstart

### 1. Build

```bash
cargo build --release
```

### 2. Configure (Optional)

Generate a local `.xatanrc` configuration file at your repository root:

```bash
target/release/xatan init
```

Or configure dynamically via environment variables in your shell (or `.env`):

```bash
XATA_API_KEY="xau_..."
XATA_ORG_ID="your-org"
XATA_PROJECT_ID="your-project-id"
XATA_DATABASE_NAME="your-db"
```

---

## Commands

- **`whoami`**: Print your unique developer identity prefix (e.g., `me-fiws-net`).
- **`url [NAME] [--create]`**: Print the Postgres connection string for a branch. Auto-creates it if `--create` is set.
- **`create <NAME> [--parent <BRANCH>]`**: Create an isolated branch cloned from a parent.
- **`list [--mine] [--all]`**: List database branches, highlighting your own.
- **`sync [NAME] [--from <BRANCH>] [-y]`**: Re-clone schema and data from a parent.
- **`delete [NAME] [-y]`**: Delete a branch safely.
- **`shell [NAME]`**: Open an interactive `psql` connection.

---

## Integration with mise

[mise](https://mise.jdx.dev) is a fast environment and tool manager. You can integrate `xatan` into your `mise.toml` to automate your local development database environment.

### 1. Auto-Inject Dynamic `DATABASE_URL`

Configure `mise` to dynamically resolve (and auto-create) your isolated developer branch and inject its connection string directly into your environment on directory entry.

Because `xatan` automatically and natively caches connection URLs locally (with sub-millisecond retrievals and zero network overhead), you can use a simple, clean, dynamic assignment:

```toml
# mise.toml
[env]
DATABASE_URL = "{{ exec(command='xatan url --create') }}"
```

Now, any application or ORM (like Prisma, Drizzle, or `psql`) in this directory can automatically connect to your isolated developer branch with zero manual steps!

### 2. Configure Credentials Securely

Keep your private API keys in `mise.local.toml` (which is gitignored) and project settings in `mise.toml`:

```toml
# mise.toml
[env]
XATA_ORG_ID = "your-org"
XATA_PROJECT_ID = "your-project-id"
XATA_DATABASE_NAME = "your-db"
```

```toml
# mise.local.toml
[env]
XATA_API_KEY = "xau_yourprivatekey..."
```

### 3. Define Convenient Tasks

Optionally, set up mise tasks to quickly interact with your database:

```toml
# mise.toml
[tasks."db:shell"]
description = "Launch psql shell targeting your isolated branch"
run = "xatan shell"

[tasks."db:sync"]
description = "Re-sync your branch schema & data from main"
run = "xatan sync -y"
```

---

## Exit Codes

- `0`: Success.
- `1`: Failure / Aborted Prompt / System or Network Error.
- `2`: Target Branch Missing (e.g., calling `url` without `--create`).
- `3`: Authentication / Config Missing (e.g., empty required environment variables).
