#!/usr/bin/env bash
# Create a read-only Proxmox Backup Server API token for aster-homelab: user aster@pbs, role Audit.
#
# Run as root on the backup server. Prints the token secret on stdout and the certificate
# fingerprint on stderr, so it can be used remotely from the LCD host:
#
#   ssh root@pbs bash -s < /opt/aoostar/setup-pbs-token.sh > /opt/aoostar/secrets/pbs-token
#
# To create a new secret, remove the old token first:
#   proxmox-backup-manager user delete-token aster@pbs lcd
set -uo pipefail

USER_ID="aster@pbs"
TOKEN_NAME="lcd"
TOKEN_ID="$USER_ID!$TOKEN_NAME"

set -e
if ! proxmox-backup-manager user list --output-format json | grep -q "\"userid\":\"$USER_ID\""; then
    proxmox-backup-manager user create "$USER_ID" --comment "AOOSTAR LCD panels (read-only)" >&2
fi
# PBS tokens have their own permissions: the effective permission is the intersection of the
# user's and the token's ACL, so both need the read-only Audit role.
proxmox-backup-manager acl update / Audit --auth-id "$USER_ID" >&2

if proxmox-backup-manager user list-tokens "$USER_ID" --output-format json | grep -q "\"tokenid\":\"$TOKEN_ID\""; then
    echo "Token $TOKEN_ID already exists on $(hostname). Remove it first to create a new secret:" >&2
    echo "  proxmox-backup-manager user delete-token $USER_ID $TOKEN_NAME" >&2
    exit 1
fi

# generate-token has no --output-format option; its output contains `"value": "<secret>"`
secret=$(proxmox-backup-manager user generate-token "$USER_ID" "$TOKEN_NAME" \
    | grep -o '"value": *"[^"]*"' | cut -d'"' -f4)
if [[ -z "$secret" ]]; then
    echo "Failed to create the API token" >&2
    exit 1
fi
# output the secret first: it can't be retrieved again later
echo "$secret"
proxmox-backup-manager acl update / Audit --auth-id "$TOKEN_ID" >&2
echo "Created token $TOKEN_ID on $(hostname)" >&2

set +e
if fingerprint=$(openssl x509 -in /etc/proxmox-backup/proxy.pem -noout -fingerprint -sha256 2>/dev/null | cut -d= -f2) \
    && [[ -n "$fingerprint" ]]; then
    echo "Certificate fingerprint for homelab.toml: tls_fingerprint = \"$fingerprint\"" >&2
else
    echo "Could not read the certificate fingerprint, see: proxmox-backup-manager cert info" >&2
fi
