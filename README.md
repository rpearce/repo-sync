# repo-sync

Public domain ([LICENSE](./LICENSE)) Rust CLI to clone or sync multiple Git
repositories listed in a file.

<details>
  <summary>Expand to view my original Bash script</summary>

```bash
#!/usr/bin/env bash

set -Eeuo pipefail

# ==============================================================================

REPOS_FILE="$1"

# Example `repos.txt` file that could be passed in:
#
# github.com/rpearce/dotfiles
# github.com/rpearce/slugger
# github.com/rpearce/react-medium-image-zoom

# ==============================================================================

function has_cmd {
  hash "${1}" 2> /dev/null
}

# ==============================================================================

function check_git {
  if ! has_cmd git; then
    echo "error: git command not found"
    return 1
  fi
}

function check_repos_file {
  if [[ ! -s "${REPOS_FILE}" ]]; then
    echo "error: repos file \"${REPOS_FILE}\" not found"
    return 1
  fi
}

# ==============================================================================

function sync_repo_branches {
  local current_branch

  current_branch="$(git rev-parse --abbrev-ref HEAD)"

  # Fetch all remotes and prune refs and tags
  git fetch --all -Pp --quiet

  # 1. Format local refs as "local-branch:upstream-branch"
  # 2. Remove those without an upstream branch
  # 3. Perform fast-forward merges on each, taking care to do
  #    nothing if the current branch has any changes.
  git for-each-ref --format '%(refname:short):%(upstream:short)' 'refs/heads' | \
    grep -Ev ':$' | \
    while IFS=: read -r local_branch upstream_branch; do
      if [[ "${current_branch}" == "${local_branch}" ]]; then
        if [[ -z $(git status --porcelain) ]]; then
          git merge --ff-only "$upstream_branch" --quiet
        fi
      else
        git fetch . "$upstream_branch:$local_branch" --quiet
      fi
    done
}

function sync_repo {
  local entry="$0"

  # Splits a string, by a delimiter, into an array.
  # Approach tradeoffs: https://stackoverflow.com/a/45201229
  # shellcheck disable=SC2206
  local org_repo=(${entry//\// })

  local repo="${org_repo[2]}"

  if [[ -d "${repo}" ]]; then
    cd "$repo" && sync_repo_branches || return 1
  else
    git clone "https://${entry}.git" --quiet || return 1
  fi
}

export -f sync_repo sync_repo_branches

function sync_repos {
  echo "Syncing repos from ${REPOS_FILE}..."
  < "${REPOS_FILE}" xargs -n 1 -P 8 /usr/bin/env bash -c 'sync_repo $@'
  echo "Done"
}

# ==============================================================================

function main {
  check_git
  check_repos_file
  sync_repos
}

main
```

</details>

## Features

- Clone multiple repositories from a text file of URLs.
- Fetch and fast-forward branches for existing repositories (never `git pull`).
- Process repositories in parallel; the number of jobs is configurable with `-j`/`--jobs` (defaults to the number of CPUs).
- Never prompts on the terminal for HTTPS credentials, so an unattended run fails fast instead of hanging (see [Credentials and prompts](#credentials-and-prompts)).
- Prebuilt binaries for Linux (x86_64 glibc/musl) and macOS (x86_64/arm64); other platforms are untested but may build from source.

## Installation

### From GitHub Releases (Recommended)

Download the latest binary for your platform from the [releases page](https://github.com/rpearce/repo-sync/releases):

**Linux x86_64:**
```bash
mkdir -p ~/.local/bin
curl -L https://github.com/rpearce/repo-sync/releases/latest/download/repo-sync-linux-x86_64.tar.gz | tar -xzf -
mv repo-sync ~/.local/bin/
```

**Linux x86_64 (musl/static):**
```bash
mkdir -p ~/.local/bin
curl -L https://github.com/rpearce/repo-sync/releases/latest/download/repo-sync-linux-musl-x86_64.tar.gz | tar -xzf -
mv repo-sync ~/.local/bin/
```

**macOS (Intel):**
```bash
mkdir -p ~/.local/bin
curl -L https://github.com/rpearce/repo-sync/releases/latest/download/repo-sync-macos-x86_64.tar.gz | tar -xzf -
mv repo-sync ~/.local/bin/
```

**macOS (Apple Silicon):**
```bash
mkdir -p ~/.local/bin
curl -L https://github.com/rpearce/repo-sync/releases/latest/download/repo-sync-macos-aarch64.tar.gz | tar -xzf -
mv repo-sync ~/.local/bin/
```

**Note:** Make sure `~/.local/bin` is in your PATH. Add this to your shell config (`~/.bashrc`, `~/.zshrc`, etc.):
```bash
export PATH="$HOME/.local/bin:$PATH"
```

### From Source

Using Cargo:

```bash
cargo install --path .
```

## Usage

```bash
repo-sync <clone|sync> -f repos.txt -o ./repos
```

### Global options

These apply to both subcommands and may appear before or after them:

- `-v, --verbose`: Print the run's header line, a few informational
  lines (e.g. an existing clone being skipped, a dirty current branch,
  or a `[gone]` upstream — named by entry/branch, not by repository),
  and the summary line on stdout when the run succeeds. Attribution is
  partial: a branch-level fast-forward or fetch is printed as
  `<repo>: <line>`, but `git clone` and `git fetch --all` inherit the
  terminal directly (they're passed `--quiet` without `-v`), so their
  own output appears unattributed to any repository. Without `-v`,
  stdout gets nothing; warnings (e.g. a duplicate entry, a diverged
  branch) still go to stderr regardless.
- `-j, --jobs <N>`: Limit how many repositories are processed in
  parallel. Defaults to the number of CPUs; `RAYON_NUM_THREADS` also
  works. `0` is rejected.

### Sync repositories

```bash
repo-sync sync -f repos.txt -o ./repos
```

- `-f, --file`: Path to a text file containing one repository URL per line.
- `-o, --out`: Output directory to clone repositories into.

For each entry:

- Clones the repository if it isn't found locally.
- Fails if the target directory already exists but isn't a git repository.
- Otherwise, updates the existing clone:
  - Fetches all remotes with `--prune`, which removes deleted
    remote-tracking branches. `repo-sync` doesn't request tag pruning
    itself, but a `fetch.pruneTags` (or `remote.<name>.pruneTags`)
    setting in your own git config still applies and can delete local
    tags.
  - Fast-forwards the current branch only (`merge --ff-only`; never
    merges or rebases), and skips it when tracked files are modified.
    Untracked files don't count as modifications (git still refuses to
    overwrite one that's in the way).
  - Fast-forwards every other local branch directly from its upstream,
    without checking it out.
  - Skips a branch whose upstream is gone (deleted on the remote); shown
    as an informational line only with `-v`.
  - A branch that can't be fast-forwarded (e.g. it has diverged) prints
    an attributed `warning:` line and is left alone; this never fails
    the run.

### Clone repositories

```bash
repo-sync clone -f repos.txt -o ./repos
```

- `-f, --file`: Path to a text file containing one repository URL per line.
- `-o, --out`: Output directory to clone repositories into.

- Clones every entry that isn't already present locally.
- An entry whose directory already exists is skipped (not re-fetched or
  verified as a git repository) and still counts as "ok" in the summary.

### Exit status

- `0`: every repository succeeded.
- `1`: any of the following:
  - the repo-list file can't be read;
  - `git` isn't on `PATH`;
  - the thread pool for `-j`/`--jobs` fails to build;
  - one or more repositories failed to clone/sync — this is the only
    case where the summary line is printed, to stderr, regardless of
    `-v`; the other three cases print just their own error line.

  A warning (e.g. a diverged branch, a `[gone]` upstream, a duplicate
  entry) never causes this.
- `2`: a command-line usage error (e.g. a missing required argument, or
  no arguments at all). `--help` and `--version` exit `0`.

The summary line's format is:

```
<Cloned|Synced> <N> repositories: <ok> ok, <failed> failed
```

### Credentials and prompts

`repo-sync` never prompts on the terminal for HTTPS credentials — every
git invocation sets `GIT_TERMINAL_PROMPT=0` and closes stdin — so an
HTTPS remote that needs a username/password fails immediately instead
of hanging. This only suppresses git's own built-in terminal prompt: a
configured credential helper or askpass program (`GIT_ASKPASS`,
`core.askPass`, `SSH_ASKPASS`, a GUI credential manager) still runs and
can still prompt, and it doesn't cover SSH's own prompts (host-key
verification, a passphrase-protected key), which read the controlling
terminal directly rather than stdin. Set up `ssh-agent`, or configure
`BatchMode`, for those remotes so they don't hang either.

### File format

`repos.txt` example:

```txt
github.com/user/repo1
github.com/user/repo2
github.com/user/repo3
```

- One entry per line.
- Blank lines and lines starting with `#` are ignored. Comments are
  whole-line only: a `#` elsewhere on a line, e.g. after an entry, is
  treated as part of the entry, not a comment. Leading and trailing
  whitespace is trimmed from every line.
- An entry can be, among other forms:
  - a bare `host/owner/repo` (gets `https://` prepended);
  - an scp-style SSH remote, e.g. `git@host:owner/repo.git`;
  - a local path, starting with `/` or `.` (`~` is **not** expanded, so
    use an absolute path or one starting with `.` instead);
  - any URL with a scheme (`https://`, `ssh://`, `git://`, `file://`,
    etc.), used as-is — except `http://`, which is upgraded to `https://`.
- A bare `host:port/owner/repo` entry is treated as scp-style (this is
  git's own rule for telling scp-style remotes from paths), so a custom
  port needs an explicit `https://` or `ssh://` URL, e.g.
  `ssh://git@host:2222/owner/repo.git`.
- The local directory an entry is cloned into is named after the last
  path segment of the entry, with a trailing `.git` removed (only one —
  `repo.git.git` becomes `repo.git`, not `repo`). An entry that has no
  such segment (e.g. a bare host with no path) is an error, and two
  *different* entries that resolve to the same directory name are also
  an error. Two entries that normalize to the exact same URL (e.g. the
  same line listed twice) aren't treated as an error: the repeat is
  ignored with a warning, and the entry is only processed once.

## Releases

To create a new release:

1. Run `./release <version>` (e.g., `./release 1.0.0`)
2. Create and merge a pull request with the version bump
3. After PR is merged: `git switch main && git pull && git push origin <version>`

GitHub Actions will automatically build and publish binaries for Linux and macOS with auto-generated release notes.
