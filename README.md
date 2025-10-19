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
- Pull updates and fast-forward branches for existing repositories.
- Cross-platform compatible (Linux, macOS, Windows).

## Installation

Using Cargo:

```bash
cargo install --path .
```

## Usage

### Sync repositories

```bash
repo-sync sync -f repos.txt -o ./repos
```

- `-f, --file`: Path to a text file containing one repository URL per line.
- `-o, --out`: Output directory to clone repositories into.

- Clones any repositories that aren't found locally.
- Pulls latest changes for the repositories.
- Fast-forward merges current branches if the working tree is clean.
- Updates any other branches from upstream without checking them out.

### Clone repositories

```bash
repo-sync clone -f repos.txt -o ./repos
```

- `-f, --file`: Path to a text file containing one repository URL per line.
- `-o, --out`: Output directory to clone repositories into.

- Used for only doing multi-repository cloning.


### File format

`repos.txt` example:

```txt
github.com/user/repo1
github.com/user/repo2
github.com/user/repo3
```

Repo lines can be prefixed with `https://` and/or end with `.git`, if preferred.
