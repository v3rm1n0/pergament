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
      in
      {
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

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
