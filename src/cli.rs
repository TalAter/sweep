#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Interactive,
    Install,
    List,
    Help,
    Version,
}

struct Entry {
    argument: Option<&'static str>,
    command: Command,
    description: &'static str,
}

// Exact arguments take precedence over the installer fallback.
const COMMANDS: &[Entry] = &[
    Entry {
        argument: Some(""),
        command: Command::Interactive,
        description: "Enter an install command interactively",
    },
    Entry {
        argument: None,
        command: Command::Install,
        description: "Review a quoted install command and run only after approval",
    },
    Entry {
        argument: Some("list"),
        command: Command::List,
        description: "List successfully installed packages",
    },
    Entry {
        argument: Some("--help"),
        command: Command::Help,
        description: "Show this help",
    },
    Entry {
        argument: Some("--version"),
        command: Command::Version,
        description: "Show the package version",
    },
];

pub fn command(argument: &str) -> Command {
    COMMANDS
        .iter()
        .find(|entry| entry.argument == Some(argument))
        .or_else(|| COMMANDS.iter().find(|entry| entry.argument.is_none()))
        .expect("command table includes an installer fallback")
        .command
}

pub fn entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    COMMANDS.iter().map(|entry| {
        (
            entry.argument.unwrap_or("<INSTALL_COMMAND>"),
            entry.description,
        )
    })
}
