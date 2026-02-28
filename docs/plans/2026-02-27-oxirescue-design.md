# OxiRescue — Standalone Disaster Recovery Tool for OxiCloud

## Problem

OxiCloud stores files as content-addressed blobs in `.blobs/{00-ff}/{hash}.blob` with all metadata (filenames, paths, ownership, folder hierarchy) in PostgreSQL. If the application is lost, uncompilable, or a backup needs restoring, those blob files are meaningless without the database. There is no way to recover a normal filesystem from the raw storage.

## Tool Identity

**Name**: `oxirescue`
**Type**: Standalone Rust binary, separate repository, no OxiCloud code dependency.
**Interface**: TUI-first with CLI flags for scripted/automated use.

## Operating Modes

| Mode | Input needed | Use case |
|---|---|---|
| **Live** | PostgreSQL connection + blob dir | Day-to-day inspection, planned backup |
| **Offline** | Exported metadata file + blob dir | Disaster recovery, no DB available |
| **Bare** | Blob dir only | Catastrophe — no DB, no metadata |

### Capability Matrix

| Capability | Live | Offline | Bare |
|---|---|---|---|
| TUI browse + export | Yes | Yes | No |
| FUSE read-only mount | Yes | Yes | No |
| Export metadata | Yes | No | No |
| Dump blobs (flat + classify) | Yes | Yes | Yes |

The tool auto-detects what's available and enables/disables features accordingly.

## Metadata Export Format

The `export-metadata` command snapshots everything needed to reconstruct the filesystem without PostgreSQL.

**Format**: Single SQLite file (e.g., `oxicloud-meta-2026-02-27.db`)

**Why SQLite over JSON:**
- Queryable — the TUI and FUSE mount can use SQL just like they would against PostgreSQL
- Handles millions of files without loading everything into RAM
- Single file, easy to back up alongside the blob directory
- Schema mirrors the PostgreSQL tables closely

**Tables in the export:**

```sql
users       (id, username, display_name, role)
folders     (id, name, parent_id, user_id, path)
files       (id, name, folder_id, user_id, blob_hash, size, mime_type, created_at, updated_at)
blobs       (hash, size, ref_count, content_type)
shares      (id, item_id, item_type, token, permissions_read, expires_at, created_by)
```

**Export command:**

```bash
oxirescue export-metadata \
  --db postgres://user:pass@host/oxicloud \
  --output oxicloud-meta-2026-02-27.db
```

**Future**: `--since` flag for incremental export, merging into an existing SQLite file.

## TUI Interface

### Launch

```bash
# Live mode
oxirescue tui --db postgres://... --blobs /storage/.blobs

# Offline mode
oxirescue tui --meta oxicloud-meta.db --blobs /storage/.blobs
```

### Dashboard (Landing Screen)

```
+-- OxiRescue ------------------------------------------------------+
|                                                                    |
|  Storage Overview                                                  |
|  ----------------                                                  |
|  Users: 12          Total files: 48,291                            |
|  Folders: 3,847     Unique blobs: 31,006                           |
|  Logical size: 142.3 GB    Physical size: 98.1 GB                  |
|  Dedup ratio: 31.1% saved                                         |
|                                                                    |
|  Users                                                             |
|  -----                                                             |
|  > alice       18,402 files   52.1 GB                              |
|  > bob          9,211 files   38.7 GB                              |
|  > charlie     12,003 files   31.2 GB                              |
|  ...                                                               |
|                                                                    |
|  [Enter] Browse user  [e] Export all  [m] Mount  [q] Quit          |
+--------------------------------------------------------------------+
```

### Dual-Pane Browser (After Selecting a User)

```
+-- alice: /Documents/Work ----------+-- /tmp/restore -----------------+
| > .. (up)                          | > ..                            |
| > Contracts/          12 items     | > already-restored/             |
| > Reports/            8 items      |   report-final.pdf              |
|   budget.xlsx       1.2 MB         |                                 |
|   notes.md          4.3 KB         |                                 |
|   presentation.pptx 8.1 MB         |                                 |
|                                    |                                 |
+-- Preview ---------------------------------------------------------+
| budget.xlsx | 1.2 MB | blob: a1b2c3d4... | 2025-11-03              |
| SHA-256 verified: ok                                                |
+--------------------------------------------------------------------+
| [Enter] Open  [Space] Select  [c] Copy to right                   |
| [/] Search    [a] Select all  [E] Export subtree           [q] Back|
+--------------------------------------------------------------------+
```

**Key interactions:**
- Arrow keys / vim keys to navigate
- `Space` to select files, `a` to select all
- `c` copies selected items to the right pane (restoring original filenames and folder structure)
- `E` exports an entire subtree to a target directory
- `/` to search by filename
- Bottom bar shows metadata preview for highlighted file
- Integrity check on copy — re-hashes blob and compares to stored hash

## FUSE Mount

### Commands

```bash
# Live mode
oxirescue mount --db postgres://... --blobs /storage/.blobs /mnt/oxicloud

# Offline mode
oxirescue mount --meta oxicloud-meta.db --blobs /storage/.blobs /mnt/oxicloud
```

### Mounted Filesystem Layout

```
/mnt/oxicloud/
+-- alice/
|   +-- Documents/
|   |   +-- Work/
|   |   |   +-- budget.xlsx
|   |   |   +-- notes.md
|   |   +-- Personal/
|   |       +-- recipes.pdf
|   +-- Photos/
|       +-- vacation.jpg
+-- bob/
|   +-- ...
+-- .meta/
    +-- stats.json
    +-- orphaned-blobs/
```

### Behavior

- Top-level directories are users
- Below that, the virtual folder hierarchy from the DB
- File reads serve blob content directly from `.blobs/{prefix}/{hash}.blob`
- All standard tools work: `cp`, `rsync`, `tar`, `find`, `grep`
- File permissions: everything shows as `r--r--r--`, owned by the mounting user
- `.meta/` virtual directory exposes stats and orphaned blobs

**FUSE library**: `fuser` crate (pure Rust, no C libfuse dependency)

### Practical Recovery

```bash
# Mount, then rsync a single user out
oxirescue mount --meta backup.db --blobs /backup/.blobs /mnt/oxicloud
rsync -av /mnt/oxicloud/alice/ /home/alice/restored/

# Or tar the whole thing
tar -czf full-restore.tar.gz -C /mnt/oxicloud .
```

## Bare Mode (Catastrophe Recovery)

### Commands

```bash
# Flat dump
oxirescue dump --blobs /storage/.blobs --output /tmp/recovered/

# With classification
oxirescue dump --blobs /storage/.blobs --output /tmp/recovered/ --classify
```

### Flat Dump Output

```
/tmp/recovered/
+-- a1b2c3d4e5f6...64chars.xlsx
+-- b7c8d9e0f1a2...64chars.pdf
+-- c3d4e5f6a7b8...64chars.jpg
+-- d9e0f1a2b3c4...64chars.bin    # unknown type
```

### Classified Output

```
/tmp/recovered/
+-- images/
|   +-- a1b2c3d4...64chars.jpg
|   +-- e5f6a7b8...64chars.png
+-- documents/
|   +-- b7c8d9e0...64chars.pdf
|   +-- c3d4e5f6...64chars.xlsx
+-- video/
+-- audio/
+-- unknown/
    +-- d9e0f1a2...64chars.bin
```

### How It Works

- Walk all 256 prefix directories (`00`-`ff`)
- Read first 8KB of each blob for magic bytes
- Use the `infer` crate to detect MIME type from file signature
- Map MIME to extension and category folder
- Hard-link by default (same filesystem), copy otherwise
- Progress bar showing blob count and bytes processed

### Flags

| Flag | Effect |
|---|---|
| `--classify` | Group into MIME-type folders |
| `--copy` | Force copy instead of hard-link |
| `--verify` | Re-hash every blob, report corrupted files |
| `--dry-run` | Show what would be extracted, with stats |
| `--min-size 1KB` | Skip tiny blobs |

## Project Structure

```
oxirescue/
+-- Cargo.toml
+-- src/
    +-- main.rs              # CLI entry point, mode detection
    +-- cli.rs               # Clap argument definitions
    +-- db/
    |   +-- mod.rs
    |   +-- postgres.rs      # Live PG reader
    |   +-- sqlite.rs        # Offline metadata reader
    |   +-- schema.rs        # Shared types (User, Folder, File, Blob)
    +-- blob/
    |   +-- mod.rs
    |   +-- store.rs         # Read blobs from {prefix}/{hash}.blob
    |   +-- hasher.rs        # SHA-256 verification
    |   +-- classifier.rs    # MIME detection + categorization
    +-- export/
    |   +-- mod.rs
    |   +-- metadata.rs      # PG -> SQLite export
    +-- fuse/
    |   +-- mod.rs
    |   +-- mount.rs         # fuser-based read-only FS
    +-- tui/
    |   +-- mod.rs
    |   +-- app.rs           # App state machine
    |   +-- dashboard.rs     # Landing screen
    |   +-- browser.rs       # Dual-pane file manager
    |   +-- preview.rs       # Bottom metadata bar
    |   +-- export.rs        # Copy-to-target logic
    +-- dump/
        +-- mod.rs
        +-- recover.rs       # Bare-mode blob extraction
```

## Dependencies

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing |
| `ratatui` + `crossterm` | TUI framework |
| `fuser` | FUSE filesystem (pure Rust) |
| `sqlx` | PostgreSQL async driver |
| `rusqlite` | SQLite for offline metadata |
| `sha2` | SHA-256 blob verification |
| `infer` | Magic-byte MIME detection |
| `indicatif` | Progress bars for dump/export |
| `tokio` | Async runtime |

## Hardcoded Knowledge About OxiCloud

The only coupling to OxiCloud (intentionally minimal):

- **Blob path formula**: `{blob_root}/{hash[0..2]}/{hash}.blob`
- **PostgreSQL schema**: `storage.files`, `storage.folders`, `storage.blobs`, `auth.users`
- **Hash algorithm**: SHA-256, 64-char hex string

If the OxiCloud schema changes, only `db/postgres.rs` and `export/metadata.rs` need updating.
