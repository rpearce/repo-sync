use clap::Command;

mod commands;
mod config;
mod git;

use config::Config;

/// Entry point for the repo-sync CLI.
///
/// Defines two subcommands:
/// - `clone`: clone repositories listed in a file into an output directory
/// - `sync`: update existing repositories (pull + branch fast-forward)
///
/// Each subcommand requires:
/// - `--file` / `-f`: path to a text file containing one repo URL per line
/// - `--out` / `-o`: local directory where repositories are cloned/synced
///
/// Example:
///   repo-sync clone -f repos.txt -o ./repos
///   repo-sync sync -f repos.txt -o ./repos
fn main() {
    let matches = Command::new("repo-sync")
        .version("0.1.0")
        .author("Robert Pearce <me@robertwpearce.com>")
        .about("Clone or sync multiple git repositories from a file")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(commands::clone::command())
        .subcommand(commands::sync::command())
        .get_matches();

    match matches.subcommand() {
        Some(("clone", sub_m)) => {
            let file = sub_m.get_one::<String>("file").unwrap();
            let out = sub_m.get_one::<String>("out").unwrap();
            let verbose = sub_m.get_flag("verbose");
            let config = Config::new(file, out).with_verbose(verbose);
            commands::clone::run(&config);
        }
        Some(("sync", sub_m)) => {
            let file = sub_m.get_one::<String>("file").unwrap();
            let out = sub_m.get_one::<String>("out").unwrap();
            let verbose = sub_m.get_flag("verbose");
            let config = Config::new(file, out).with_verbose(verbose);
            commands::sync::run(&config);
        }
        _ => unreachable!("Subcommand required"),
    };
}
