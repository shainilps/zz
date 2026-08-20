mod helper;
mod storage;

use std::process::Command;

#[derive(Debug)]
enum Cmd {
    Add {
        path: String,
        name: String,
        tag: String,
        create_tag: bool,
    },
    Rm {
        name: String,
    },
    Run {
        tag: String,
    },
    Kill {
        tag: String,
    },
    Status {
        tag: Option<String>,
        all: bool,
    },
    List {
        tags: Vec<String>,
    },
    TagAdd {
        name: String,
    },
    TagRm {
        name: String,
        force: bool,
    },
    Help,
}

const HELP: &str = "\
zz - tmux session manager

usage:
  zz add <path> <name> <tag> [--create-tag]   register a directory
  zz rm <name>                                remove an entry
  zz run <tag>                                start every session in the tag
  zz kill <tag>                               kill the tag's sessions
  zz status [tag] [--all]                     report running sessions, --all for stopped too
  zz list [tag...]                            list what is registered
  zz tag add <name>                           create a tag
  zz tag rm <name> [--force]                  delete a tag, --force if not empty

tags are never created implicitly, pass --create-tag to add a new one";

fn parse(args: &[String]) -> Result<Cmd, String> {
    if args.is_empty() {
        return Err("command required, check zz --help".to_string());
    }

    let rest = &args[1..];

    match args[0].as_str() {
        "add" => {
            let mut create_tag = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--create-tag" {
                    create_tag = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 3 {
                return Err("usage: add <path> <name> <tag> [--create-tag]".to_string());
            }

            Ok(Cmd::Add {
                path: positional[0].clone(),
                name: positional[1].clone(),
                tag: positional[2].clone(),
                create_tag,
            })
        }
        "rm" => {
            if rest.len() != 1 {
                return Err("usage: rm <name>".to_string());
            }
            Ok(Cmd::Rm {
                name: rest[0].clone(),
            })
        }
        "run" => {
            if rest.len() != 1 {
                return Err("usage: run <tag>".to_string());
            }
            Ok(Cmd::Run {
                tag: rest[0].clone(),
            })
        }
        "kill" => {
            if rest.len() != 1 {
                return Err("usage: kill <tag>".to_string());
            }
            Ok(Cmd::Kill {
                tag: rest[0].clone(),
            })
        }
        "status" => {
            let mut all = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--all" {
                    all = true;
                } else {
                    positional.push(a.clone());
                }
            }

            match positional.len() {
                0 => Ok(Cmd::Status { tag: None, all }),
                1 => Ok(Cmd::Status {
                    tag: Some(positional[0].clone()),
                    all,
                }),
                _ => Err("usage: status [tag] [--all]".to_string()),
            }
        }
        "list" => Ok(Cmd::List {
            tags: rest.to_vec(),
        }),
        "tag" => {
            let mut force = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--force" {
                    force = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 2 {
                return Err("usage: tag add <name> | tag rm <name> [--force]".to_string());
            }

            match positional[0].as_str() {
                "add" => Ok(Cmd::TagAdd {
                    name: positional[1].clone(),
                }),
                "rm" => Ok(Cmd::TagRm {
                    name: positional[1].clone(),
                    force,
                }),
                other => Err(format!(
                    "unknown tag subcommand '{other}', usage: tag add <name> | tag rm <name> [--force]"
                )),
            }
        }
        "--help" | "-h" | "help" => Ok(Cmd::Help),
        other => Err(format!("command not valid: '{other}', check zz --help")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 1 {
        eprint!("impossible state lol");
    }

    let command = match parse(&args[1..]) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    };
    run(command);
}

fn require_tag(registry: &storage::Registry, tag: &str) {
    if registry.tags.iter().any(|t| t == tag) {
        return;
    }

    let near = registry
        .tags
        .iter()
        .map(|t| (helper::edit_distance(t, tag), t))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d);

    match near {
        Some((_, t)) => eprintln!("tag '{tag}' does not exist, did you mean '{t}'?"),
        None => eprintln!("tag '{tag}' does not exist"),
    }
    std::process::exit(1);
}

fn run(command: Cmd) {
    match command {
        Cmd::Add {
            path,
            name,
            tag,
            create_tag,
        } => {
            let mut registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            if !registry.tags.contains(&tag) {
                if create_tag {
                    registry.tags.push(tag.clone());
                } else {
                    eprintln!("tag '{tag}' does not exist, use --create-tag to create it");
                    std::process::exit(1);
                }
            }

            let canonical_path = match std::fs::canonicalize(&path) {
                Ok(p) => p.display().to_string(),
                Err(e) => {
                    eprintln!("failed to resolve path '{path}': {e}");
                    std::process::exit(1);
                }
            };

            if let Some(existing) = registry.entries.iter_mut().find(|e| e.name == name) {
                existing.path = canonical_path;
                existing.tags = vec![tag];
            } else {
                registry.entries.push(storage::Entry {
                    name,
                    path: canonical_path,
                    tags: vec![tag],
                });
            }

            if let Err(e) = storage::save(&registry) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }

        Cmd::Rm { name } => {
            let mut registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            let before = registry.entries.len();
            registry.entries.retain(|e| e.name != name);

            if registry.entries.len() == before {
                eprintln!("no entry named '{name}'");
                std::process::exit(1);
            }

            if let Err(e) = storage::save(&registry) {
                eprintln!("{e}");
                std::process::exit(1);
            }

            println!("removed {name}");
        }

        Cmd::Run { tag } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            require_tag(&registry, &tag);

            let mut failed = false;

            for entry in registry.entries.iter().filter(|e| e.tags.contains(&tag)) {
                let target = format!("={}", entry.name);

                let running = Command::new("tmux")
                    .args(["has-session", "-t", &target])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if running {
                    let live = Command::new("tmux")
                        .args([
                            "display-message",
                            "-p",
                            "-t",
                            &format!("={}:", entry.name),
                            "#{session_path}",
                        ])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();

                    if live != entry.path {
                        println!(
                            "skipped {}: running at {live}, registry says {}",
                            entry.name, entry.path
                        );
                    }
                    continue;
                }

                let started = Command::new("tmux")
                    .args(["new-session", "-d", "-s", &entry.name, "-c", &entry.path])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if started {
                    println!("started {}", entry.name);
                } else {
                    eprintln!("failed to start {}", entry.name);
                    failed = true;
                }
            }

            if failed {
                std::process::exit(1);
            }
        }

        Cmd::Kill { tag } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            require_tag(&registry, &tag);

            for entry in registry.entries.iter().filter(|e| e.tags.contains(&tag)) {
                let target = format!("={}", entry.name);

                let running = Command::new("tmux")
                    .args(["has-session", "-t", &target])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if !running {
                    continue;
                }

                if let Ok(o) = Command::new("tmux")
                    .args(["list-panes", "-s", "-t", &target, "-F", "#{pane_pid}"])
                    .output()
                {
                    for pid in String::from_utf8_lossy(&o.stdout).lines() {
                        let children =
                            std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                                .unwrap_or_default();

                        for child in children.split_whitespace() {
                            let name = std::fs::read_to_string(format!("/proc/{child}/comm"))
                                .unwrap_or_default();
                            eprintln!("warning: {} is running {}", entry.name, name.trim());
                        }
                    }
                }

                let killed = Command::new("tmux")
                    .args(["kill-session", "-t", &target])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if killed {
                    println!("killed {}", entry.name);
                } else {
                    eprintln!("failed to kill {}", entry.name);
                }
            }
        }

        Cmd::Status { tag, all } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            if let Some(t) = &tag {
                require_tag(&registry, t);
            }

            let mut all_up = true;
            let mut hidden = 0;

            for entry in registry
                .entries
                .iter()
                .filter(|e| tag.as_ref().is_none_or(|t| e.tags.contains(t)))
            {
                let target = format!("={}", entry.name);

                let running = Command::new("tmux")
                    .args(["has-session", "-t", &target])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if !running {
                    all_up = false;
                    if !all {
                        hidden += 1;
                        continue;
                    }
                }

                let windows = if running {
                    let n = Command::new("tmux")
                        .args([
                            "display-message",
                            "-p",
                            "-t",
                            &format!("={}:", entry.name),
                            "#{session_windows}",
                        ])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|_| "?".to_string());
                    format!("{n} win")
                } else {
                    "-".to_string()
                };

                let note = if std::path::Path::new(&entry.path).is_dir() {
                    ""
                } else {
                    " (path missing)"
                };

                println!(
                    "{:<8} {:<20} {:<7} {}{note}",
                    if running { "running" } else { "stopped" },
                    entry.name,
                    windows,
                    entry.path
                );
            }

            if hidden > 0 {
                println!("{hidden} stopped, pass --all to see them");
            }

            if !all_up {
                std::process::exit(1);
            }
        }

        Cmd::List { tags } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            for t in &tags {
                require_tag(&registry, t);
            }

            let show = if tags.is_empty() {
                registry.tags.clone()
            } else {
                tags
            };

            for t in &show {
                println!("{t}");

                let mut any = false;
                for entry in registry.entries.iter().filter(|e| e.tags.contains(t)) {
                    println!("  {:<20} {}", entry.name, entry.path);
                    any = true;
                }

                if !any {
                    println!("  (empty)");
                }
            }
        }

        Cmd::TagAdd { name } => {
            let mut registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            if registry.tags.contains(&name) {
                println!("tag '{name}' already exists");
                return;
            }

            registry.tags.push(name.clone());

            if let Err(e) = storage::save(&registry) {
                eprintln!("{e}");
                std::process::exit(1);
            }

            println!("created tag {name}");
        }

        Cmd::TagRm { name, force } => {
            let mut registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            require_tag(&registry, &name);

            let count = registry
                .entries
                .iter()
                .filter(|e| e.tags.contains(&name))
                .count();

            if count > 0 && !force {
                eprintln!(
                    "tag '{name}' still has {count} entries, pass --force to remove it and them"
                );
                std::process::exit(1);
            }

            registry.entries.retain(|e| !e.tags.contains(&name));
            registry.tags.retain(|t| t != &name);

            if let Err(e) = storage::save(&registry) {
                eprintln!("{e}");
                std::process::exit(1);
            }

            if count > 0 {
                println!("removed tag {name} and {count} entries");
            } else {
                println!("removed tag {name}");
            }
        }

        Cmd::Help => println!("{HELP}"),
    }
}
