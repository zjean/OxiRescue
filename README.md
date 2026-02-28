# oxirescue

Standalone disaster recovery tool for [OxiCloud](https://github.com/your-org/oxicloud).

## The Problem

OxiCloud stores uploaded files in a content-addressed blob store (`.blobs/`). The files are
stored by SHA-256 hash, not by their original names. Without a running OxiCloud instance and
access to its PostgreSQL database, the data is unreadable. **oxirescue** solves this by reading
the blob store and the metadata (live from Postgres or from an offline SQLite export) directly,
with zero dependency on OxiCloud source code.

## Operating Modes

| Mode     | Requires         | Blobs needed | Description                              |
|----------|------------------|--------------|------------------------------------------|
| Live     | PostgreSQL + blobs | Yes        | Direct connection to the running database |
| Offline  | SQLite export + blobs | Yes     | Works without network access to Postgres |
| Bare     | Blobs only       | Yes          | Dump/classify all blobs without metadata |

## Installation

```sh
# Standard build (TUI + dump + export)
cargo install --path .

# With read-only FUSE mount support (requires macFUSE on macOS)
cargo install --path . --features fuse
```

## Commands

### dump — extract blobs without database

Extract and optionally classify every blob in the store into a normal directory tree.

```sh
oxirescue dump \
  --blobs /var/lib/oxicloud/.blobs \
  --output /tmp/recovered \
  --classify
```

Flags:
- `--classify` — group files into subdirectories by MIME type (images/, documents/, …)
- `--verify` — re-hash every blob and report corrupted files
- `--dry-run` — show what would be extracted without writing anything
- `--min-size 1MB` — skip blobs smaller than the given size
- `--copy` — force a full copy instead of a hard-link (useful across filesystems)

### export-metadata — back up PostgreSQL metadata to SQLite

```sh
oxirescue export-metadata \
  --db "postgres://user:pass@localhost/oxicloud" \
  --output backup.db
```

The resulting `backup.db` file is a portable SQLite database that can be used with the `--meta`
flag for offline operation.

### tui — interactive file browser

```sh
# Live mode (requires access to Postgres)
oxirescue tui \
  --db "postgres://user:pass@localhost/oxicloud" \
  --blobs /var/lib/oxicloud/.blobs

# Offline mode
oxirescue tui \
  --meta backup.db \
  --blobs /var/lib/oxicloud/.blobs
```

### mount — read-only FUSE filesystem (requires `--features fuse`)

```sh
oxirescue mount \
  --meta backup.db \
  --blobs /var/lib/oxicloud/.blobs \
  /mnt/oxicloud
```

Mounts the OxiCloud filesystem as a standard read-only directory tree. Requires macFUSE on
macOS (`brew install --cask macfuse`).

## TUI Keybindings

| Key         | Context    | Action                              |
|-------------|------------|-------------------------------------|
| `q` / `Esc` | Browser    | Return to dashboard                 |
| `q`         | Dashboard  | Quit                                |
| `j` / `Down`| Anywhere   | Move selection down                 |
| `k` / `Up`  | Anywhere   | Move selection up                   |
| `Enter`     | Dashboard  | Open user file browser              |
| `Enter`     | Browser (left pane) | Navigate into folder / up  |
| `Tab`       | Browser    | Switch between left and right pane  |
| `Space`     | Browser (left) | Toggle file selection           |
| `a`         | Browser (left) | Select all files in current folder |
| `c`         | Browser    | Copy selected files to target dir   |
| `E`         | Browser    | Export highlighted folder subtree   |

The right pane shows the contents of `/tmp/oxirescue-export` (the current target directory).

## Architecture

oxirescue is fully self-contained. It does not import any OxiCloud crates. It speaks directly
to the same PostgreSQL schema (or an offline SQLite copy of it) and reads blobs from the
content-addressed store using the same SHA-256 naming convention. This means it continues to
work even if OxiCloud's own code changes.

Key modules:
- `blob/` — blob store reader, SHA-256 verifier, MIME classifier
- `db/` — PostgreSQL and SQLite metadata backends behind a common `MetadataSource` trait
- `dump/` — blob extraction with progress reporting
- `export/` — PostgreSQL-to-SQLite metadata export
- `tui/` — ratatui-based interactive file browser
- `fuse/` — optional read-only FUSE filesystem (feature-gated)
