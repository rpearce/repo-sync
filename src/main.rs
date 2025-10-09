use clap::Command;

mod commands;
mod git;

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
            commands::clone::run(file, out);
        }
        Some(("sync", sub_m)) => {
            let file = sub_m.get_one::<String>("file").unwrap();
            let out = sub_m.get_one::<String>("out").unwrap();
            commands::sync::run(file, out);
        }
        _ => unreachable!("Subcommand required"),
    }
}
