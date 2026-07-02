{
  description = "Immich transpiler framework — transcode originals to AV1/JXL";

  inputs = {
    rs-harbor.url = "path:/data/nvme0/can/Projects/rs-harbor";
    nixpkgs.follows = "rs-harbor/nixpkgs";
    rust-overlay.follows = "rs-harbor/rust-overlay";
    crane.follows = "rs-harbor/crane";
    flake-utils.follows = "rs-harbor/flake-utils";
  };

  outputs =
    { self, nixpkgs, rs-harbor, flake-utils, rust-overlay, ... }@inputs:
    let
      nixosModule = import ./nixos-modules/immich-convert-originals.nix;
    in
    {
      nixosModules.immich-convert-originals = nixosModule;
      nixosModules.default = nixosModule;
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        toolchain = rs-harbor.lib.mkToolchain {
          inherit pkgs;
          channel = "stable";
          extensions = [ "rust-src" "rustfmt" "clippy" "llvm-tools-preview" ];
          withRustAnalyzer = false;
          crossTargets = [ "x86_64-unknown-linux-gnu" ];
        };
        inherit (toolchain) craneLib;
        cross = rs-harbor.lib.mkCross {
          inherit pkgs system;
        };
        cargoConfig = rs-harbor.lib.mkCargoConfig {
          inherit pkgs;
          channel = "stable";
          enableCranelift = false;
          enableShareGenerics = false;
          enableParallelFrontend = false;
        };

        src = craneLib.cleanCargoSource ./.;
        cargoVendorDir = craneLib.vendorCargoDeps {
          inherit src;
        };

        nativeBuildInputs = with pkgs; [
          pkg-config
          clang
          mold
        ];
        buildInputs = with pkgs; [
          ffmpeg
          libvmaf
          svt-av1
          openssl
        ];

        commonArgs = {
          inherit src cargoVendorDir;
          strictDeps = true;
          inherit nativeBuildInputs buildInputs;
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };

        package = craneLib.buildPackage (
          commonArgs
          // {
            pname = "rs-immicher-oxide";
          }
        );
      in
      {
        packages.default = package;

        checks = {
          clippy = craneLib.mkCargoDerivation (
            commonArgs
            // {
              cargoArtifacts = null;
              pnameSuffix = "-clippy";
              buildPhaseCargoCommand = "cargoWithProfile clippy --locked --all-targets -- --deny warnings";
            }
          );
          fmt = craneLib.cargoFmt {
            inherit src;
          };
        };

        devShells = rs-harbor.lib.mkDevShells {
          inherit pkgs cross cargoConfig;
          inherit (toolchain) craneLib;
          pkgConfigDeps = buildInputs;
          packages = nativeBuildInputs ++ buildInputs;
        };
      }
    );
}
