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
