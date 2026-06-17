# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/ghismary/embedded-sgp30/compare/v0.1.0...v0.2.0) - 2026-06-17

### Added

- Add async implementation and update dependencies

### Fixed

- Update tests so that they can build on platforms other than Linux

### Other

- Improve code coverage
- Add github and codecov badges
- Add zed tasks
- Add github actions for CI

## 0.1.0 - 2024-01-25

### Added

- Add licence files.

### Fixed

- Do not use complex transactions with multiple reads and writes as it does not work on all plaforms (at least not working for with linux-embedded-hal on an orangepi4-lts board).
- Add missing i2c.set_slave_address() in the examples.
