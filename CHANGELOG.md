# Changelog

## [Unreleased]

### Added

- `TranscodedPayload` enum with `Reader` and `File` variants for memory-efficient
  file-backed transcoder output.
- `ImmichApiClient::upload_asset_file()` for streaming file uploads without
  buffering the entire payload into memory.
- Concurrent asset processing in the pipeline via `Pipeline::with_concurrency()`
  and `buffer_unordered`.
- Immich source discovery rewritten as a state-machine stream using
  `futures::stream::unfold` for proper pagination and backpressure.
- Crate metadata for crates.io: `description`, `documentation`, `readme`,
  `keywords`, `categories`, `include`, `repository`, `rust-version`.
- `docs.rs` metadata (`all-features`, `rustdoc-args`) on the backend-immich,
  codec-av1, and codec-jxl crates.
- README files for all crates (`core`, `cli`, `backend-immich`, `codec-av1`,
  `codec-jxl`).
- Immich source integration tests in `backend-immich/tests/source_tests.rs`.
- Forgejo CI workflows for all crates and publish pipelines.
- `simit.toml` project configuration.
- `.opencode/opencode.jsonc` configuration.
- `LICENSE` (AGPL-3.0-only) and `keys/maintainers.gpg`.
- `RecordingState` mock for testing concurrent pipeline state recording.

### Changed

- Crate sources restructured from flat top-level directories into
  `crates/` subdirectory.
- Pipeline internals use `Arc<dyn T>` instead of `Box<dyn T>` to support
  shared ownership in concurrent processing.
- AV1 transcoder (`codec-av1`) uses `std::io::copy` instead of
  `read_to_end` for streaming input; output uses `TranscodedPayload::File`.
- JXL transcoder (`codec-jxl`) output uses `TranscodedPayload::Reader`;
  `encode_pixels` extracted into a dedicated `impl` block.
- Immich sink matches on `TranscodedPayload` variants: reads `Reader` into
  memory or streams `File` via `upload_asset_file`.
- Workspace `Cargo.toml` uses `crates/*` glob for member discovery.
- Workspace-level metadata: `rust-version = "1.93"`, `repository`,
  `[workspace.metadata.crane]`.

### Fixed

- Test cleanup: `set_body_json(&body)` → `set_body_json(body)` and
  `set_body_json(&make_search_response(...))` → `set_body_json(make_search_response(...))`
  in `client_tests.rs`.
