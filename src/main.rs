mod helper;
mod storage;

use std::os::unix::process::CommandExt;
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
        name: bool,
        key: String,
    },
    Kill {
        key: String,
        name: bool,
        force: bool,
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
    Attach {
        tag: String,
    },
    Help,
}

const HELP: &str = "\
zz - tmux session manager

usage:
  zz attach/a/at <tag>                          attach to the tag's tmux server
  zz add <path> <name> <tag> [--create-tag/-c-t]   register a directory
  zz rm <name>                                  remove an entry
  zz run/rn <tag>/<name> [--name/-n]            start every session in the tag
  zz kill/k <tag>/<name> [--name/-n] [--force/-f] kill the sessions by tag or name, --force/-f to kill running session
  zz status/s/st [tag] [--all/-a]               report running sessions, --all/-a for stopped too
  zz list/ls [tag...]                           list what is registered
  zz tag/t add/a <name>                         create a tag
  zz tag/t rm <name> [--force/-f]               delete a tag, --force/-f if not empty

tags are never created implicitly, you create one when you need (only reason is: it avoids mistype)
attach, add, run, status and list accept a shortened tag (fr or frlnc for freelance) as long as only one tag matches,
run --name accepts a shortened name the same way, everything else needs the full name";

fn parse(args: &[String]) -> Result<Cmd, String> {
    if args.is_empty() {
        return Ok(Cmd::Help);
    }

    let rest = &args[1..];

    match args[0].as_str() {
        "add" => {
            let mut create_tag = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--create-tag" || a == "-c-t" {
                    create_tag = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 3 {
                return Err("usage: add <path> <name> <tag> [--create-tag/-c-t]".to_string());
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
        "run" | "rn" => {
            let mut name_tag = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--name" || a == "-n" {
                    name_tag = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 1 {
                return Err("usage: run/rn <tag>/<name> [--name/-n]".to_string());
            }

            Ok(Cmd::Run {
                key: positional[0].clone(),
                name: name_tag,
            })
        }
        "kill" | "k" => {
            let mut force = false;
            let mut name_tag = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--force" || a == "-f" {
                    force = true;
                } else if a == "--name" || a == "-n" {
                    name_tag = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 1 {
                return Err("usage: kill/k <tag>/<name> [--name/-n] [--force/-f]".to_string());
            }
            Ok(Cmd::Kill {
                key: positional[0].clone(),
                name: name_tag,
                force,
            })
        }
        "status" | "s" | "st" => {
            let mut all = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--all" || a == "-a" {
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
                _ => Err("usage: status/s/st [tag] [--all/-a]".to_string()),
            }
        }
        "list" | "ls" => Ok(Cmd::List {
            tags: rest.to_vec(),
        }),
        "tag" | "t" => {
            let mut force = false;
            let mut positional = Vec::new();

            for a in rest {
                if a == "--force" || a == "-f" {
                    force = true;
                } else {
                    positional.push(a.clone());
                }
            }

            if positional.len() != 2 {
                return Err("usage: tag/t add/a <name> | tag/t rm <name> [--force/-f]".to_string());
            }

            match positional[0].as_str() {
                "add" | "a" => Ok(Cmd::TagAdd {
                    name: positional[1].clone(),
                }),
                "rm" => Ok(Cmd::TagRm {
                    name: positional[1].clone(),
                    force,
                }),
                other => Err(format!(
                    "unknown tag subcommand '{other}', usage: tag/t add/a <name> | tag/t rm <name> [--force/-f]"
                )),
            }
        }
        "attach" | "a" | "at" => {
            if rest.len() != 1 {
                return Err("usage: attach/a/at <tag>".to_string());
            }
            Ok(Cmd::Attach {
                tag: rest[0].clone(),
            })
        }
        "--help" | "-h" | "help" => Ok(Cmd::Help),
        other => Err(format!(
            "command not valid: '{other}', check zz --help/-h/help"
        )),
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

// fuzzy: accept a unique tag whose letters contain `tag` in order (frlnc -> freelance),
// only for commands that can't destroy anything
fn require_tag(registry: &storage::Registry, tag: &str, fuzzy: bool) -> String {
    if registry.tags.iter().any(|t| t == tag) {
        return tag.to_string();
    }

    let matches: Vec<&str> = registry
        .tags
        .iter()
        .filter(|t| {
            let mut rest = t.chars();
            fuzzy && tag.chars().all(|c| rest.any(|x| x == c))
        })
        .map(|t| t.as_str())
        .collect();

    // a unique prefix wins over looser matches, so fr is freelance even though forme has f..r
    let starts: Vec<&str> = matches.iter().copied().filter(|t| t.starts_with(tag)).collect();
    if starts.len() == 1 {
        return starts[0].to_string();
    }

    if matches.len() == 1 {
        return matches[0].to_string();
    }

    if matches.len() > 1 {
        eprintln!("tag '{tag}' is ambiguous, could be: {}", matches.join(", "));
        std::process::exit(1);
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
            mut tag,
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
                } else if registry.tags.iter().any(|t| {
                    let mut rest = t.chars();
                    tag.chars().all(|c| rest.any(|x| x == c))
                }) {
                    tag = require_tag(&registry, &tag, true);
                } else {
                    eprintln!("tag '{tag}' does not exist, use --create-tag/-c-t to create it");
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

        Cmd::Run { mut key, name } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            if name {
                if !registry.entries.iter().any(|e| e.name == key) {
                    // same fuzzy rules as tags: unique prefix first, then letters in order
                    let matches: Vec<&str> = registry
                        .entries
                        .iter()
                        .map(|e| e.name.as_str())
                        .filter(|n| {
                            let mut rest = n.chars();
                            key.chars().all(|c| rest.any(|x| x == c))
                        })
                        .collect();
                    let starts: Vec<&str> =
                        matches.iter().copied().filter(|n| n.starts_with(&key)).collect();

                    if starts.len() == 1 {
                        key = starts[0].to_string();
                    } else if matches.len() == 1 {
                        key = matches[0].to_string();
                    } else if matches.len() > 1 {
                        eprintln!("name '{key}' is ambiguous, could be: {}", matches.join(", "));
                        std::process::exit(1);
                    } else {
                        eprintln!("name '{key}' does not exist");
                        std::process::exit(1);
                    }
                }
            } else {
                key = require_tag(&registry, &key, true);
            }

            let mut failed = false;

            let entries: Vec<_> = if name {
                registry.entries.iter().filter(|e| e.name == key).collect()
            } else {
                registry
                    .entries
                    .iter()
                    .filter(|e| e.tags.contains(&key))
                    .collect()
            };

            for entry in entries {
                let target = format!("={}", entry.name);
                let socket = format!("zz-{}", entry.tags[0]);

                let running = Command::new("tmux")
                    .args(["-L", &socket, "has-session", "-t", &target])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if running {
                    let live = Command::new("tmux")
                        .args([
                            "-L",
                            &socket,
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
                    .args([
                        "-L",
                        &socket,
                        "new-session",
                        "-d",
                        "-s",
                        &entry.name,
                        "-c",
                        &entry.path,
                    ])
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

        Cmd::Kill { key, name, force } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };

            if name {
                if !registry.entries.iter().any(|e| e.name == key) {
                    eprintln!("name '{key}' does not exist");
                    std::process::exit(1);
                }
            } else {
                require_tag(&registry, &key, false);
            }

            let entries: Vec<_> = if name {
                registry.entries.iter().filter(|e| e.name == key).collect()
            } else {
                registry
                    .entries
                    .iter()
                    .filter(|e| e.tags.contains(&key))
                    .collect()
            };

            for entry in entries {
                let target = format!("={}", entry.name);
                let socket = format!("zz-{}", entry.tags[0]);

                let running = Command::new("tmux")
                    .args(["-L", &socket, "has-session", "-t", &target])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if !running {
                    continue;
                }

                let mut have_process = false;

                if let Ok(o) = Command::new("tmux")
                    .args([
                        "-L",
                        &socket,
                        "list-panes",
                        "-s",
                        "-t",
                        &target,
                        "-F",
                        "#{pane_pid}",
                    ])
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
                            have_process = true;
                        }
                    }
                }

                if have_process && !force {
                    continue;
                }

                let killed = Command::new("tmux")
                    .args(["-L", &socket, "kill-session", "-t", &target])
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
            let tag = tag.map(|t| require_tag(&registry, &t, true));

            let show = match &tag {
                Some(t) => vec![t.clone()],
                None => registry.tags.clone(),
            };

            let home = dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            let mut all_up = true;
            let mut hidden = 0;
            let mut rows: Vec<(String, String, &str, String, String)> = Vec::new();

            for t in &show {
                let socket = format!("zz-{t}");

                for entry in registry.entries.iter().filter(|e| e.tags.contains(t)) {
                    let target = format!("={}", entry.name);

                    let running = Command::new("tmux")
                        .args(["-L", &socket, "has-session", "-t", &target])
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
                        Command::new("tmux")
                            .args([
                                "-L",
                                &socket,
                                "display-message",
                                "-p",
                                "-t",
                                &format!("={}:", entry.name),
                                "#{session_windows}",
                            ])
                            .output()
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                            .unwrap_or_else(|_| "?".to_string())
                    } else {
                        "-".to_string()
                    };

                    let path = match entry.path.strip_prefix(&home) {
                        Some(rest)
                            if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
                        {
                            format!("~{rest}")
                        }
                        _ => entry.path.clone(),
                    };

                    let path = if std::path::Path::new(&entry.path).is_dir() {
                        path
                    } else {
                        format!("{path}  (path missing)")
                    };

                    rows.push((
                        t.clone(),
                        entry.name.clone(),
                        if running { "running" } else { "stopped" },
                        windows,
                        path,
                    ));
                }
            }

            let name_w = rows
                .iter()
                .map(|r| r.1.chars().count())
                .chain(std::iter::once(4))
                .max()
                .unwrap();

            let win_w = rows
                .iter()
                .map(|r| r.3.chars().count())
                .chain(std::iter::once(3))
                .max()
                .unwrap();

            let mut first = true;

            for t in &show {
                let group: Vec<_> = rows.iter().filter(|r| &r.0 == t).collect();
                let registered = registry.entries.iter().any(|e| e.tags.contains(t));

                if group.is_empty() && registered {
                    continue;
                }

                if !first {
                    println!();
                }
                first = false;

                println!("{t}");

                if !registered {
                    println!("  (empty)");
                    continue;
                }

                println!(
                    "  {:<name_w$}  {:<7}  {:>win_w$}  PATH",
                    "NAME", "STATUS", "WIN"
                );

                for r in group {
                    println!("  {:<name_w$}  {:<7}  {:>win_w$}  {}", r.1, r.2, r.3, r.4);
                }
            }

            if hidden > 0 {
                if !first {
                    println!();
                }
                println!("{hidden} stopped, pass --all/-a to see them");
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

            let tags: Vec<String> = tags.iter().map(|t| require_tag(&registry, t, true)).collect();

            let show = if tags.is_empty() {
                registry.tags.clone()
            } else {
                tags
            };

            let home = dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            let name_w = registry
                .entries
                .iter()
                .filter(|e| show.iter().any(|t| e.tags.contains(t)))
                .map(|e| e.name.chars().count())
                .chain(std::iter::once(4))
                .max()
                .unwrap();

            let mut first = true;

            for t in &show {
                let group: Vec<_> = registry
                    .entries
                    .iter()
                    .filter(|e| e.tags.contains(t))
                    .collect();

                if !first {
                    println!();
                }
                first = false;

                println!("{t}");

                if group.is_empty() {
                    println!("  (empty)");
                    continue;
                }

                println!("  {:<name_w$}  PATH", "NAME");

                for entry in group {
                    let path = match entry.path.strip_prefix(&home) {
                        Some(rest)
                            if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
                        {
                            format!("~{rest}")
                        }
                        _ => entry.path.clone(),
                    };

                    println!("  {:<name_w$}  {path}", entry.name);
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
            require_tag(&registry, &name, false);

            let count = registry
                .entries
                .iter()
                .filter(|e| e.tags.contains(&name))
                .count();

            if count > 0 && !force {
                eprintln!(
                    "tag '{name}' still has {count} entries, pass --force/-f to remove it and them"
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

        Cmd::Attach { tag } => {
            let registry = match storage::load() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
            let tag = require_tag(&registry, &tag, true);

            let socket = format!("zz-{tag}");

            let running = Command::new("tmux")
                .args(["-L", &socket, "list-sessions"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !running {
                eprintln!("no sessions running for tag '{tag}', run: zz run {tag}");
                std::process::exit(1);
            }

            // inside tmux: replace this client with one on the tag's server, no nesting
            if std::env::var("TMUX").is_ok_and(|v| !v.is_empty()) {
                let ok = Command::new("tmux")
                    .args(["detach-client", "-E", &format!("tmux -L {socket} attach")])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !ok {
                    eprintln!("failed to switch to {socket}");
                    std::process::exit(1);
                }
                return;
            }

            let e = Command::new("tmux").args(["-L", &socket, "attach"]).exec();
            eprintln!("failed to attach to {socket}: {e}");
            std::process::exit(1);
        }

        Cmd::Help => println!("{HELP}"),
    }
}
