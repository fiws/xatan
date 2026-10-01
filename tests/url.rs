use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const FEATURE_URL: &str = "postgresql://user:secret@feature.test/database?sslmode=require";
const MAIN_URL: &str = "postgresql://user:secret@main.test/database?sslmode=require";
const NOBRANCH_URL: &str = "postgresql://user:secret@nobranch.test/database?sslmode=require";

struct Repository {
    root: PathBuf,
    repo: PathBuf,
    git: PathBuf,
    cache_path: PathBuf,
}

impl Repository {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "xatan-url-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo = root.join("repo");
        // macOS ignores XDG_CACHE_HOME; share its HOME-based cache path on all platforms.
        for directory in [
            &repo,
            &root.join("home"),
            &root.join("bin"),
            &root.join("home/Library/Caches/xatan"),
        ] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .unwrap()
            .canonicalize()
            .unwrap();
        // Keep Jujutsu out of PATH so these scenarios exercise real Git behavior.
        std::os::unix::fs::symlink(&git, root.join("bin/git")).unwrap();

        let canonical_repo = repo.canonicalize().unwrap();
        let mut hash = 0xcbf29ce484222325u64;
        for byte in canonical_repo.to_string_lossy().bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        let cache_path = root.join(format!("home/Library/Caches/xatan/repo_{hash:016x}.json"));
        let repository = Self {
            root,
            repo,
            git,
            cache_path,
        };
        repository.git(&["init", "--quiet", "--initial-branch=main", "--template="]);
        repository.git(&["config", "user.email", "test@example.test"]);
        repository.git(&["config", "user.name", "Test User"]);
        std::fs::write(repository.repo.join("conflict.txt"), "base\n").unwrap();
        repository.git(&["add", "conflict.txt"]);
        repository.git(&["commit", "--quiet", "-m", "base"]);
        repository.git(&["checkout", "--quiet", "-b", "feature"]);
        repository
    }

    fn command(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .current_dir(&self.repo)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", self.root.join("bin"))
            .env("XDG_CACHE_HOME", self.root.join("home/Library/Caches"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_EDITOR", ":");
        command
    }

    fn git_output(&self, args: &[&str]) -> Output {
        self.command(&self.git).args(args).output().unwrap()
    }

    fn git(&self, args: &[&str]) {
        let output = self.git_output(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn seed_cache(&self) {
        // Existing credential caches must remain usable without a cached VCS ref.
        let cache = serde_json::json!({
            "version": 2,
            "branches": {
                "test-user-feature": FEATURE_URL,
                "test-user-main": MAIN_URL,
                "test-user-nobranch": NOBRANCH_URL
            }
        });
        std::fs::write(&self.cache_path, cache.to_string()).unwrap();
    }

    fn assert_url(&self, name: Option<&str>, expected: &str) {
        let nested = self.repo.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let mut command = self.command(Path::new(env!("CARGO_BIN_EXE_xatan")));
        command
            .current_dir(nested)
            .args(["url", "--no-create"])
            .env("XATA_API_KEY", "test-key")
            .env("XATA_ORG_ID", "test-org")
            .env("XATA_PROJECT_ID", "test-project")
            .env("XATA_DATABASE_NAME", "database")
            .env("XATAN_PREFIX", "test-user");
        if let Some(name) = name {
            command.arg(name);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{expected}\n")
        );
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn url_reuses_last_vcs_ref_during_rebase_and_refreshes_afterward() {
    let repository = Repository::new();
    std::fs::write(repository.repo.join("conflict.txt"), "feature\n").unwrap();
    repository.git(&["commit", "--quiet", "-am", "feature"]);
    repository.git(&["checkout", "--quiet", "main"]);
    std::fs::write(repository.repo.join("conflict.txt"), "main\n").unwrap();
    repository.git(&["commit", "--quiet", "-am", "main"]);
    repository.git(&["checkout", "--quiet", "feature"]);
    repository.seed_cache();
    repository.assert_url(None, FEATURE_URL);

    let rebase = repository.git_output(&["rebase", "main"]);
    assert!(!rebase.status.success());
    assert!(repository.repo.join(".git/rebase-merge").is_dir());
    assert!(
        repository
            .git_output(&["branch", "--show-current"])
            .stdout
            .is_empty()
    );
    repository.assert_url(None, FEATURE_URL);
    repository.assert_url(Some("main"), MAIN_URL);
    repository.assert_url(None, FEATURE_URL);

    repository.git(&["rebase", "--abort"]);
    repository.git(&["checkout", "--quiet", "main"]);
    repository.assert_url(None, MAIN_URL);
    repository.git(&["checkout", "--quiet", "--detach"]);
    repository.assert_url(None, MAIN_URL);
}

#[test]
fn url_without_vcs_history_keeps_nobranch_fallback_and_ignores_explicit_names() {
    let repository = Repository::new();
    repository.seed_cache();
    repository.git(&["checkout", "--quiet", "--detach"]);
    repository.assert_url(Some("feature"), FEATURE_URL);
    repository.assert_url(None, NOBRANCH_URL);
}
