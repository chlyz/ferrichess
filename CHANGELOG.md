# Changelog

All notable changes to Ferrichess are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A pull-only top-level `ferrichess study pull` workflow for snapshotting
  configured authoritative Lichess studies and rebuilding stable local FEN
  indexes without modifying remote studies.
- A source-neutral `ferrichess-games` PGN parser and continuation-statistics
  library.
- A publish-disabled `ferrichess-archive` CLI for local Chess.com/Lichess raw
  snapshots, combined PGN, SQLite storage, and opening reports.
- A variation-aware `ferrichess-pgn-index` CLI for building course-specific
  FEN indexes from one or more multi-game PGNs.
- Lichess opening-explorer position reports with candidate filtering, result
  scores, optional cached cloud evaluations, and local Stockfish MultiPV
  analysis with automatic fallback, five unrestricted lines, and a default
  search depth of 20.
- Explicit per-chapter repertoire side, role, and alternative-label metadata.

### Changed

- Continue development in public while disabling crates.io publication.
- Define `course.pgn` as every game from every chapter in course order, while
  repertoire side-full trees accept only `Main` chapters. Tactics courses do
  not produce side-full trees.

## [0.2.0] - 2026-07-18

### Added

- Exact-ply raw comment directives for preserving PGN graphical annotations
  without interrupting compact mainline parsing.

### Added

- Public-release documentation, contribution guidance, and continuous
  integration checks.
- The `ferrichess-study` compact-format and API contract.

### Changed

- Package licensing and test-data provenance are documented for the first
  public release.
