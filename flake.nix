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
            ./assets
            ./src
            ./tests
            ./src-tauri
            ./ui
            ./package.json
            ./bun.lock
            ./tsconfig.json
            ./vite.config.ts
          ];
        };
        meta = {
          description = "Pergament, an unofficial Linux reader for .jwpub publications";
          platforms = pkgs.lib.platforms.linux;
          license = pkgs.lib.licenses.gpl3Only;
        };

        pergament = pkgs.rustPlatform.buildRustPackage (finalAttrs: {
          pname = "pergament";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;

          # React frontend, built into dist/ and embedded into pergament-app.
          # node_modules is fetched in a fixed-output derivation from bun.lock.
          nodeModules = pkgs.stdenvNoCC.mkDerivation {
            pname = "pergament-node-modules";
            inherit version;
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.unions [
                ./package.json
                ./bun.lock
              ];
            };
            nativeBuildInputs = [ pkgs.bun ];
            dontConfigure = true;
            dontFixup = true;
            buildPhase = ''
              runHook preBuild
              export HOME=$TMPDIR
              bun install --frozen-lockfile --ignore-scripts --no-progress
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              cp -r node_modules $out
              runHook postInstall
            '';
            outputHashMode = "recursive";
            outputHashAlgo = "sha256";
            outputHash = "sha256-dc8Z1aEMhwvHYa8YCbwxTguyvppRsadxYxBJt1Jb7/Y=";
          };

          cargoBuildFlags = [
            "-p"
            "pergament"
            "-p"
            "pergament-app"
            "--features"
            "pergament-app/custom-protocol"
          ];
          cargoTestFlags = [
            "-p"
            "pergament"
            "-p"
            "pergament-app"
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
            nodejs_22
            bun
            wrapGAppsHook3
            copyDesktopItems
          ];
          buildInputs = [ pkgs.sqlite ] ++ tauriLibs
            # WebKitGTK plays media through GStreamer; the recordings are H.264/AAC MP4,
            # which needs the libav decoders.
            ++ (with pkgs.gst_all_1; [
              gstreamer
              gst-plugins-base
              gst-plugins-good
              gst-plugins-bad
              gst-libav
            ]);

          preBuild = ''
            cp -r ${finalAttrs.nodeModules} node_modules
            chmod -R u+w node_modules
            patchShebangs node_modules
            bun run build
          '';

          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "io.github.v3rm1n.pergament";
              desktopName = "Pergament";
              genericName = "Publication Reader";
              comment = meta.description;
              exec = "pergament-app";
              startupWMClass = "pergament-app";
              icon = "io.github.v3rm1n.pergament";
              categories = [
                "Office"
                "Viewer"
              ];
              keywords = [
                "jwpub"
                "Bible"
                "reader"
              ];
            })
          ];

          postInstall = ''
            for size in 32 64 128 256; do
              install -Dm644 src-tauri/icons/''${size}x''${size}.png \
                $out/share/icons/hicolor/''${size}x''${size}/apps/io.github.v3rm1n.pergament.png
            done
            install -Dm644 src-tauri/icons/icon.png \
              $out/share/icons/hicolor/512x512/apps/io.github.v3rm1n.pergament.png
            install -Dm644 src-tauri/icons/icon.svg \
              $out/share/icons/hicolor/scalable/apps/io.github.v3rm1n.pergament.svg
          '';

          meta = meta // {
            mainProgram = "pergament-app";
          };
        });
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

            # Frontend (React + Vite) and Tauri CLI via bun
            nodejs_22
            bun

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

          buildInputs =
            [ pkgs.sqlite ]
            ++ tauriLibs
            # WebKitGTK plays media through GStreamer. The recordings are H.264/AAC
            # MP4, which needs the libav decoders.
            ++ (with pkgs.gst_all_1; [
              gstreamer
              gst-plugins-base
              gst-plugins-good
              gst-plugins-bad
              gst-libav
            ]);

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
