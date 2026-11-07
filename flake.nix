{
  description = "Perfwork - Modern Perfboard & Stripboard CAD Designer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        darwinDeps = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
          pkgs.apple-sdk_15
          pkgs.libiconv
        ];

        linuxDeps = lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.pkg-config
          pkgs.libxkbcommon
          pkgs.libGL
          pkgs.fontconfig
          pkgs.wayland
          pkgs.xorg.libXcursor
          pkgs.xorg.libXrandr
          pkgs.xorg.libXi
          pkgs.xorg.libX11
        ];

        runtimeLibs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.libxkbcommon
          pkgs.libGL
          pkgs.wayland
          pkgs.xorg.libXcursor
          pkgs.xorg.libXrandr
          pkgs.xorg.libXi
          pkgs.xorg.libX11
        ];
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "perfwork";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [
            pkgs.pkg-config
            pkgs.makeWrapper
          ];

          buildInputs = darwinDeps ++ linuxDeps;

          postInstall = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            wrapProgram $out/bin/perfwork \
              --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
          '';

          meta = with lib; {
            description = "Modern Perfboard, Stripboard & Breadboard CAD Designer";
            license = licenses.mit;
            mainProgram = "perfwork";
          };
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/perfwork";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            pkgs.clippy
            pkgs.rust-analyzer
            pkgs.trunk
            pkgs.lld
          ] ++ darwinDeps ++ linuxDeps;

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      }
    );
}
