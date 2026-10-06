{
  description = "Pergament - unofficial Linux reader for .jwpub publications";

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

        # Native libraries for the Tauri v2 desktop app (WebKitGTK 4.1 + GTK3).
        tauriLibs = with pkgs; [
          webkitgtk_4_1
          gtk3
          libsoup_3
          glib
          glib-networking
          cairo
          pango
          gdk-pixbuf
          atk
          librsvg
          dbus
          openssl
        ];

        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./src
            ./tests
            ./src-tauri
          ];
        };
        meta = {
          description = "Pergament, an unofficial Linux reader for .jwpub publications";
          platforms = pkgs.lib.platforms.linux;
          license = pkgs.lib.licenses.gpl3Plus;
        };

        pergament = pkgs.rustPlatform.buildRustPackage {
          pname = "pergament";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "pergament"
          ];
          cargoTestFlags = [
            "-p"
            "pergament"
          ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];
          meta = meta // {
            mainProgram = "pergament";
          };
        };
      in
      {
        packages = {
          default = pergament;
          inherit pergament;
        };
        checks = {
          inherit pergament;
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

            # Frontend (React + Vite) and Tauri CLI via pnpm
            nodejs_22
            pnpm

            pkg-config
            wrapGAppsHook3

            # Inspecting .jwpub files (zip -> manifest.json + contents zip -> SQLite)
            sqlite-interactive
            unzip
            zip
            jq
            file
            xxd
            hexyl
          ];

          buildInputs = [ pkgs.sqlite ] ++ tauriLibs;

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          RUST_BACKTRACE = "1";

          shellHook = ''
            # GTK file dialogs need the schemas; TLS in WebKit needs glib-networking.
            export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS"
            export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
            echo "Pergament dev shell: $(rustc --version), node $(node --version)"
            [ -n "$PERGAMENT_TEST_JWPUB" ] || echo "hint: export PERGAMENT_TEST_JWPUB=/path/to/nwtsty_X.jwpub for fixture tests"
          '';
        };

        formatter = pkgs.nixfmt;
      }
    );
}
