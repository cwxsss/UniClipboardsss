//! Styled CLI output helpers wrapping `dialoguer`, `console`, and `indicatif`.

use console::{style, Key, Style, Term};
use dialoguer::{Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};

// ── Colour palette ──────────────────────────────────────────────────

fn cyan() -> Style {
    Style::new().cyan()
}

fn green() -> Style {
    Style::new().green()
}

fn yellow() -> Style {
    Style::new().yellow()
}

fn red() -> Style {
    Style::new().red()
}

fn dim() -> Style {
    Style::new().dim()
}

fn bold() -> Style {
    Style::new().bold()
}

// ── Structured messages ─────────────────────────────────────────────

/// Print a section header: `◆  Title`
pub fn header(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(
        "\n {} {}",
        style("◆").cyan().bold(),
        bold().apply_to(text)
    ));
}

/// Print a step label: `◇  Label`
pub fn step(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(" {} {}", style("◇").cyan(), text));
}

/// Print a success line: `✓  Message`
pub fn success(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(" {}  {}", green().apply_to("✓"), text));
}

/// Print a warning line: `⚠  Message`
pub fn warn(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(" {}  {}", yellow().apply_to("⚠"), text));
}

/// Print an error line: `✗  Message`
pub fn error(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(" {}  {}", red().apply_to("✗"), text));
}

/// Print an info/detail line with dim prefix.
pub fn info(label: &str, value: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(
        " {}  {} {}",
        dim().apply_to("│"),
        dim().apply_to(format!("{label}:")),
        value,
    ));
}

/// Print a dim separator bar.
pub fn bar() {
    let term = Term::stderr();
    let _ = term.write_line(&format!(" {}", dim().apply_to("│")));
}

/// Print a closing corner: `└  Message`
pub fn end(text: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(
        " {} {}",
        green().apply_to("└"),
        green().apply_to(text),
    ));
}

// ── Interactive prompts ─────────────────────────────────────────────

/// Show a Select prompt and return the chosen index.
pub fn select(prompt: &str, items: &[String]) -> Result<usize, String> {
    Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .interact_on(&Term::stderr())
        .map_err(|e| format!("selection cancelled: {e}"))
}

/// Show a Confirm prompt (y/n).
pub fn confirm(prompt: &str, default: bool) -> Result<bool, String> {
    Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact_on(&Term::stderr())
        .map_err(|e| format!("confirmation cancelled: {e}"))
}

/// Show a masked password prompt (displays `•` per character).
pub fn password(prompt: &str) -> Result<String, String> {
    read_masked_password(prompt)
}

/// Show a masked password prompt with confirmation.
pub fn password_with_confirm(prompt: &str, confirm_prompt: &str) -> Result<String, String> {
    loop {
        let p1 = read_masked_password(prompt)?;
        let p2 = read_masked_password(confirm_prompt)?;
        if p1 == p2 {
            return Ok(p1);
        }
        let term = Term::stderr();
        let _ = term.write_line(&format!(
            " {}  {}",
            red().apply_to("✗"),
            "Passphrases do not match, try again"
        ));
    }
}

const MASK_CHAR: &str = "•";

/// Read a password with masked feedback on stderr.
///
/// Renders as two lines during input:
/// ```text
///  ◇ Prompt label
///  │ ••••
/// ```
/// Collapses to one line after Enter:
/// ```text
///  ◇ Prompt label ••••••••
/// ```
fn read_masked_password(prompt: &str) -> Result<String, String> {
    let term = Term::stderr();

    // Line 1: prompt label
    let _ = term.write_line(&format!(" {} {}", style("◇").cyan(), prompt));
    // Line 2: input line with bar prefix + cursor
    let bar_prefix = format!(" {}  ", dim().apply_to("│"));
    let _ = term.write_str(&bar_prefix);
    let _ = term.flush();

    let mut input = String::new();
    loop {
        let key = term
            .read_key()
            .map_err(|e| format!("password input failed: {e}"))?;
        match key {
            Key::Enter => {
                // Clear the two live lines (input line + prompt line)
                let _ = term.clear_line();
                let _ = term.move_cursor_up(1);
                let _ = term.clear_line();
                // Rewrite as single collapsed line
                let mask: String = MASK_CHAR.repeat(input.len());
                let _ = term.write_line(&format!(
                    " {} {} {}",
                    style("◇").cyan(),
                    prompt,
                    dim().apply_to(mask),
                ));
                return Ok(input);
            }
            Key::Backspace => {
                if !input.is_empty() {
                    input.pop();
                    let _ = term.write_str("\x08 \x08");
                    let _ = term.flush();
                }
            }
            Key::Char(c) => {
                input.push(c);
                let _ = term.write_str(MASK_CHAR);
                let _ = term.flush();
            }
            Key::Escape => {
                // Clear live lines
                let _ = term.clear_line();
                let _ = term.move_cursor_up(1);
                let _ = term.clear_line();
                return Err("password input cancelled".to_string());
            }
            _ => {}
        }
    }
}

// ── Spinner ─────────────────────────────────────────────────────────

/// Create and start a spinner with the given message.
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["◒", "◐", "◓", "◑"])
            .template(" {spinner} {msg}")
            .expect("valid spinner template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(120));
    pb
}

/// Finish spinner with a success message.
pub fn spinner_finish_success(pb: &ProgressBar, message: &str) {
    pb.set_style(
        ProgressStyle::default_spinner()
            .template(&format!(" {}  {{msg}}", Style::new().green().apply_to("✓")))
            .expect("valid template"),
    );
    pb.finish_with_message(message.to_string());
}

/// Finish spinner with an error message.
pub fn spinner_finish_error(pb: &ProgressBar, message: &str) {
    pb.set_style(
        ProgressStyle::default_spinner()
            .template(&format!(" {}  {{msg}}", Style::new().red().apply_to("✗")))
            .expect("valid template"),
    );
    pb.finish_with_message(message.to_string());
}

// ── Identity banner ─────────────────────────────────────────────────

/// Print a styled identity banner for setup flows.
pub fn identity_banner(profile: &str, mode: &str, device: &str, peer_id: &str) {
    bar();
    info("Profile", profile);
    info("Mode", mode);
    info("Device", device);
    info("Peer ID", &truncate_peer_id(peer_id));
    bar();
}

fn truncate_peer_id(peer_id: &str) -> String {
    if peer_id.len() > 16 {
        format!("{}…", &peer_id[..16])
    } else {
        peer_id.to_string()
    }
}

// ── Verification code display ───────────────────────────────────────

/// Display a verification code prominently.
pub fn verification_code(code: &str) {
    let term = Term::stderr();
    let _ = term.write_line(&format!(
        " {}  Verification code: {}",
        dim().apply_to("│"),
        cyan().bold().apply_to(code),
    ));
}
