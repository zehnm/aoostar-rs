#!/usr/bin/env bash
# Create a read-only Proxmox VE API token for aster-homelab: user aster@pve, role PVEAuditor.
#
# Run as root on each Proxmox host (or once per cluster). Prints the token secret on stdout and
# the certificate fingerprint on stderr, so it can be used remotely from the LCD host:
#
#   ssh root@pve-b bash -s < /opt/aoostar/setup-pve-token.sh > /opt/aoostar/secrets/pve-b-token
#
# To create a new secret, remove the old token first: pveum user token remove aster@pve lcd
set -uo pipefail

USER_ID="aster@pve"
TOKEN_NAME="lcd"

set -e
if ! pveum user list --output-format json | grep -q "\"userid\":\"$USER_ID\""; then
    pveum user add "$USER_ID" --comment "AOOSTAR LCD panels (read-only)" >&2
fi
pveum acl modify / --users "$USER_ID" --roles PVEAuditor >&2

if pveum user token list "$USER_ID" --output-format json | grep -q "\"tokenid\":\"$TOKEN_NAME\""; then
    echo "Token $USER_ID!$TOKEN_NAME already exists on $(hostname). Remove it first to create a new secret:" >&2
    echo "  pveum user token remove $USER_ID $TOKEN_NAME" >&2
    exit 1
fi

secret=$(pveum user token add "$USER_ID" "$TOKEN_NAME" --privsep 0 --output-format json \
    | grep -o '"value":"[^"]*"' | cut -d'"' -f4)
if [[ -z "$secret" ]]; then
    echo "Failed to create the API token" >&2
    exit 1
fi
# output the secret first: it can't be retrieved again later
echo "$secret"
echo "Created token $USER_ID!$TOKEN_NAME on $(hostname)" >&2

# Certificate fingerprint, read from the certificate file served by pveproxy. A custom certificate
# (pveproxy-ssl.pem) takes precedence over the self-signed default (pve-ssl.pem).
cert=/etc/pve/local/pveproxy-ssl.pem
[[ -f "$cert" ]] || cert=/etc/pve/local/pve-ssl.pem
if fingerprint=$(openssl x509 -in "$cert" -noout -fingerprint -sha256 2>/dev/null | cut -d= -f2) \
    && [[ -n "$fingerprint" ]]; then
    echo "Certificate fingerprint for homelab.toml: tls_fingerprint = \"$fingerprint\"" >&2
else
    echo "Could not read the certificate fingerprint from $cert, see: pvenode cert info" >&2
fi
