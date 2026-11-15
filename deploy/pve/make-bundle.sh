#!/usr/bin/env bash
# Assemble the Proxmox host install bundle from release binaries.
#
#   deploy/pve/make-bundle.sh <directory with asterctl, aster-sysinfo, aster-homelab>
#
# Creates dist/aoostar-homelab-x86_64.tar.gz
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN_DIR=${1:?usage: $0 <binary directory>}
NAME=aoostar-homelab
OUT="$ROOT/dist/$NAME"

rm -rf "$OUT"
mkdir -p "$OUT"/{bin,fonts,panels/img,systemd}
cp "$BIN_DIR"/{asterctl,aster-sysinfo,aster-homelab} "$OUT/bin/"
cp "$ROOT"/fonts/*.ttf "$OUT/fonts/"
cp "$ROOT"/cfg/homelab/{homelab.json,generate.py,sensor-mapping.cfg,sensor-mapping-filter.cfg} "$OUT/panels/"
cp "$ROOT"/cfg/homelab/img/* "$OUT/panels/img/"
cp "$ROOT"/deploy/pve/*.service "$OUT/systemd/"
cp "$ROOT"/deploy/pve/{install.sh,setup-pve-token.sh,setup-pbs-token.sh,panels.env} "$OUT/"
cp "$ROOT"/crates/aster-homelab/homelab.example.toml "$OUT/"
chmod 0755 "$OUT"/install.sh "$OUT"/bin/*

COPYFILE_DISABLE=1 tar --no-xattrs --no-mac-metadata -C "$ROOT/dist" -czf "$ROOT/dist/$NAME-x86_64.tar.gz" "$NAME"
echo "$ROOT/dist/$NAME-x86_64.tar.gz"
