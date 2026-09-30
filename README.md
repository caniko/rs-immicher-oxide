# rs-immicher-oxide

<!-- simit:badges:start -->

![CI](https://img.shields.io/badge/CI-managed-2088ff) [![docs](https://img.shields.io/badge/docs-enabled-6f42c1)](https://docs.rs/rs-immicher-oxide) [![crates.io](https://img.shields.io/badge/crates.io-ready-f46623)](https://crates.io/crates/rs-immicher-oxide)

<!-- simit:badges:end -->

Immich media transcoding pipeline for replacing original assets with modern
encodes: JPEG XL for images and AV1/Opus MP4 for videos.

The workspace is split into small crates so the Immich API backend, pipeline
orchestration, and codec implementations can evolve independently. The CLI
crate wires those pieces together for one-shot conversion, polling daemon mode,
and a small webhook listener.

## Status

This project is early-stage software. The default CLI mode is deliberately
conservative: `run` and `watch` default to dry-run discovery, so they log the
assets that would be handled without transcoding, uploading, or deleting
anything.

When dry-run is disabled, the sink lifecycle is:

1. upload the transcoded asset to Immich,
2. copy selected metadata from the original,
3. verify that the new asset is readable,
4. delete the original asset through the Immich API.

Use a test Immich library or a fresh backup before processing irreplaceable
media.

## Workspace

| Crate | Purpose |
| --- | --- |
| `rs-immicher-oxide` | CLI runtime in `crates/cli` |
| `rs-immicher-oxide-core` | Pipeline traits, state, outcomes, and shared media types |
| `rs-immicher-oxide-backend-immich` | Immich asset discovery, download, upload, metadata copy, and deletion |
| `rs-immicher-oxide-codec-jxl` | JPEG XL image transcoder |
| `rs-immicher-oxide-codec-av1` | SVT-AV1 video transcoder via `avio` |

The repository also exports a NixOS module at
`nixosModules.immich-convert-originals`.

## Requirements

- Rust 1.93 or newer.
- Immich server URL and API key.
- Native build dependencies from the flake, including FFmpeg, SVT-AV1,
  libvmaf, OpenSSL, Clang, pkg-config, and mold.
- VAAPI-capable render device for the AV1 path when using the current video
  transcoder configuration.

The easiest development environment is the Nix shell:

```sh
nix develop
```

## Build And Test

```sh
nix flake check
nix build
```

Inside the dev shell:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- --deny warnings
cargo fmt --check
```

## CLI Usage

The CLI reads `IMMICH_URL` and `IMMICH_API_KEY`, or the same values can be
passed as flags.

```sh
export IMMICH_URL="http://localhost:2283"
export IMMICH_API_KEY="..."
```

Discover assets without changing Immich. This is the default write mode:

```sh
rs-immicher-oxide run
rs-immicher-oxide run --write-mode dry-run
```

Process only images or only videos:

```sh
rs-immicher-oxide run --image-only
rs-immicher-oxide run --video-only
```

Run a bounded production canary without touching originals:

```sh
rs-immicher-oxide run \
  --write-mode upload-only \
  --asset-id IMAGE_ASSET_ID \
  --asset-id VIDEO_ASSET_ID \
  --state-path ./state.jsonl \
  --manifest-path ./manifest.jsonl
```

`upload-only` uploads replacements, copies supported metadata, verifies the new
asset through the Immich API, and leaves originals untouched. `trash-original`
is the only mode that moves originals to Immich trash after verification. The
CLI does not expose force-delete.

Tune codecs:

```sh
rs-immicher-oxide run --image-distance 1.0 --video-crf 20 --concurrency 2
```

Poll for newly discoverable assets:

```sh
rs-immicher-oxide watch --interval 5min
```

Run the webhook listener:

```sh
rs-immicher-oxide serve --bind 0.0.0.0:8088
```

Webhook mode is dry-run only until single-asset webhook processing is
implemented.

`RUST_LOG` controls tracing output:

```sh
RUST_LOG=debug rs-immicher-oxide run
```

## Supported Media Paths

Images are discovered from Immich image assets and transcoded to JPEG XL when
the source codec is not already JXL. Supported source image formats are JPEG,
PNG, WebP, HEIC/HEIF, AVIF, TIFF, GIF, and BMP.

Videos are discovered from Immich video assets and transcoded to AV1 in an MP4
container when the source codec is supported by the AV1 transcoder. The current
video path accepts H.264, H.265, AV1, and VP9 inputs, uses VAAPI decode, SVT-AV1
encode, Opus audio, and MP4 faststart output.

## NixOS Module

The flake provides `nixosModules.immich-convert-originals`, which configures a
systemd service around `rs-immicher-oxide watch`.

Example:

```nix
{
  inputs.rs-immicher-oxide.url = "git+https://github.com/caniko/rs-immicher-oxide.git";

  outputs = { self, nixpkgs, rs-immicher-oxide, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        rs-immicher-oxide.nixosModules.immich-convert-originals
        {
          services.immich-convert-originals = {
            enable = true;
            package = rs-immicher-oxide.packages.x86_64-linux.default;
            immichUrl = "http://localhost:2283";
            apiKeyFile = "/run/secrets/immich-convert-api-key";
            writeMode = "dry-run";
            pollInterval = "5min";
            concurrency = 1;
            assetIds = [];
            statePath = "/var/lib/rs-immicher-oxide/state.jsonl";
            manifestPath = "/var/lib/rs-immicher-oxide/manifest.jsonl";
          };
        }
      ];
    };
  };
}
```

The secret file is consumed as a systemd `EnvironmentFile`, so it must define
the API key in environment-file syntax:

```sh
IMMICH_API_KEY=...
```

Before switching a production system from `dry-run` to `upload-only`, create
and validate a fresh Immich PostgreSQL dump plus a readonly btrfs snapshot of
the media location. If either artifact is missing, do not run production writes.

## Repository Maintenance

CI and publish workflows are generated by `simit` for Forgejo Actions on
Codeberg. The repository uses Nix-backed CI on `atlas-nix-trusted` and publishes
each crate independently.

Useful checks:

```sh
simit init ci --platform forgejo --check --diff
simit release trust check
```

## License

Licensed under `AGPL-3.0-only`. See `LICENSE`.
