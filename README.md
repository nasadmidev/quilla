# Quilla

English | [Español](README.es.md)

Quilla is an experimental package manager written in Rust. It combines straightforward package installation (Pacman) with an immutable store and generation-based profiles (Nix). The project is under development and has so far been tested with the Arch Linux repositories on `x86_64`.

> [!IMPORTANT]
> Currently, it operates in a single-threaded manner with `reqwest` configured in blocking mode, so you may experience long wait times when installing a package. An asynchronous or concurrent approach will be added in the near future.

## How it works

When installing a package, Quilla:

1. Reads the configured repository indexes and resolves dependencies.
2. Downloads package archives and checks their SHA-256 hashes against the values in the repository index.
3. Extracts packages and stores them by package, version, and architecture. Executable files keep their execute permission; the store is read-only.
4. Creates a generation that brings together package files through symbolic links and updates the profile's `current` link.
5. Saves the local package and generation database as JSON.

Files for each package version remain in the store. A generation represents the set of files available in a profile.

## Current status

Installation (`install` or `-S`) is the only operation implemented in the normal workflow. The `remove` (`-R`), `upgrade` (`-Syu`), and `rollback` commands are temporary placeholders; they do not modify packages, generations, or profiles yet.

Quilla is an early-stage project. Package archives are currently checked against the SHA-256 hashes in the downloaded repository index; cryptographic signatures on repository indexes are not verified yet. Try it in a test environment, not on a system you need to keep stable.

## Build

Rust and Cargo are required. From the project root, run:

```sh
cargo build --release
```

The executable is created at `target/release/quilla`.

## Configuration

Quilla loads a TOML configuration file. Set the `QUILLA_CONFIG` environment variable to specify its location. The file must define paths for the cache, store, profile, and database, as well as one or more repositories.

Example configuration for Arch Linux's official `core` and `extra` repositories:

```toml
[paths]
cache_dir = "/home/user/.cache/quilla"
store_dir = "/home/user/quilla/store"
profile_dir = "/home/user/quilla/profile"
database_path = "/home/user/quilla/database.json"

[[repositories]]
name = "core"
index_path = "core.db"
mirrors = ["https://geo.mirror.pkgbuild.com/$repo/os/$arch"]

[[repositories]]
name = "extra"
index_path = "extra.db"
mirrors = ["https://geo.mirror.pkgbuild.com/$repo/os/$arch"]
```

Quilla replaces `$repo` with the repository name and `$arch` with the detected architecture. You can add more mirrors to each repository's `mirrors` list.

## Usage

Set the configuration file path when running Quilla:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla install bash
```

You can install multiple packages in one operation:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla install bash gzip
```

The short form of `install` is `-S`:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla -S bash
```

The active profile is available at `profile_dir/current`, which points to a generation under `profile_dir/generations/`. The local database is stored at `database_path`.

## Project structure

```text
src/
├── main.rs          # Program entry point and command coordination
├── lib.rs           # Modules and shared utilities
├── utils.rs         # Utils module
├── fetch.rs         # Fetching module / Downloads and cache
├── core.rs          # Core module
├── core/
├──── db.rs          # Local package and generation database
├──── generations.rs # Profile and generation creation and activation
├──── package.rs     # Package metadata, versions, and dependencies
├──── repo.rs        # Pacman repository index parsing
├──── resolver.rs    # Dependency and conflict resolution
├──── store.rs       # Hash-based package store
├──── transaction.rs # Installation workflow
├── fetch/
├──── verify.rs      # SHA-256 package checks
├── utils/
├──── cli.rs         # Command-line parsing
├──── config.rs      # TOML configuration loading
├──── error.rs       # Shared error type
```
