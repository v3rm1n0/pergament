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
        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./src
            ./tests
            ./gtk
          ];
        };
        meta = {
          description = "Unofficial native Linux reader for JW publications (.jwpub)";
          platforms = pkgs.lib.platforms.linux;
        };

        jwlinux = pkgs.rustPlatform.buildRustPackage {
          pname = "jwlinux";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "jwlinux"
          ];
          cargoTestFlags = [
            "-p"
            "jwlinux"
          ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];
          meta = meta // {
            mainProgram = "jwl";
          };
        };

        jwlinux-gtk = pkgs.rustPlatform.buildRustPackage {
          pname = "jwlinux-gtk";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "jwlinux-gtk"
          ];
          cargoTestFlags = [
            "-p"
            "jwlinux-gtk"
          ];
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.wrapGAppsHook4
          ];
          buildInputs = [ pkgs.sqlite ] ++ guiLibs;
          postInstall = ''
            install -Dm644 gtk/data/io.github.jwlinux.Reader.desktop -t /share/applications
          '';
          meta = meta // {
            mainProgram = "jwlinux-gtk";
          };
        };
      in
      {
        packages = {
          default = jwlinux;
          inherit jwlinux jwlinux-gtk;
        };
        checks = {
          inherit jwlinux jwlinux-gtk;
        };

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
