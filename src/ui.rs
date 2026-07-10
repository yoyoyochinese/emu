use console::style;

pub fn info(label: &str, value: &str) {
    let prefix = style("›").dim();
    let val = style(value).cyan();
    println!("{prefix} {label}: {val}");
}

pub fn success(msg: &str) {
    let prefix = style("✓").green().bold();
    println!("{prefix} {msg}");
}

pub fn error(msg: &str) {
    let prefix = style("✗").red();
    eprintln!("{prefix} {msg}");
}

pub fn build_failed(msg: &str) {
    let text = style(msg).red().bold();
    eprintln!("{text}");
}

pub fn colorize_logcat(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }

    let priority = line.chars().next().unwrap_or(' ');
    let rest = &line[1..];

    let styled = match priority {
        'V' => style(priority.to_string()).dim(),
        'D' => style(priority.to_string()).blue(),
        'I' => style(priority.to_string()).green(),
        'W' => style(priority.to_string()).yellow(),
        'E' => style(priority.to_string()).red().bold(),
        'F' => style(priority.to_string()).red().bold().on_white(),
        _ => return line.to_owned(),
    };

    format!("{styled}{rest}")
}

pub fn spinner_style() -> indicatif::ProgressStyle {
    indicatif::ProgressStyle::with_template("{spinner:.green} {prefix:.bold} {wide_msg}")
        .unwrap()
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorize_logcat_verbose() {
        let result = colorize_logcat("V/Tag(123): message");
        assert!(result.contains('V'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_debug() {
        let result = colorize_logcat("D/Tag(123): message");
        assert!(result.contains('D'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_info() {
        let result = colorize_logcat("I/Tag(123): message");
        assert!(result.contains('I'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_warn() {
        let result = colorize_logcat("W/Tag(123): message");
        assert!(result.contains('W'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_error() {
        let result = colorize_logcat("E/Tag(123): message");
        assert!(result.contains('E'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_fatal() {
        let result = colorize_logcat("F/Tag(123): message");
        assert!(result.contains('F'));
        assert!(result.contains("/Tag(123): message"));
    }

    #[test]
    fn colorize_logcat_empty() {
        assert_eq!(colorize_logcat(""), "");
    }

    #[test]
    fn colorize_logcat_non_priority() {
        let result = colorize_logcat("some random line");
        assert_eq!(result, "some random line");
    }
}
