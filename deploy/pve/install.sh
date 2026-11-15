#!/usr/bin/env bash
# Install the AOOSTAR homelab panels on a Proxmox VE host.
#
#   ./install.sh             install or update files in /opt/aoostar (does not start anything)
#   ./install.sh --enable    switch from aoostar-lcd.service to the homelab panels
#   ./install.sh --rollback  switch back to aoostar-lcd.service
#
# Existing files in /opt/aoostar are left alone, except bin/, fonts/ and panels/img/, which are
# backed up before being replaced. User-edited configuration files are never overwritten.
set -euo pipefail

PREFIX=/opt/aoostar
SRC="$(cd "$(dirname "$0")" && pwd)"
NEW_UNITS=(aoostar-sysinfo.service aoostar-homelab.service aoostar-panels.service)
OLD_UNIT=aoostar-lcd.service

[[ $EUID -eq 0 ]] || { echo "Please run as root" >&2; exit 1; }

install_files() {
    if ! id aoostar &>/dev/null; then
        useradd --system --no-create-home --home-dir "$PREFIX" --shell /usr/sbin/nologin aoostar
        echo "Created system user aoostar"
    fi

    local backup="$PREFIX/backup/$(date +%Y%m%d-%H%M%S)"
    for dir in bin fonts panels; do
        if [[ -d "$PREFIX/$dir" ]]; then
            mkdir -p "$backup"
            cp -a "$PREFIX/$dir" "$backup/"
        fi
    done
    [[ -d "$backup" ]] && echo "Backup of previous files: $backup"

    install -d -m 0755 "$PREFIX" "$PREFIX/bin" "$PREFIX/fonts" "$PREFIX/panels" "$PREFIX/panels/img"
    install -m 0755 "$SRC"/bin/* "$PREFIX/bin/"
    install -m 0644 "$SRC"/fonts/* "$PREFIX/fonts/"
    install -m 0644 "$SRC"/panels/img/* "$PREFIX/panels/img/"
    install -m 0644 "$SRC"/panels/homelab.json "$SRC"/panels/generate.py "$PREFIX/panels/"
    install -m 0644 "$SRC/setup-pve-token.sh" "$SRC/setup-pbs-token.sh" "$PREFIX/"

    # configuration files: install, or place the new version next to the existing one
    install_config "$SRC/panels/sensor-mapping.cfg" "$PREFIX/panels/sensor-mapping.cfg" 0644 root
    install_config "$SRC/panels/sensor-mapping-filter.cfg" "$PREFIX/panels/sensor-mapping-filter.cfg" 0644 root
    install_config "$SRC/panels.env" "$PREFIX/panels.env" 0644 root
    install_config "$SRC/homelab.example.toml" "$PREFIX/homelab.toml" 0640 aoostar

    # API secrets: readable by the aoostar service user only
    install -d -m 0750 -o root -g aoostar "$PREFIX/secrets"

    install -m 0644 "$SRC"/systemd/*.service /etc/systemd/system/
    systemctl daemon-reload
    echo "Installed to $PREFIX"
}

install_config() {
    local src=$1 dst=$2 mode=$3 group=$4
    if [[ -e "$dst" ]]; then
        if ! cmp -s "$src" "$dst"; then
            install -m "$mode" -o root -g "$group" "$src" "$dst.new"
            echo "Kept $dst, new version: $dst.new"
        fi
    else
        install -m "$mode" -o root -g "$group" "$src" "$dst"
    fi
}

enable_panels() {
    if systemctl list-unit-files "$OLD_UNIT" &>/dev/null; then
        systemctl disable --now "$OLD_UNIT" || true
    fi
    systemctl enable --now "${NEW_UNITS[@]}"
    systemctl --no-pager status "${NEW_UNITS[@]}" | grep -E "●|Active:" || true
    echo
    echo "Logs: journalctl -fu aoostar-panels -u aoostar-homelab"
}

rollback() {
    systemctl disable --now "${NEW_UNITS[@]}" || true
    if systemctl list-unit-files "$OLD_UNIT" &>/dev/null; then
        systemctl enable --now "$OLD_UNIT"
        echo "Switched back to $OLD_UNIT"
    fi
}

case "${1:-}" in
    "") install_files
        echo
        echo "Next steps: create API credentials, edit $PREFIX/homelab.toml, then run: $0 --enable"
        ;;
    --enable) enable_panels ;;
    --rollback) rollback ;;
    *) echo "Usage: $0 [--enable|--rollback]" >&2; exit 1 ;;
esac
