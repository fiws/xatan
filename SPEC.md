# Technical Specification: xatan CLI Tool

This document defines the language-agnostic architecture, command interface, and behavior for **`xatan`**—a developer-centric command-line helper for isolated, conflict-free Xata database branch orchestration.

---

## 1. Core Principles & Design Invariants

An implementation of `xatan` MUST respect the following invariants:

### I. Strict I/O Stream Separation
To support composability and piping, `xatan` separates human interaction from machine output:
* **`stdout` (Standard Output):** Reserved exclusively for deterministic command payloads (e.g., connection URLs, computed prefixes). No log lines, interactive prompts, spinners, or warnings may ever be printed to `stdout`.
* **`stderr` (Standard Error):** Used for all diagnostics, warnings, interactive prompts, progress indicators, and errors.

### II. Explicit Over Implicit Environment Mutation
`xatan` MUST NOT silently or implicitly edit environment configuration files (such as `.env`, `.env.local`, or `package.json`) unless explicitly instructed via a dedicated flag. Instead, it outputs clean raw strings to `stdout` to let developers pipe and compose actions themselves.

### III. Safe-by-Default Fallbacks
If a requested operation fails or targets a resource that is missing, the tool MUST return a non-zero exit code and write a structured error message to `stderr`, immediately halting any chained commands.

---

## 2. Smart Identity Resolution Algorithm

To prevent branches from conflicting across developers, every dynamic branch is prefixed with an ASCII-safe representation of the developer's identity.

```
                  +--------------------------------+
                  |  Resolve Developer Identity    |
                  +--------------------------------+
                                  |
                                  v
                  +--------------------------------+
                  |   Check Env Var: XATAN_PREFIX  |
                  +--------------------------------+
                     /                          \
             [Set]  /                            \ [Not Set]
                   v                              v
        +---------------------+        +--------------------+
        | Use Custom Prefix   |        |  Read Git Config:  |
        +---------------------+        |     user.email     |
                                       +--------------------+
                                         /                \
                                [Found] /                  \ [Missing]
                                       v                    v
                             +-------------------+  +------------------+
                             | Extract LocalPart |  | Read Git Config: |
                             | (Before the @)    |  |    user.name     |
                             +-------------------+  +------------------+
                                                      /              \
                                             [Found] /                \ [Missing]
                                                    v                  v
                                          +------------------+  +--------------+
                                          | Slugify Full Name|  | Read OS User |
                                          +------------------+  | ($USER)      |
                                                                +--------------+
                                                                       |
                                                                       v
                                                                +--------------+
                                                                | Slugify User |
                                                                +--------------+
```

### Steps of Resolution:
1. **Environment Override:** Check the `XATAN_PREFIX` environment variable. If present and non-empty, use it verbatim as the raw identifier.
2. **Git Email Extraction:** If not overridden, query `git config user.email`.
   * If found, extract the portion of the email *before* the `@` sign (the local-part).
3. **Git Name Fallback:** If the email config is missing, query `git config user.name`.
   * If found, use the full name as the raw identifier.
4. **OS User Fallback:** If all else fails, read the system username from the operating system environment (e.g., `$USER` or `whoami`).

### The Slugification Function:
The raw identifier MUST be converted to an ASCII-safe prefix string via these exact deterministic rules:
1. Convert all characters to lowercase.
2. Replace any continuous block of non-alphanumeric characters (including spaces, dots, `@`, `+`, hyphens, underscores) with a single hyphen `-`.
3. Trim any leading or trailing hyphens.

#### Examples:
* `Alice.Smith@company.com` $\rightarrow$ `alice-smith`
* `Dev+Sandbox@org.net` $\rightarrow$ `dev-sandbox`
* `Jane Doe` $\rightarrow$ `jane-doe`
* `admin_local` $\rightarrow$ `admin-local`

---

## 3. Configuration Schema (`.xatanrc`) & Resolution

Implementations read configuration from a file named `.xatanrc` or `xatan.json` located in the current repository root directory or any of its parent directories. However, **local configuration files are entirely optional**; the tool can be fully configured using environment variables.

### JSON-Schema Definition
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "XatanConfig",
  "type": "object",
  "properties": {
    "org": {
      "type": "string",
      "description": "The Xata Organization ID"
    },
    "project": {
      "type": "string",
      "description": "The Xata Project ID"
    },
    "database": {
      "type": "string",
      "description": "The default Xata Database name"
    },
    "fallbackParent": {
      "type": "string",
      "description": "The default parent branch to clone from when creating new branches",
      "default": "main"
    },
    "postCreate": {
      "type": "string",
      "description": "An optional hook command to run immediately after a database branch is created or synchronized"
    }
  },
  "required": ["org", "project", "database"]
}
```

### Configuration & Authentication Resolution

To support seamless CI/CD, scripting, and manual override capabilities, `xatan` resolves its credentials and settings through both environment variables and local config files using a defined resolution process.

#### 1. Authentication
All commands interacting with the Xata API (including `url`, `create`, `list`, `sync`, `delete`, and `shell`) require authentication.
* **API Key:** `xatan` expects the **`XATA_API_KEY`** environment variable to be set for authentication.
* If `XATA_API_KEY` is missing or empty when executing an API-interacting command, the tool MUST print a structured error message to `stderr` and exit with code `3` (Authentication / Config Missing).

#### 2. Settings Resolution Priority
`xatan` resolves each of the key properties (`org`, `project`, `database`, `fallbackParent`) dynamically in the following order of precedence (highest to lowest):

1. **Environment Variables:**
   * `org` $\leftarrow$ `XATA_ORG_ID`
   * `project` $\leftarrow$ `XATA_PROJECT_ID`
   * `database` $\leftarrow$ `XATA_DATABASE_NAME` (with fallback to `XATA_DATABASE`)
   * `fallbackParent` $\leftarrow$ `XATAN_FALLBACK_PARENT`
   * `postCreate` $\leftarrow$ `XATAN_POST_CREATE`
2. **Local Configuration File:**
   * Checks for `.xatanrc` or `xatan.json` in the current repository root/working directory or any parent directories.
   * If found, parses it as JSON.
   * Uses any values present in the file to resolve keys that have not already been supplied by environment variables.
3. **Defaults:**
   * `fallbackParent` defaults to `main` if undefined by both environment variables and files.
   * `fallbackParent` defaults to `main` if undefined by both environment variables and files.

#### 4. Post-Creation Database Hook Discovery (Zero-Config)
If `postCreate` is not explicitly configured via environment variables or a configuration file, `xatan` automatically performs zero-config hook discovery. It recursively looks for the repository root (containing `.git` or `.jj`) and checks for an executable or script inside the `.xata/` directory:
* **Windows:** Looks for `post-create.bat`, `post-create.cmd`, `post-create.ps1`, `post-create.sh`, or `post-create`.
* **Unix-like:** Looks for `post-create`, `post-create.sh`, or `post-create.bash`.

If a matching file is found, it is automatically resolved and executed as the post-creation hook.
#### 3. Validation Invariant
If, after applying the resolution order above, any of the required properties (`org`, `project`, `database`) cannot be resolved (i.e., they are empty or undefined), the tool MUST print a descriptive error to `stderr` and exit with code `3` (Authentication / Config Missing).

---
## 4. Command Specifications

Every command must support the global options `-h`/`--help` and `-v`/`--version`.

### 4.1. `init`
Sets up the workspace configuration file (OPTIONAL).

* **Behavior:**
  1. This command is completely optional. `xatan` works without any local configuration file if the required properties are provided via environment variables.
  2. If invoked, prompt interactively on `stderr` for Organization ID, Project ID, and Database Name.
  3. Attempt to auto-fill defaults by reading existing Xata environment variables (`XATA_ORG_ID`, `XATA_PROJECT_ID`, `XATA_DATABASE_NAME`, `XATA_DATABASE`) or checking an existing local `.xata/config.json`.
  4. Write the validated JSON payload into `.xatanrc` at the root of the current repository.
* **Exit Codes:**
  * `0`: Success.
  * `1`: Failure (e.g., directory not writeable, prompt aborted).
---

### 4.2. `whoami`
Outputs the resolved developer identity.

* **Behavior:**
  1. Execute the *Smart Identity Resolution Algorithm*.
  2. Write the final computed prefix string directly to `stdout` with a trailing newline.
  3. No other logs may be printed to `stdout`.
* **Exit Codes:**
  * `0`: Success.
  * `1`: Failed to resolve any valid identity fallback.

---

### 4.3. `url`
Resolves and prints the connection URL for a database branch.

```text
xatan-url 
Print the connection URL for a resolved database branch

USAGE:
    xatan url [OPTIONS] [NAME]

ARGS:
    <NAME>    The suffix of the target branch. If omitted, defaults
              to the slugified name of the active Git branch.

OPTIONS:
    --create              Auto-create the branch in Xata if it does not exist
    --parent <BRANCH>     The parent branch to clone from if creating [default: main]
    --skip-post-create    Skip executing the post-creation database hook
```

* **Resolution Logic:**
  1. Determine developer prefix (e.g., `jane-doe`).
  2. Determine target branch suffix:
     * If argument `[NAME]` is specified: Slugify the argument.
     * If argument `[NAME]` is omitted: Query the current local Git branch (e.g., `git branch --show-current`). Slugify the branch name.
  3. Concatenate: `<prefix>-<suffix>` (e.g., `jane-doe-feature-login`).
  4. Query Xata API to see if `<prefix>-<suffix>` exists.
  5. **Branch Exists:** Retrieve its connection URL, write it to `stdout`, and exit `0`.
  6. **Branch is Missing:**
     * If `--create` is set: Invoke Xata API to create the branch (parenting from the specified `--parent` or fallback parent from config). Block until creation completes, execute the post-creation hook (unless `--skip-post-create` is set), retrieve the connection URL, write it to `stdout`, and exit `0`.
* **Exit Codes:**
  * `0`: Success (printed URL to stdout).
  * `1`: General Error (invalid configuration, missing network).
  * `2`: Branch not found (when `--create` is omitted).

---

### 4.4. `create`
Creates a new isolated Xata branch prefixed with your identity.

```text
xatan-create 
Create a new isolated Xata branch prefixed with your identity

USAGE:
    xatan create [OPTIONS] <NAME>

ARGS:
    <NAME>    The clean suffix of the branch to create

OPTIONS:
    --parent <BRANCH>     Parent branch to clone from [default: main]
    --skip-post-create    Skip executing the post-creation database hook
```

* **Behavior:**
  1. Determine developer prefix (e.g., `jane-doe`) and resolve target branch name: `<prefix>-<slugified-name>`.
  2. Check if the branch already exists. If it does, write a warning to `stderr` and exit `0` (or write error and exit `1` if strict behavior is desired).
  3. Call Xata API to create the branch with the specified parent.
  4. Execute the post-creation hook (unless `--skip-post-create` is set).
  5. Output the fully qualified branch name to `stdout` upon success.
* **Exit Codes:**
  * `0`: Success.
  * `1`: Creation failed.

---

### 4.5. `list`
Lists project database branches, highlighting the developer's own.

```text
xatan-list 
List Xata branches, highlighting your own and active status

USAGE:
    xatan list [OPTIONS]

OPTIONS:
    --mine    Only show branches matching your developer prefix
    --all     Show all branches, including other developers [default]
```

* **Behavior:**
  1. Determine developer prefix.
  2. Retrieve all branches for the active project from the Xata API.
  3. Format a clean ASCII/Unicode table to `stdout`.
  4. Highlight rows matching the developer's prefix using colors (if TTY detected) or a leading indicator like `*` or `(mine)`.
* **Exit Codes:**
  * `0`: Success.
  * `1`: API retrieval failed.

---

### 4.6. `sync`
Re-clones or re-syncs schema/data from a parent branch. Because database branching in Xata is instantaneous, a synchronization is accomplished by a rapid tear-down and re-branch.

```text
xatan-sync 
Re-clone or re-sync schema and data from a parent branch

USAGE:
    xatan sync [OPTIONS] [NAME]

ARGS:
    [NAME]    The suffix of the branch to sync. Defaults to current Git branch counterpart.

OPTIONS:
    --from <BRANCH>       The parent branch to re-sync from [default: main]
    -y, --yes             Bypass safety confirmation prompt
    --skip-post-create    Skip executing the post-creation database hook
```

* **Behavior:**
  1. Resolve target branch name: `<prefix>-<suffix>`.
  2. Prompt for confirmation on `stderr` unless `-y`/`--yes` is supplied.
  3. Call Xata API to delete `<prefix>-<suffix>`.
  4. Call Xata API to create `<prefix>-<suffix>` with parent specified by `--from`.
  5. Execute the post-creation hook (unless `--skip-post-create` is set).
  6. Print success confirmation to `stderr`.
* **Exit Codes:**
  * `0`: Success.
  * `1`: Sync operation failed or was aborted.

---

### 4.7. `delete`
Deletes a developer branch safely.

```text
xatan-delete 
Tear down a Xata branch

USAGE:
    xatan delete [OPTIONS] [NAME]

ARGS:
    [NAME]    The suffix of the branch to delete. Defaults to current Git branch counterpart.

OPTIONS:
    -y, --yes    Bypass safety confirmation prompt
```

* **Behavior:**
  1. Resolve target branch name: `<prefix>-<suffix>`.
  2. Prompt for confirmation on `stderr` unless `-y`/`--yes` is supplied.
  3. Call Xata API to delete the branch.
* **Exit Codes:**
  * `0`: Success.
  * `1`: Deletion failed or was aborted.

---

### 4.8. `shell`
Launches an interactive `psql` connection or console targeting the resolved branch.

* **Behavior:**
  1. Resolve target branch name: `<prefix>-<suffix>`.
  2. Query Xata API for the branch database credentials.
  3. Execute `psql` (or equivalent database client process), replacing the current process context (`execve`) so that terminal signals and input are handled directly by the shell client.
* **Exit Codes:**
  * Inherits exit status of the database client process, or `1` if the connection could not be established.

---

## 5. Exit Code & Stream Matrix

| Outcome | Exit Code | Stdout Content | Stderr Content |
| :--- | :---: | :--- | :--- |
| **Command Success (Payload)** | `0` | Clean Payload (e.g., URL or branch name) | Empty, or non-polluting visual markers |
| **Command Success (Action)** | `0` | Empty | Success confirmation logs / spinners |
| **User Aborted Prompt** | `1` | Empty | Action cancelled message |
| **General System / Network Error** | `1` | Empty | Technical error detail |
| **Target Branch Missing** | `2` | Empty | "Branch does not exist" message |
| **Authentication / Config Missing**| `3` | Empty | "No valid credentials/config found" message |
