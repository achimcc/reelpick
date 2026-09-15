# nix/test.nix — filled in by task 11
{ pkgs, module, package }:
pkgs.runCommand "reelpick-vm-placeholder" { } "echo ${package} > $out"
