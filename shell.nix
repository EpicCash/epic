{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = with pkgs; [
    rustup
    lmdb
    cmake
    clang
    ncurses
  ];
  name = "Epic";
  shellHook = ''
  export LIBCLANG_PATH=${pkgs.llvmPackages.libclang}/lib
  '';
}
