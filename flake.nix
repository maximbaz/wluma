{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, naersk }:
    let systems = [ "x86_64-linux" "aarch64-linux" ];
    in flake-utils.lib.eachSystem systems (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        libs = with pkgs; [
          udev
          v4l-utils
          vulkan-loader
          dbus
        ];
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          src = ./.;
          meta.mainProgram = "wluma";
          nativeBuildInputs = with pkgs; [
            makeWrapper
            pkg-config
            rustPlatform.bindgenHook
            marked-man
          ];
          buildInputs = libs;
        };
        devShell = with pkgs; mkShell {
          buildInputs = [ cargo rustc rustfmt pre-commit rustPackages.clippy pkg-config ] ++ libs;
          LD_LIBRARY_PATH = "${lib.makeLibraryPath [ wayland ]}";
          VK_LAYER_PATH = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
          VK_INSTANCE_LAYERS = "VK_LAYER_KHRONOS_validation";
          VK_LOADER_DEBUG = "error,warn";
          RUST_SRC_PATH = rustPlatform.rustLibSrc;
          LIBCLANG_PATH = "${llvmPackages_12.libclang.lib}/lib";
          BINDGEN_EXTRA_CLANG_ARGS = [
            ''-I"${llvmPackages_12.libclang.lib}/lib/clang/${llvmPackages_12.libclang.version}/include"''
          ] ++ (builtins.map (a: ''-I"${a}/include"'') [
            glibc.dev
            libv4l.dev
          ]);
        };
      }
    );
}
