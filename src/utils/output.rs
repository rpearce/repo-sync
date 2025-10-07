use crate::config::Config;

/// Print a message only when verbose mode is enabled.
///
/// # Arguments
/// * `config` - Configuration containing verbose flag
/// * `message` - Message to print
pub fn verbose_println(config: &Config, message: &str) {
    if config.verbose {
        println!("{}", message);
    }
}


/// Print repository operation status messages.
pub struct RepoStatusPrinter<'a> {
    config: &'a Config,
}

impl<'a> RepoStatusPrinter<'a> {
    /// Create a new status printer.
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    /// Print repository clone success message.
    pub fn cloned(&self, repo_name: &str) {
        verbose_println(self.config, &format!("Cloned {} successfully", repo_name));
    }

    /// Print repository sync success message.
    pub fn synced(&self, repo_name: &str) {
        verbose_println(self.config, &format!("Synced {} successfully", repo_name));
    }

    /// Print repository skip message.
    pub fn skipped(&self, repo_name: &str, reason: &str) {
        verbose_println(self.config, &format!("Skipping {}, {}", repo_name, reason));
    }

    /// Print operation summary.
    pub fn summary(&self, operation: &str, total: usize, target_dir: &str) {
        verbose_println(
            self.config,
            &format!("{} {} repositories in {}", operation, total, target_dir),
        );
    }

    /// Print branch merge skip message.
    pub fn skip_merge(&self, branch: &str, reason: &str) {
        verbose_println(self.config, &format!("Skipping merge on {} ({})", branch, reason));
    }

    /// Print final operation summary (always visible).
    pub fn final_summary(&self, operation: &str, failed: usize, total: usize) {
        if failed > 0 {
            println!("Warning: Failed to {} {} out of {} repositories", operation.to_lowercase(), failed, total);
        } else {
            println!("Successfully {}d all {} repositories", operation.to_lowercase(), total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config(verbose: bool) -> Config {
        Config::new("test.txt", "/tmp").with_verbose(verbose)
    }

    #[test]
    fn test_verbose_println_enabled() {
        let config = test_config(true);
        // This would print in verbose mode (can't easily test output in unit tests)
        verbose_println(&config, "test message");
    }

    #[test]
    fn test_verbose_println_disabled() {
        let config = test_config(false);
        // This would not print (can't easily test output in unit tests)
        verbose_println(&config, "test message");
    }

    #[test]
    fn test_repo_status_printer_creation() {
        let config = test_config(true);
        let _printer = RepoStatusPrinter::new(&config);
        // Just testing that it constructs without error
    }
}