use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

mod cache;
mod config;
mod identity;
mod prompt;
mod xata;

#[derive(Parser, Debug)]
#[command(
    name = "xatan",
    version,
    about = "Developer-centric helper for isolated, conflict-free Xata database branch orchestration"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sets up the workspace configuration file (.xatanrc)
    Init,

    /// Outputs the resolved developer identity
    Whoami,

    /// Resolves and prints the connection URL for a database branch
    Url {
        /// The suffix of the target branch. Defaults to current Git branch counterpart.
        name: Option<String>,

        /// Auto-create the branch in Xata if it does not exist (this is now the default)
        #[arg(long, overrides_with = "no_create")]
        create: bool,

        /// Do not auto-create the branch in Xata if it does not exist
        #[arg(long, overrides_with = "create")]
        no_create: bool,

        /// The parent branch to clone from if creating
        #[arg(long)]
        parent: Option<String>,

        /// Skip executing the post-creation database hook
        #[arg(long)]
        skip_post_create: bool,
    },

    /// Creates a new isolated Xata branch prefixed with your identity
    Create {
        /// The clean suffix of the branch to create
        name: String,

        /// Parent branch to clone from
        #[arg(long)]
        parent: Option<String>,

        /// Skip executing the post-creation database hook
        #[arg(long)]
        skip_post_create: bool,
    },
    /// Lists project database branches, showing only your own by default
    List {
        /// Only show branches matching your developer prefix [default]
        #[arg(long, conflicts_with = "all")]
        mine: bool,

        /// Show all branches, including other developers
        #[arg(long, conflicts_with = "mine")]
        all: bool,
    },
    /// Re-creates (re-clones) a branch from a parent, tearing down the old one
    Recreate {
        /// The suffix of the branch to recreate. Defaults to current Git branch counterpart.
        name: Option<String>,

        /// The parent branch to recreate from
        #[arg(long)]
        from: Option<String>,

        /// Bypass safety confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Skip executing the post-creation database hook
        #[arg(long)]
        skip_post_create: bool,
    },

    /// Deletes a developer branch safely
    Delete {
        /// The suffix of the branch to delete. Defaults to current Git branch counterpart.
        name: Option<String>,

        /// Bypass safety confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Launches an interactive psql connection targeting the resolved branch
    Shell {
        /// The suffix of the branch to open. Defaults to current Git branch counterpart.
        name: Option<String>,
    },
}

/// Query current active local Jujutsu bookmark/revision or Git branch
fn get_current_vcs_branch_or_bookmark() -> Option<String> {
    // 1. Try Jujutsu (jj) first
    // First, check if there are any local bookmarks on `@`
    let jj_bookmark = Command::new("jj")
        .args([
            "log",
            "-r",
            "@",
            "-T",
            "local_bookmarks",
            "--no-graph",
            "--color=never",
        ])
        .output();

    if let Ok(output) = jj_bookmark {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // Otherwise, find the "branch" (the root/sprout of the branch) and use its change_id
    let jj_branch = Command::new("jj")
        .args([
            "log",
            "-r",
            "roots(trunk()..@)",
            "-T",
            "change_id.short(12)",
            "--no-graph",
            "--color=never",
        ])
        .output();

    if let Ok(output) = jj_branch {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // 2. Fallback to Git
    Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Recursively searches for repository root by looking for .git or .jj traversing up
fn find_repository_root() -> Option<PathBuf> {
    let mut current_dir = std::env::current_dir().ok()?;
    loop {
        if current_dir.join(".git").exists() || current_dir.join(".jj").exists() {
            return Some(current_dir);
        }
        if !current_dir.pop() {
            break;
        }
    }
    std::env::current_dir().ok()
}
/// Searches for a post-create hook by convention inside the `.xata/` directory.
/// Returns the absolute path of the found script or executable, if any exists.
fn find_convention_hook_file() -> Option<String> {
    let root = find_repository_root()?;
    let xata_dir = root.join(".xata");
    if !xata_dir.is_dir() {
        return None;
    }

    let candidates = if cfg!(windows) {
        vec![
            "post-create.bat",
            "post-create.cmd",
            "post-create.ps1",
            "post-create.sh",
            "post-create",
        ]
    } else {
        vec!["post-create", "post-create.sh", "post-create.bash"]
    };

    for candidate in candidates {
        let file_path = xata_dir.join(candidate);
        if file_path.is_file() {
            if let Ok(abs_path) = file_path.canonicalize() {
                return Some(abs_path.to_string_lossy().into_owned());
            } else {
                return Some(file_path.to_string_lossy().into_owned());
            }
        }
    }

    None
}

/// Resolves full target branch name `<prefix>-<suffix>` using Smart Identity Resolution Algorithm
fn resolve_target_branch(name_arg: Option<&str>) -> Result<String, String> {
    let prefix = identity::resolve_identity()?;
    let suffix = if let Some(n) = name_arg {
        identity::slugify(n)
    } else {
        let vcs_ref = get_current_vcs_branch_or_bookmark()
            .ok_or_else(|| "Failed to query current Git branch or Jujutsu bookmark. Please specify branch name argument.".to_string())?;
        identity::slugify(&vcs_ref)
    };

    if suffix.is_empty() {
        return Err("Resolved target branch suffix is empty".to_string());
    }

    if suffix == prefix || suffix.starts_with(&format!("{}-", prefix)) {
        Ok(suffix)
    } else {
        Ok(format!("{}-{}", prefix, suffix))
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            if let Err(e) = run_init() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Whoami => match identity::resolve_identity() {
            Ok(prefix) => {
                println!("{}", prefix);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Url {
            name,
            create: _,
            no_create,
            parent,
            skip_post_create,
        } => {
            let create = !no_create;
            let config = resolve_or_exit();
            let branch_name = match resolve_target_branch(name.as_deref()) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // 1. Check local cache first for sub-millisecond retrieval
            if let Some(cached_url) = cache::get_cached_url(&branch_name) {
                println!("{}", cached_url);
                std::process::exit(0);
            }

            let client = xata::XataClient::new(&config);
            match client.get_branch(&branch_name) {
                Ok(Some(branch)) => {
                    if let Some(conn_str) = branch.connection_string {
                        let rewritten = rewrite_connection_string(&conn_str, &config.database);
                        // Save to cache
                        cache::set_cached_url(&branch_name, &rewritten);
                        println!("{}", rewritten);
                        std::process::exit(0);
                    } else {
                        eprintln!(
                            "Error: Branch '{}' exists but has no connection URL.",
                            branch_name
                        );
                        std::process::exit(1);
                    }
                }
                Ok(None) => {
                    if create {
                        let parent_branch = parent.as_deref().unwrap_or(&config.fallback_parent);
                        let parent_id = resolve_parent_id(&client, parent_branch);
                        use std::io::IsTerminal;
                        let is_tty = std::io::stderr().is_terminal() && std::io::stdout().is_terminal();

                        let spinner = if is_tty {
                            let _ = prompt::intro("xatan url");
                            let s = prompt::spinner();
                            s.start(format!(
                                "Creating branch '{}' from '{}'...",
                                branch_name, parent_branch
                            ));
                            Some(s)
                        } else {
                            eprintln!(
                                "Creating branch '{}' from '{}'...",
                                branch_name, parent_branch
                            );
                            None
                        };

                        match client.create_branch(&branch_name, Some(&parent_id)) {
                            Ok(created_branch) => {
                                if let Some(s) = &spinner {
                                    s.stop("Branch created.");
                                }
                                if let Some(conn_str) = created_branch.connection_string {
                                    let rewritten =
                                        rewrite_connection_string(&conn_str, &config.database);
                                    // Save to cache
                                    cache::set_cached_url(&branch_name, &rewritten);

                                    if !skip_post_create {
                                        if let Some(ref command) = config
                                            .post_create
                                            .clone()
                                            .or_else(find_convention_hook_file)
                                        {
                                            eprintln!("Running post-creation hook: {}", command);
                                            if let Err(e) = run_post_create_hook(
                                                command,
                                                &rewritten,
                                                &branch_name,
                                                parent_branch,
                                                &config,
                                            ) {
                                                eprintln!(
                                                    "Error executing post-creation hook: {}",
                                                    e
                                                );
                                                std::process::exit(1);
                                            }
                                        }
                                    }

                                    println!("{}", rewritten);
                                    std::process::exit(0);
                                } else {
                                    // Fallback retry getting detailed branch
                                    match client.get_branch(&branch_name) {
                                        Ok(Some(re_fetched)) => {
                                            if let Some(conn_str) = re_fetched.connection_string {
                                                let rewritten = rewrite_connection_string(
                                                    &conn_str,
                                                    &config.database,
                                                );
                                                // Save to cache
                                                cache::set_cached_url(&branch_name, &rewritten);

                                                if !skip_post_create {
                                                    if let Some(ref command) = config
                                                        .post_create
                                                        .clone()
                                                        .or_else(find_convention_hook_file)
                                                    {
                                                        eprintln!(
                                                            "Running post-creation hook: {}",
                                                            command
                                                        );
                                                        if let Err(e) = run_post_create_hook(
                                                            command,
                                                            &rewritten,
                                                            &branch_name,
                                                            parent_branch,
                                                            &config,
                                                        ) {
                                                            eprintln!(
                                                                "Error executing post-creation hook: {}",
                                                                e
                                                            );
                                                            std::process::exit(1);
                                                        }
                                                    }
                                                }

                                                println!("{}", rewritten);
                                                std::process::exit(0);
                                            } else {
                                                eprintln!(
                                                    "Error: Branch created, but connection URL is not available."
                                                );
                                                std::process::exit(1);
                                            }
                                        }
                                        _ => {
                                            eprintln!(
                                                "Error: Created branch but failed to retrieve credentials."
                                            );
                                            std::process::exit(1);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                if let Some(s) = &spinner {
                                    s.stop("Creation failed.");
                                } else {
                                    eprintln!("Creation failed.");
                                }
                                eprintln!("API Error: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        eprintln!(
                            "Error: Branch '{}' does not exist. Omit --no-create to create it dynamically.",
                            branch_name
                        );
                    }
                }
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Create {
            name,
            parent,
            skip_post_create,
        } => {
            let config = resolve_or_exit();
            let branch_name = match resolve_target_branch(Some(&name)) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let client = xata::XataClient::new(&config);
            match client.get_branch(&branch_name) {
                Ok(Some(_)) => {
                    eprintln!("Warning: Branch '{}' already exists.", branch_name);
                    println!("{}", branch_name);
                    std::process::exit(0);
                }
                Ok(None) => {
                    let parent_branch = parent.as_deref().unwrap_or(&config.fallback_parent);
                    let parent_id = resolve_parent_id(&client, parent_branch);
                    let _ = prompt::intro("xatan create");
                    let spinner = prompt::spinner();
                    spinner.start(format!(
                        "Creating branch '{}' from '{}'...",
                        branch_name, parent_branch
                    ));

                    match client.create_branch(&branch_name, Some(&parent_id)) {
                        Ok(created_branch) => {
                            spinner.stop("Branch created.");

                            if !skip_post_create {
                                if let Some(ref command) = config
                                    .post_create
                                    .clone()
                                    .or_else(find_convention_hook_file)
                                {
                                    // Resolve connection string
                                    let mut conn_url = created_branch.connection_string.clone();
                                    if conn_url.is_none() {
                                        // Fallback fetch
                                        if let Ok(Some(fetched)) = client.get_branch(&branch_name) {
                                            conn_url = fetched.connection_string;
                                        }
                                    }

                                    if let Some(conn_str) = conn_url {
                                        let rewritten =
                                            rewrite_connection_string(&conn_str, &config.database);
                                        eprintln!("Running post-creation hook: {}", command);
                                        if let Err(e) = run_post_create_hook(
                                            command,
                                            &rewritten,
                                            &branch_name,
                                            parent_branch,
                                            &config,
                                        ) {
                                            eprintln!("Error executing post-creation hook: {}", e);
                                            std::process::exit(1);
                                        }
                                    } else {
                                        if config.post_create.is_some()
                                            || find_convention_hook_file().is_some()
                                        {
                                            eprintln!(
                                                "Warning: Skipping post-creation hook because database connection URL could not be retrieved."
                                            );
                                        }
                                    }
                                }
                            }

                            println!("{}", branch_name);
                            std::process::exit(0);
                        }
                        Err(e) => {
                            spinner.stop("Creation failed.");
                            eprintln!("API Error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::List { mine: _, all } => {
            let config = resolve_or_exit();
            let prefix = match identity::resolve_identity() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let client = xata::XataClient::new(&config);
            let mut branches = match client.list_branches() {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            };

            branches.sort_by(|a, b| {
                let a_is_mine = a.name.starts_with(&prefix);
                let b_is_mine = b.name.starts_with(&prefix);
                if a_is_mine != b_is_mine {
                    b_is_mine.cmp(&a_is_mine)
                } else {
                    a.name.cmp(&b.name)
                }
            });

            if !all {
                branches.retain(|b| b.name.starts_with(&prefix));
            }

            if branches.is_empty() {
                println!("No branches found.");
                std::process::exit(0);
            }

            let is_atty = std::io::stdout().is_terminal();
            let active_branch = resolve_target_branch(None).ok();

            let mut max_name_len = 11;
            let mut max_parent_len = 6;
            let mut max_created_len = 10;

            let mut formatted_branches = Vec::new();
            for b in &branches {
                let is_mine = b.name.starts_with(&prefix);
                let is_active = Some(&b.name) == active_branch.as_ref();
                let parent_str = b.parent_id.as_deref().unwrap_or("-").to_string();
                let created_str = b
                    .created_at
                    .as_deref()
                    .map(|s| humanize_time_ago(s))
                    .unwrap_or_else(|| "-".to_string());

                max_name_len = max_name_len.max(b.name.len());
                max_parent_len = max_parent_len.max(parent_str.len());
                max_created_len = max_created_len.max(created_str.len());

                formatted_branches.push((b, is_mine, is_active, parent_str, created_str));
            }

            let header = format!(
                "    {:width_name$} | {:width_parent$} | {:width_created$}",
                "Branch Name",
                "Parent",
                "Created At",
                width_name = max_name_len,
                width_parent = max_parent_len,
                width_created = max_created_len
            );
            let divider = "-".repeat(header.len());

            println!("{}", header);
            println!("{}", divider);

            for (b, is_mine, is_active, parent_str, created_str) in formatted_branches {
                let indicator = if is_active {
                    "[*]"
                } else if is_mine {
                    " * "
                } else {
                    "   "
                };

                let row = format!(
                    "{} {:width_name$} | {:width_parent$} | {:width_created$}",
                    indicator,
                    b.name,
                    parent_str,
                    created_str,
                    width_name = max_name_len,
                    width_parent = max_parent_len,
                    width_created = max_created_len
                );

                if is_mine && is_atty {
                    println!("\x1b[1;32m{}\x1b[0m", row);
                } else {
                    println!("{}", row);
                }
            }
        }
        Commands::Recreate {
            name,
            from,
            yes,
            skip_post_create,
        } => {
            let config = resolve_or_exit();
            let branch_name = match resolve_target_branch(name.as_deref()) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let from_parent = from.as_deref().unwrap_or(&config.fallback_parent);

            if !yes {
                let _ = prompt::intro("xatan recreate");
                let msg = format!(
                    "Are you sure you want to recreate branch '{}'? This will delete ALL its data and re-branch from '{}'.",
                    branch_name, from_parent
                );
                match prompt::prompt_confirm(&msg, true) {
                    Ok(true) => {}
                    _ => {
                        eprintln!("Operation aborted.");
                        std::process::exit(1);
                    }
                }
            }

            let client = xata::XataClient::new(&config);
            let from_parent_id = resolve_parent_id(&client, from_parent);
            let spinner = prompt::spinner();
            spinner.start(format!("Recreating '{}'...", branch_name));

            spinner.set_message("Tearing down old branch...");
            if let Err(e) = client.delete_branch(&branch_name) {
                if !e.contains("404") && !e.to_lowercase().contains("not found") {
                    spinner.stop("Teardown failed.");
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }

            spinner.set_message(format!("Cloning new branch from '{}'...", from_parent));
            match client.create_branch(&branch_name, Some(&from_parent_id)) {
                Ok(created) => {
                    spinner.stop("Recreation complete.");
                    let mut conn_url = created.connection_string.clone();
                    if conn_url.is_none() {
                        if let Ok(Some(fetched)) = client.get_branch(&branch_name) {
                            conn_url = fetched.connection_string;
                        }
                    }

                    if let Some(conn_str) = conn_url {
                        let rewritten = rewrite_connection_string(&conn_str, &config.database);
                        cache::set_cached_url(&branch_name, &rewritten);

                        if !skip_post_create {
                            if let Some(ref command) = config
                                .post_create
                                .clone()
                                .or_else(find_convention_hook_file)
                            {
                                eprintln!("Running post-creation hook: {}", command);
                                if let Err(e) = run_post_create_hook(
                                    command,
                                    &rewritten,
                                    &branch_name,
                                    from_parent,
                                    &config,
                                ) {
                                    eprintln!("Error executing post-creation hook: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    } else {
                        if !skip_post_create {
                            if config.post_create.is_some() || find_convention_hook_file().is_some()
                            {
                                eprintln!(
                                    "Warning: Skipping post-creation hook because database connection URL could not be retrieved."
                                );
                            }
                        }
                    }
                    std::process::exit(0);
                }
                Err(e) => {
                    spinner.stop("Cloning failed.");
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Delete { name, yes } => {
            let config = resolve_or_exit();
            let branch_name = match resolve_target_branch(name.as_deref()) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            if !yes {
                let _ = prompt::intro("xatan delete");
                let msg = format!(
                    "Are you sure you want to permanently delete branch '{}'?",
                    branch_name
                );
                match prompt::prompt_confirm(&msg, false) {
                    Ok(true) => {}
                    _ => {
                        eprintln!("Operation aborted.");
                        std::process::exit(1);
                    }
                }
            }

            let client = xata::XataClient::new(&config);
            let spinner = prompt::spinner();
            spinner.start(format!("Deleting branch '{}'...", branch_name));

            match client.delete_branch(&branch_name) {
                Ok(_) => {
                    spinner.stop("Branch deleted.");
                    cache::remove_cached_url(&branch_name);
                    std::process::exit(0);
                }
                Err(e) => {
                    spinner.stop("Deletion failed.");
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Shell { name } => {
            let config = resolve_or_exit();
            let branch_name = match resolve_target_branch(name.as_deref()) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            // Check cache first for sub-millisecond psql startup
            if let Some(cached_url) = cache::get_cached_url(&branch_name) {
                let err = Command::new("psql").arg(cached_url).exec();
                eprintln!("Failed to execute psql: {}", err);
                std::process::exit(1);
            }

            let client = xata::XataClient::new(&config);
            match client.get_branch(&branch_name) {
                Ok(Some(branch)) => {
                    if let Some(conn_str) = branch.connection_string {
                        let rewritten = rewrite_connection_string(&conn_str, &config.database);
                        // Save to cache
                        cache::set_cached_url(&branch_name, &rewritten);
                        let err = Command::new("psql").arg(rewritten).exec();
                        eprintln!("Failed to execute psql: {}", err);
                        std::process::exit(1);
                    } else {
                        eprintln!(
                            "Error: Branch '{}' exists but has no connection URL.",
                            branch_name
                        );
                        std::process::exit(1);
                    }
                }
                Ok(None) => {
                    eprintln!("Error: Branch '{}' does not exist.", branch_name);
                    std::process::exit(2);
                }
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Helper to resolve configurations, exiting with exit code 3 on failure
fn resolve_or_exit() -> config::ResolvedConfig {
    match config::resolve_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Authentication / Config Missing: {}", e);
            std::process::exit(3);
        }
    }
}

/// Interactive initialization session
fn run_init() -> Result<(), String> {
    let defaults = config::get_xata_defaults();

    prompt::intro("xatan init").map_err(|e| e.to_string())?;

    let org = prompt::prompt_text("Organization ID", defaults.0.as_deref())?;
    let project = prompt::prompt_text("Project ID", defaults.1.as_deref())?;
    let database = prompt::prompt_text("Database Name", defaults.2.as_deref())?;

    if org.trim().is_empty() || project.trim().is_empty() || database.trim().is_empty() {
        return Err(
            "Organization ID, Project ID, and Database Name are all required fields.".to_string(),
        );
    }

    let root =
        find_repository_root().ok_or_else(|| "Failed to resolve repository root".to_string())?;
    let config_path = root.join(".xatanrc");

    let payload = config::XatanConfig {
        org: Some(org.trim().to_string()),
        project: Some(project.trim().to_string()),
        database: Some(database.trim().to_string()),
        fallback_parent: Some("main".to_string()),
        post_create: None,
    };

    let config_json = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    std::fs::write(&config_path, config_json).map_err(|e| {
        format!(
            "Failed to write configuration file {}: {}",
            config_path.display(),
            e
        )
    })?;

    prompt::outro("Successfully initialized .xatanrc!").map_err(|e| e.to_string())?;

    Ok(())
}

/// Resolves parent branch name (e.g. "main") to its unique branch ID
fn resolve_parent_id(client: &xata::XataClient, parent_name: &str) -> String {
    if let Ok(branches) = client.list_branches() {
        if let Some(b) = branches
            .iter()
            .find(|b| b.name == parent_name || b.id == parent_name)
        {
            return b.id.clone();
        }
    }
    parent_name.to_string()
}

/// Rewrites the database name path segment in the connection URL to match XATA_DATABASE_NAME
fn rewrite_connection_string(conn_str: &str, db_name: &str) -> String {
    if let Some(scheme_idx) = conn_str.find("://") {
        let rest = &conn_str[scheme_idx + 3..];
        if let Some(slash_idx) = rest.find('/') {
            let path_and_query = &rest[slash_idx + 1..];
            let end_idx = path_and_query
                .find('?')
                .or_else(|| path_and_query.find('#'))
                .unwrap_or(path_and_query.len());
            let query_part = &path_and_query[end_idx..];
            let host_part = &rest[..slash_idx];
            let scheme = &conn_str[..scheme_idx + 3];
            return format!("{}{}/{}{}", scheme, host_part, db_name, query_part);
        }
    }
conn_str.to_string()
}

#[cfg(test)]
const MAX_ATTEMPTS: usize = 2;
#[cfg(test)]
const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_millis(10);
#[cfg(test)]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(10);

#[cfg(not(test))]
const MAX_ATTEMPTS: usize = 30;
#[cfg(not(test))]
const SLEEP_DURATION: std::time::Duration = std::time::Duration::from_secs(1);
#[cfg(not(test))]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

fn parse_host_port(connection_url: &str) -> Option<(String, u16)> {
    let scheme_idx = connection_url.find("://")?;
    let rest = &connection_url[scheme_idx + 3..];
    let end_idx = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let host_and_auth = &rest[..end_idx];

    let host_port = if let Some(at_idx) = host_and_auth.find('@') {
        &host_and_auth[at_idx + 1..]
    } else {
        host_and_auth
    };

    let (host, port) = if let Some(colon_idx) = host_port.rfind(':') {
        let port_str = &host_port[colon_idx + 1..];
        if let Ok(p) = port_str.parse::<u16>() {
            (&host_port[..colon_idx], p)
        } else {
            (host_port, 5432)
        }
    } else {
        (host_port, 5432)
    };

    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some((host.to_string(), port))
}

fn wait_for_database(connection_url: &str) -> Result<(), String> {
    let (host, port) = parse_host_port(connection_url)
        .ok_or_else(|| "Invalid database connection URL".to_string())?;

    use std::net::ToSocketAddrs;

    eprintln!("Checking database availability at {}:{}...", host, port);

    for attempt in 1..=MAX_ATTEMPTS {
        match (host.as_str(), port).to_socket_addrs() {
            Ok(addrs) => {
                let mut connected = false;
                for addr in addrs {
                    if std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok() {
                        connected = true;
                        break;
                    }
                }
                if connected {
                    return Ok(());
                }
            }
            Err(_) => {}
        }

        if attempt < MAX_ATTEMPTS {
            eprintln!(
                "Database not ready yet, retrying in 1s (attempt {}/{})...",
                attempt, MAX_ATTEMPTS
            );
            std::thread::sleep(SLEEP_DURATION);
        }
    }

    Err(format!(
        "Database at {}:{} did not become ready after {} attempts",
        host, port, MAX_ATTEMPTS
    ))
}

/// Executes the post-creation hook subprocess in a platform-appropriate shell.
/// Ensures standard output of the child is forwarded to standard error of the parent
/// to avoid standard output pollution, while standard error is inherited directly.
fn run_post_create_hook(
    command_str: &str,
    connection_url: &str,
    branch_name: &str,
    parent_branch: &str,
    config: &config::ResolvedConfig,
) -> Result<(), String> {
    wait_for_database(connection_url)?;

    use std::io::IsTerminal;
    let is_tty = std::io::stderr().is_terminal() && std::io::stdout().is_terminal();

    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd.exe");
        c.arg("/C").arg(command_str);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(command_str);
        c
    };

    cmd.env("DATABASE_URL", connection_url)
        .env("XATA_DATABASE_URL", connection_url)
        .env("XATAN_BRANCH_NAME", branch_name)
        .env("XATAN_PARENT_BRANCH", parent_branch)
        .env("XATA_ORG_ID", &config.org)
        .env("XATA_PROJECT_ID", &config.project)
        .env("XATA_DATABASE_NAME", &config.database);

    if is_tty {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn hook: {}", e))?;

        // Forward child's stdout to stderr of the parent
        let stdout_thread = if let Some(stdout) = child.stdout.take() {
            Some(std::thread::spawn(move || {
                use std::io::{BufRead, BufReader, Write};
                let mut reader = BufReader::new(stdout);
                let mut line = Vec::new();
                while let Ok(n) = reader.read_until(b'\n', &mut line) {
                    if n == 0 {
                        break;
                    }
                    let mut err = std::io::stderr();
                    let _ = err.write_all(&line);
                    let _ = err.flush();
                    line.clear();
                }
            }))
        } else {
            None
        };

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for hook: {}", e))?;
        if let Some(t) = stdout_thread {
            let _ = t.join();
        }

        if !status.success() {
            let code = status.code().unwrap_or(1);
            return Err(format!("Hook exited with non-zero status code: {}", code));
        }
    } else {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn hook: {}", e))?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let stdout_thread = stdout_handle.map(|stdout| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let mut r = stdout;
                let _ = r.read_to_end(&mut buf);
                buf
            })
        });

        let stderr_thread = stderr_handle.map(|stderr| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let mut r = stderr;
                let _ = r.read_to_end(&mut buf);
                buf
            })
        });

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for hook: {}", e))?;

        let stdout_bytes = if let Some(t) = stdout_thread {
            t.join().unwrap_or_default()
        } else {
            Vec::new()
        };

        let stderr_bytes = if let Some(t) = stderr_thread {
            t.join().unwrap_or_default()
        } else {
            Vec::new()
        };

        if !status.success() {
            use std::io::Write;
            let mut err = std::io::stderr();
            if !stdout_bytes.is_empty() {
                let _ = err.write_all(&stdout_bytes);
            }
            if !stderr_bytes.is_empty() {
                let _ = err.write_all(&stderr_bytes);
            }
            let _ = err.flush();

            let code = status.code().unwrap_or(1);
            return Err(format!("Hook exited with non-zero status code: {}", code));
        }
    }

    Ok(())
}

pub fn humanize_time_ago(date_str: &str) -> String {
    let parsed = match chrono::DateTime::parse_from_rfc3339(date_str) {
        Ok(dt) => dt,
        Err(_) => return date_str.to_string(),
    };
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(parsed.with_timezone(&chrono::Utc));
    let secs = duration.num_seconds();

    if secs < 0 {
        return "just now".to_string();
    }
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        if mins == 1 {
            return "1 minute ago".to_string();
        } else {
            return format!("{} minutes ago", mins);
        }
    }
    let hours = mins / 60;
    if hours < 24 {
        if hours == 1 {
            return "1 hour ago".to_string();
        } else {
            return format!("{} hours ago", hours);
        }
    }
    let days = hours / 24;
    if days < 30 {
        if days == 1 {
            return "1 day ago".to_string();
        } else {
            return format!("{} days ago", days);
        }
    }
    let months = days / 30;
    if months < 12 {
        if months == 1 {
            return "1 month ago".to_string();
        } else {
            return format!("{} months ago", months);
        }
    }
    let years = days / 365;
    if years == 1 {
        "1 year ago".to_string()
    } else {
        format!("{} years ago", years)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_humanize_time_ago() {
        use chrono::{Duration, Utc};

        let now = Utc::now();

        // 10 seconds ago -> "just now"
        let t1 = (now - Duration::seconds(10)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t1), "just now");

        // 45 seconds ago -> "just now"
        let t2 = (now - Duration::seconds(45)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t2), "just now");

        // 2 minutes ago -> "2 minutes ago"
        let t3 = (now - Duration::minutes(2)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t3), "2 minutes ago");

        // 1 hour ago -> "1 hour ago"
        let t4 = (now - Duration::hours(1)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t4), "1 hour ago");

        // 3 hours ago -> "3 hours ago"
        let t5 = (now - Duration::hours(3)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t5), "3 hours ago");

        // 1 day ago -> "1 day ago"
        let t6 = (now - Duration::days(1)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t6), "1 day ago");

        // 5 days ago -> "5 days ago"
        let t7 = (now - Duration::days(5)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t7), "5 days ago");

        // 1 month ago (approx 30 days) -> "1 month ago"
        let t8 = (now - Duration::days(30)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t8), "1 month ago");

        // 3 months ago (approx 90 days) -> "3 months ago"
        let t9 = (now - Duration::days(90)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t9), "3 months ago");

        // 1 year ago (approx 365 days) -> "1 year ago"
        let t10 = (now - Duration::days(365)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t10), "1 year ago");

        // 2 years ago (approx 730 days) -> "2 years ago"
        let t11 = (now - Duration::days(730)).to_rfc3339();
        assert_eq!(humanize_time_ago(&t11), "2 years ago");

        // Invalid format fallback
        assert_eq!(humanize_time_ago("invalid"), "invalid");
    }

    #[test]
    fn test_resolve_target_branch_already_prefixed() {
        unsafe {
            std::env::set_var("XATAN_PREFIX", "me-fiws-net");
        }

        // 1. Passing just suffix
        let res1 = resolve_target_branch(Some("nkotxwxwpswz")).unwrap();
        assert_eq!(res1, "me-fiws-net-nkotxwxwpswz");

        // 2. Passing already prefixed string
        let res2 = resolve_target_branch(Some("me-fiws-net-nkotxwxwpswz")).unwrap();
        assert_eq!(res2, "me-fiws-net-nkotxwxwpswz");
    }

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            parse_host_port("postgresql://user:pass@localhost:5432/mydb"),
            Some(("localhost".to_string(), 5432))
        );
        assert_eq!(
            parse_host_port("postgresql://localhost/mydb"),
            Some(("localhost".to_string(), 5432))
        );
        assert_eq!(
            parse_host_port("postgresql://user:pass@[::1]:5432/mydb"),
            Some(("::1".to_string(), 5432))
        );
        assert_eq!(
            parse_host_port("postgresql://[::1]/mydb"),
            Some(("::1".to_string(), 5432))
        );
        assert_eq!(
            parse_host_port("postgres://some-host:1234?sslmode=require"),
            Some(("some-host".to_string(), 1234))
        );
        assert_eq!(
            parse_host_port("invalid-url"),
            None
        );
    }

    #[test]
    fn test_wait_for_database_success() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let conn_url = format!("postgresql://user:pass@127.0.0.1:{}/mydb", port);
        assert!(wait_for_database(&conn_url).is_ok());
    }

    #[test]
    fn test_wait_for_database_failure() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let conn_url = format!("postgresql://user:pass@127.0.0.1:{}/mydb", port);
        assert!(wait_for_database(&conn_url).is_err());
    }

    #[test]
    fn test_run_post_create_hook_success() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let conn_url = format!("postgresql://user:pass@127.0.0.1:{}/mydb", port);

        let command = if cfg!(windows) {
            "echo [HOOK_TEST_OK] %DATABASE_URL%"
        } else {
            "echo [HOOK_TEST_OK] $DATABASE_URL"
        };
        let config = config::ResolvedConfig {
            org: "test-org".to_string(),
            project: "test-proj".to_string(),
            database: "test-db".to_string(),
            fallback_parent: "main".to_string(),
            api_key: "test-key".to_string(),
            post_create: None,
        };
        let res = run_post_create_hook(
            command,
            &conn_url,
            "test-branch",
            "main",
            &config,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_run_post_create_hook_failure() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let conn_url = format!("postgresql://user:pass@127.0.0.1:{}/mydb", port);

        let command = if cfg!(windows) { "exit 42" } else { "exit 42" };
        let config = config::ResolvedConfig {
            org: "test-org".to_string(),
            project: "test-proj".to_string(),
            database: "test-db".to_string(),
            fallback_parent: "main".to_string(),
            api_key: "test-key".to_string(),
            post_create: None,
        };
        let res = run_post_create_hook(
            command,
            &conn_url,
            "test-branch",
            "main",
            &config,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("42"));
    }
}
