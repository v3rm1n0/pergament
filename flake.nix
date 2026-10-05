{
  description = "jwlinux - unofficial native Linux reader for JW publications";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Native libraries for the later GTK4/libadwaita + WebKitGTK reader.
        guiLibs = with pkgs; [
          gtk4
          libadwaita
          webkitgtk_6_0
          glib
          cairo
          pango
          gdk-pixbuf
          graphene
        ];
        jwlinux = pkgs.rustPlatform.buildRustPackage {
          pname = "jwlinux";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              ./tests
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];
          meta = {
            description = "Unofficial native Linux reader for JW publications (.jwpub)";
            mainProgram = "jwl";
            platforms = pkgs.lib.platforms.linux;
          };
        };
      in
      {
        packages.default = jwlinux;
        packages.jwlinux = jwlinux;
        checks.default = jwlinux;

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            # Rust toolchain (stable, from nixpkgs)
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            cargo-nextest
            cargo-deny

            pkg-config
            wrapGAppsHook4

            # Inspecting .jwpub files (zip -> manifest.json + contents zip -> SQLite)
            sqlite-interactive
            unzip
            zip
            jq
            file
            xxd
            hexyl
          ];

          buildInputs =
            (with pkgs; [
              sqlite
              openssl
            ])
            ++ guiLibs;

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          RUST_BACKTRACE = "1";

          shellHook = ''
            echo "jwlinux dev shell: $(rustc --version)"
            [ -n "$JWL_TEST_JWPUB" ] || echo "hint: export JWL_TEST_JWPUB=/path/to/nwtsty_X.jwpub for fixture tests"
          '';
        };

        formatter = pkgs.nixfmt;
      }
    );
}
