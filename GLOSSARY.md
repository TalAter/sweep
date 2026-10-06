# Sweep

Sweep reviews shell installers, runs approved scripts, and tracks successful
installs.

## Language

**Install command**: The user's instruction to download and run a script,
including its URL, shell, environment values, optional sudo, and script arguments.

**Fetcher**: The download program named in an install command (`curl` or `wget`).
Sweep downloads the script itself, so fetcher flags do not carry over.

**Runner shell**: The shell selected to execute the approved script: `sh`, `bash`,
or `zsh`.

**Invocation**: One committed install attempt, including a failure or decline.
Cancelling command entry before submission is not an invocation.

**Package**: A tracked tool identified by its source URL. Its slug is a
human-readable display name, not a unique identity.

**Install**: An approved execution that returned zero. A failed later attempt
does not erase an earlier successful install.

**Analysis pass**: The review of a script's behavior, concerns, and severity.

**Manipulation pass**: An independent review for attempts to steer the analyzer.
Only an explicitly clean result allows the analysis summary to be trusted.

**Sweep home**: The directory containing Sweep's configuration, history, and
saved scripts.

**Script store**: Saved script contents identified by their content hash.
A declined invocation can have a hash without a saved script.
