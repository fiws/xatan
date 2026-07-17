use std::io;

/// Displays the intro banner for a CLI prompt session on stderr.
pub fn intro(title: &str) -> io::Result<()> {
    cliclack::intro(title)
}

/// Displays the outro banner for a CLI prompt session on stderr.
pub fn outro(message: &str) -> io::Result<()> {
    cliclack::outro(message)
}

/// Displays an interactive text input prompt on stderr and returns the input.
/// Maps I/O errors or user cancels (Ctrl+C) to a descriptive error String.
pub fn prompt_text(message: &str, default: Option<&str>) -> Result<String, String> {
    let mut p = cliclack::input(message);
    if let Some(def) = default
        && !def.is_empty()
    {
        p = p.default_input(def);
    }
    p.interact().map_err(|e| {
        if e.kind() == io::ErrorKind::Interrupted {
            "Operation cancelled.".to_string()
        } else {
            format!("Prompt failed: {}", e)
        }
    })
}

/// Displays an interactive yes/no confirmation prompt on stderr and returns the choice.
/// Maps I/O errors or user cancels (Ctrl+C) to a descriptive error String.
pub fn prompt_confirm(message: &str, default: bool) -> Result<bool, String> {
    cliclack::confirm(message)
        .initial_value(default)
        .interact()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::Interrupted {
                "Operation cancelled.".to_string()
            } else {
                format!("Prompt failed: {}", e)
            }
        })
}

/// Creates and returns a Cliclack Spinner.
pub fn spinner() -> cliclack::ProgressBar {
    cliclack::spinner()
}

/// Creates and returns a Cliclack ProgressBar with a given total.
pub fn progress_bar(total: usize) -> cliclack::ProgressBar {
    cliclack::progress_bar(total as u64)
}

/// Updates the terminal/OS progress integration (OSC 9;4) on stderr.
/// State:
/// * 0: Reset/Clear
/// * 1: Normal progress
/// * 2: Error
/// * 3: Indeterminate
/// * 4: Paused
pub fn update_terminal_progress(state: u8, percentage: u8) {
    use std::io::IsTerminal;
    use std::io::Write;
    if io::stderr().is_terminal() {
        let _ = write!(io::stderr(), "\x1b]9;4;{};{}\x1b\\", state, percentage);
        let _ = io::stderr().flush();
    }
}

struct RichTheme;

impl cliclack::Theme for RichTheme {
    fn default_progress_template(&self) -> String {
        "{msg} [{elapsed_precise}] {bar:30.#F92672/#3a3a3a} ({pos}/{len})".into()
    }

    fn default_download_template(&self) -> String {
        "{msg} [{elapsed_precise}] [{bar:30.#F92672/#3a3a3a}] {bytes}/{total_bytes} ({eta})".into()
    }

    fn progress_chars(&self) -> String {
        "━━".into()
    }
}

/// Initializes the custom terminal progress bar theme.
pub fn init_theme() {
    cliclack::set_theme(RichTheme);
}

