# xatan

Developer-centric CLI helper for isolated, conflict-free Xata database branch orchestration.

---

## Features

- **VCs-Aware Suffix Resolution:** Automatically resolves target branch suffixes from active **Git branches** or **Jujutsu (jj) revisions**.
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
- **`url [NAME] [--no-create]`**: Print the Postgres connection string for a branch. Auto-creates it if it does not exist (unless `--no-create` is passed).
- **`create <NAME> [--parent <BRANCH>]`**: Create an isolated branch cloned from a parent.
- **`list [--mine] [--all]`**: List database branches, showing only your own by default.
- **`recreate [NAME] [--from <BRANCH>] [-y]`**: Re-clone schema and data from a parent.
- **`delete [NAME] [-y]`**: Delete a branch safely.
- **`shell [NAME]`**: Open an interactive `psql` connection.

---

## Post-Creation Database Hooks

`xatan` supports executing automated post-creation database modification scripts (e.g., seeding, migrations, or database initialization) immediately after a new database branch is dynamically created (via `url` or `create`) or recreated (via `recreate`).

### 1. Explicit Configuration

Configure your post-creation script command directly in your local `.xatanrc` configuration file:

```json
{
  "org": "your-org",
  "project": "your-project",
  "database": "your-db",
  "postCreate": "npm run db:seed"
}
```

Or dynamically via the `XATAN_POST_CREATE` environment variable:

```bash
export XATAN_POST_CREATE="python run_migrations.py && python seed_admin.py"
```

### 2. Zero-Config Convention-Based Hooks

If no explicit hook is configured, `xatan` will automatically look for and run an executable or script located in the `.xata/` directory at the root of your repository (e.g., `.xata/post-create`).

Supported convention file names:
* **Unix-like (macOS/Linux):** `.xata/post-create`, `.xata/post-create.sh`, or `.xata/post-create.bash` (Ensure the file is executable: `chmod +x .xata/post-create`)
* **Windows:** `.xata/post-create.bat`, `.xata/post-create.cmd`, `.xata/post-create.ps1`, `.xata/post-create.sh`, or `.xata/post-create`

### 3. Injected Environment Variables

The post-creation script subprocess runs with the following automatically injected environment variables, allowing your seeders/migration tools to connect to the newly created branch instantly:

* `DATABASE_URL` / `XATA_DATABASE_URL`: The fully qualified and rewritten connection string for the new isolated branch.
* `XATAN_BRANCH_NAME`: The resolved name of the new branch (e.g., `jane-doe-feature-login`).
* `XATAN_PARENT_BRANCH`: The parent branch that this new branch was cloned from (e.g., `main`).
* `XATA_ORG_ID`: The resolved Xata organization ID.
* `XATA_PROJECT_ID`: The resolved Xata project ID.
* `XATA_DATABASE_NAME`: The resolved default database name.

### 4. Direct Stream Separation & Piping

The stdout of the hook subprocess is automatically captured and redirected to standard error (`stderr`) of the `xatan` parent process. This guarantees that your hook's console output is safely visible in your terminal, while strictly preserving clean `stdout` separation so dynamic evaluation chains like `DATABASE_URL=$(xatan url)` work without pollution.

### 5. Bypassing the Hook

To temporarily bypass the post-creation hook for a specific command run, pass the `--skip-post-create` flag:

```bash
xatan url --skip-post-create
xatan create feature-branch --skip-post-create
xatan recreate --skip-post-create
```

---

## Integration with mise

[mise](https://mise.jdx.dev) is a fast environment and tool manager. You can integrate `xatan` into your `mise.toml` to automate your local development database environment.

### 1. Auto-Inject Dynamic `DATABASE_URL`

Configure `mise` to dynamically resolve (and auto-create) your isolated developer branch and inject its connection string directly into your environment on directory entry.

Because `xatan` automatically and natively caches connection URLs locally (with sub-millisecond retrievals and zero network overhead), you can use a simple, clean, dynamic assignment:

```toml
# mise.toml
[env]
DATABASE_URL = "{{ exec(command='xatan url') }}"
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

[tasks."db:recreate"]
description = "Recreate your branch schema & data from main"
run = "xatan recreate -y"
```

---

## Exit Codes

- `0`: Success.
- `1`: Failure / Aborted Prompt / System or Network Error.
- `2`: Target Branch Missing (e.g., calling `url` with `--no-create`).
- `3`: Authentication / Config Missing (e.g., empty required environment variables).
