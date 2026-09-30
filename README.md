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

### Requirements

`git` must be on your PATH.

### From GitHub Releases (Recommended)

Pick the asset for your platform:

| Platform                    | Asset                                |
|------------------------------|---------------------------------------|
| Linux x86_64                 | `repo-sync-linux-x86_64.tar.gz`       |
| Linux x86_64 (musl/static)   | `repo-sync-linux-musl-x86_64.tar.gz`  |
| macOS (Intel)                | `repo-sync-macos-x86_64.tar.gz`       |
| macOS (Apple Silicon)        | `repo-sync-macos-aarch64.tar.gz`      |

Set `ASSET` below to that filename, then paste the whole block into your
shell (works in both bash and zsh). It runs in a subshell so it can't alter
or kill your interactive shell, downloads only over HTTPS, and verifies the
release's SHA-256 checksum — including that the checksum file actually
names the tarball just downloaded, not some other file — before extracting
anything:

```bash
(
  set -euo pipefail
  ASSET=repo-sync-macos-aarch64.tar.gz
  BASE="https://github.com/rpearce/repo-sync/releases/latest/download"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT; cd "$tmp"
  curl --proto '=https' --tlsv1.2 -fsSLO "$BASE/$ASSET"
  curl --proto '=https' --tlsv1.2 -fsSLO "$BASE/$ASSET.sha256"
  grep -qx "[0-9a-f]\{64\}  $ASSET" "$ASSET.sha256" || { echo "error: $ASSET.sha256 does not list $ASSET" >&2; exit 1; }
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$ASSET.sha256"
  else
    shasum -a 256 -c "$ASSET.sha256"
  fi
  tar -xzf "$ASSET"
  mkdir -p ~/.local/bin
  install -m 0755 repo-sync ~/.local/bin/repo-sync
)
```

**Recommended, if you have the [GitHub CLI](https://cli.github.com/)
installed and logged in (`gh auth login`):** use this variant instead of
the one above. It additionally verifies the release's build provenance
attestation before extracting, which is a stronger guarantee than the
checksum: the checksum only proves the download matches what this release
published, while the attestation proves this repository's GitHub Actions
workflow actually built it.

```bash
(
  set -euo pipefail
  ASSET=repo-sync-macos-aarch64.tar.gz
  BASE="https://github.com/rpearce/repo-sync/releases/latest/download"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT; cd "$tmp"
  curl --proto '=https' --tlsv1.2 -fsSLO "$BASE/$ASSET"
  curl --proto '=https' --tlsv1.2 -fsSLO "$BASE/$ASSET.sha256"
  grep -qx "[0-9a-f]\{64\}  $ASSET" "$ASSET.sha256" || { echo "error: $ASSET.sha256 does not list $ASSET" >&2; exit 1; }
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$ASSET.sha256"
  else
    shasum -a 256 -c "$ASSET.sha256"
  fi
  gh attestation verify "$ASSET" --repo rpearce/repo-sync --signer-workflow rpearce/repo-sync/.github/workflows/release.yml
  tar -xzf "$ASSET"
  mkdir -p ~/.local/bin
  install -m 0755 repo-sync ~/.local/bin/repo-sync
)
```

**Note:** `.sha256` checksums and build provenance attestations are
published for releases after 0.1.2; 0.1.2 and earlier don't have them.

**Note:** Make sure `~/.local/bin` is in your PATH. Add this to your shell config (`~/.bashrc`, `~/.zshrc`, etc.):
```bash
export PATH="$HOME/.local/bin:$PATH"
```

### From Source

Using Cargo:

```bash
cargo install --locked --path .
```

Or directly from GitHub, without cloning:

```bash
cargo install --locked --git https://github.com/rpearce/repo-sync
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

## Releases

To create a new release:

1. On a non-`main` branch, run `./release <version>` (e.g., `./release 1.0.0`). This bumps the version in `Cargo.toml`/`Cargo.lock`, runs the test suite, and commits the change; it does not tag or push anything.
2. Open a pull request for that branch and merge it into `main`.
3. Tag the merge commit on `main` and push the tag (or `git tag -a` if you don't sign tags):
   ```bash
   git switch main && git pull --ff-only && git tag -s <version> -m "Release <version>" && git push origin <version>
   ```

Pushing the tag triggers GitHub Actions, which refuses to build unless the tag matches the version in `Cargo.toml` and the tagged commit is on `main`. It then builds and publishes binaries for Linux and macOS, each with a `.sha256` checksum and a signed build provenance attestation, with auto-generated release notes.
