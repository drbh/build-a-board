{
  description = "bb: build, route, check, and release KiCad boards described as WebAssembly components";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # A Rust toolchain that can target wasm32-wasip2, which board crates build for.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; }));
      toolchain = pkgs: pkgs.rust-bin.stable.latest.default.override {
        targets = [ "wasm32-wasip2" ];
        extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
      };
    in
    {
      packages = forAllSystems (pkgs:
        let
          # The exact router release the reviewed routes were accepted with.
          freerouting = pkgs.fetchurl {
            url = "https://github.com/freerouting/freerouting/releases/download/v2.0.1/freerouting-2.0.1.jar";
            hash = "sha256-1/0PY/UubXSw+tZxX4fKnw/9cQnWaypYRjgAAnBZLs8=";
          };
          jdk = pkgs.jdk21_headless;
          linux = pkgs.stdenv.hostPlatform.isLinux;
          rust = toolchain pkgs;
        in
        {
          inherit freerouting;

          bb = pkgs.rustPlatform.buildRustPackage {
            pname = "build-a-board";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "build-a-board" ];
            # Tests build board crates for wasm32-wasip2; run them with `nix develop -c cargo test`.
            doCheck = false;
            # build.rs compiles the vendored Clipper2 (C++17 from stdenv);
            # release zips are deflated by zlib itself.
            nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
            buildInputs = [ pkgs.zlib ];
            # Pinned tools become defaults; the environment still overrides them.
            # cargo (with the wasm32-wasip2 target) builds board crates; KiCad
            # comes from nixpkgs on Linux and /Applications/KiCad on macOS.
            postInstall = ''
              wrapProgram $out/bin/bb \
                --suffix PATH : ${rust}/bin \
                --set-default FREEROUTING_JAR ${freerouting} \
                --set-default JAVA ${jdk}/bin/java \
                --set-default MAGICK ${pkgs.imagemagick}/bin/magick \
                --set-default BB_PYTHON ${pkgs.python3}/bin/python3 ${pkgs.lib.optionalString linux ''\
                --set-default KICAD_CLI ${pkgs.kicad}/bin/kicad-cli \
                --set-default KICAD_FOOTPRINT_DIR ${pkgs.kicad.libraries.footprints}/share/kicad/footprints''}
            '';
            meta.mainProgram = "bb";
          };

          default = self.packages.${pkgs.stdenv.hostPlatform.system}.bb;
        });

      devShells = forAllSystems (pkgs:
        let
          p = self.packages.${pkgs.stdenv.hostPlatform.system};
          jdk = pkgs.jdk21_headless;
          linux = pkgs.stdenv.hostPlatform.isLinux;
        in
        {
          default = pkgs.mkShell {
            packages = [
              (toolchain pkgs)
              pkgs.pkg-config
              pkgs.zlib
              jdk
              pkgs.imagemagick
              # Only for a board project's own external check scripts.
              pkgs.python3
              pkgs.git
            ]
            # nixpkgs marks KiCad broken on Darwin, so macOS keeps using
            # /Applications/KiCad (bb's default); KICAD_CLI and
            # KICAD_FOOTPRINT_DIR select another install.
            ++ pkgs.lib.optional linux pkgs.kicad;

            FREEROUTING_JAR = "${p.freerouting}";
            JAVA = "${jdk}/bin/java";
            MAGICK = "${pkgs.imagemagick}/bin/magick";
            BB_PYTHON = "${pkgs.python3}/bin/python3";

            shellHook = pkgs.lib.optionalString linux ''
              export KICAD_CLI=${pkgs.kicad}/bin/kicad-cli
              export KICAD_FOOTPRINT_DIR=${pkgs.kicad.libraries.footprints}/share/kicad/footprints
            '';
          };
        });
    };
}
