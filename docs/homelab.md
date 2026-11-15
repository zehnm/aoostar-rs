# Homelab Panels: Proxmox VE and TrueNAS

Rotating LCD panels for a WTR MAX running Proxmox VE, with data from the host itself, from other
Proxmox hosts, and from a TrueNAS VM. Everything runs on the Proxmox host with the LCD. No
Prometheus or other monitoring stack is required.

| Panel        | Content                                                                    | Shown when                    |
|--------------|----------------------------------------------------------------------------|-------------------------------|
| Summary      | One big number: how many things are wrong, the top 3 problems, 6 key tiles | always                        |
| Host         | CPU, memory, CPU temperature, root disk, load, uptime, network, guests     | always                        |
| Nodes        | Status, CPU, memory, guests and uptime per Proxmox host                    | more than one host or node    |
| Guests       | VMs and containers of all hosts: status, CPU, memory (14 per page)         | Proxmox configured            |
| Guests 2     | Guests 15 – 28                                                             | more than 14 guests           |
| TrueNAS      | Pools with health and usage, disk temperatures, alerts, apps, uptime       | TrueNAS configured            |
| Data protection | Pool scrubs, last Proxmox backup job per host, TrueNAS replication, cloud sync, rsync and snapshot tasks | always |
| Backup server | Proxmox Backup Server datastores (usage, availability, growth) and verify, sync, prune and garbage collection jobs | PBS configured |

The Guests panel also shows the age of each guest's newest backup, and the Summary panel how many
guests have a fresh backup.

Design rules:
- Healthy is quiet: running guests and online pools are grey, not green. Only problems use color.
- The summary inverts the usual "69/69 up" counters: healthy is a big green **0**, anything else is
  a red or amber number. You read a shape, not digits.
- Values that are no longer refreshed are shown as `--` instead of the last known value.

## Components

```
aoostar-sysinfo.service   aster-sysinfo   → /run/aoostar-sysinfo/sysinfo.txt   host sensors
aoostar-homelab.service   aster-homelab   → /run/aoostar-homelab/homelab.txt   Proxmox VE, PBS and TrueNAS APIs
aoostar-panels.service    asterctl        ← both sensor files                  renders panels on the LCD
```

- `aster-homelab` polls the Proxmox API of each configured host (`/cluster/resources`) and the
  TrueNAS JSON-RPC WebSocket API (`wss://<truenas>/api/current`) every 10 seconds, and the
  Proxmox Backup Server API once a minute.
- API credentials are read-only: Proxmox `PVEAuditor` role, PBS `Audit` role, TrueNAS read-only
  administrator.
- Self-signed certificates are pinned by their SHA-256 fingerprint instead of disabling
  certificate verification.
- An unreachable or misconfigured host is shown as unreachable; the other sources keep working.

## Installation on the Proxmox host

All commands as root. The bundle installs into `/opt/aoostar` and does not touch an existing
`aoostar-lcd.service` or its files, so you can switch back at any time.

### 1. Install files

```shell
tar xzf aoostar-homelab-x86_64.tar.gz
cd aoostar-homelab
./install.sh
```

This creates the system user `aoostar`, installs binaries, fonts, panels and systemd units, and
creates `/opt/aoostar/homelab.toml` from the example. Nothing is started yet.

### 2. Proxmox API tokens

Create a read-only token `aster@pve!lcd` on the LCD host and on every other standalone host
(once per cluster). The helper script prints the secret on stdout and the certificate fingerprint
on stderr:

```shell
cd /opt/aoostar
bash setup-pve-token.sh > secrets/pve-a-token
ssh root@pve-b bash -s < setup-pve-token.sh > secrets/pve-b-token
ssh root@pve-c bash -s < setup-pve-token.sh > secrets/pve-c-token
chgrp aoostar secrets/*-token && chmod 0640 secrets/*-token
```

Copy each printed fingerprint into the `tls_fingerprint` setting of the matching `[[proxmox]]`
entry in `homelab.toml`, and check the `url` of each host (host name or IP address).

If `LISTEN_IP` is set in `/etc/default/pveproxy`, the Proxmox API only listens on that address and
not on `127.0.0.1`. Use that address in the `url` of the local host as well. Check with:

```shell
ss -tlnp | grep 8006
```

### 3. TrueNAS API key

In the TrueNAS web interface:

1. Create a user, for example `aster`: no password login, no SMB, no home directory. Give it
   read-only access: the read-only administrator role (member of the built-in
   `builtin_readonly_administrators` group). Menu names differ slightly between releases.
2. Create an API key for this user and save it on the Proxmox host:

```shell
cd /opt/aoostar
read -rsp "TrueNAS API key: " key && printf '%s\n' "$key" > secrets/truenas-api-key && unset key
chgrp aoostar secrets/truenas-api-key && chmod 0640 secrets/truenas-api-key
```

Get the certificate fingerprint and compare it with the certificate shown in the TrueNAS web
interface (System → Certificates) before adding it as `tls_fingerprint` to the `[truenas]` section:

```shell
/opt/aoostar/bin/aster-homelab --print-fingerprint 192.0.2.10
```

> TrueNAS revokes API keys that are used over plain HTTP. `aster-homelab` always uses HTTPS.

### 3b. Proxmox Backup Server token (optional)

Create a read-only token `aster@pbs!lcd` with the `Audit` role on the backup server, from the
Proxmox host. Replace `pbs` with the host name or IP address of your backup server:

```shell
cd /opt/aoostar
ssh root@pbs bash -s < setup-pbs-token.sh > secrets/pbs-token
chgrp aoostar secrets/pbs-token && chmod 0640 secrets/pbs-token
```

Add a `[pbs]` section to `homelab.toml` (see `homelab.toml.new` or `homelab.example.toml`) with
the `url` of the backup server (port 8007) and the printed `tls_fingerprint`.

If a datastore is on a removable disk, `aster-homelab` reports it as unavailable when the disk is
missing. PBS removable datastores can only be chosen when a datastore is created.

### 4. Test

```shell
runuser -u aoostar -- /opt/aoostar/bin/aster-homelab --config /opt/aoostar/homelab.toml --once --console
/opt/aoostar/bin/aster-sysinfo --console | grep -E "temperature_|disk_.*usage_percent|network_.*speed"
```

Adjust `/opt/aoostar/panels/sensor-mapping.cfg` to the names of the host sensors: root disk
device, CPU temperature and the network interface (for example `vmbr0`).

### 5. Switch to the homelab panels

```shell
cd ~/aoostar-homelab
./install.sh --enable      # disables aoostar-lcd.service, starts the three new services
journalctl -fu aoostar-panels -u aoostar-homelab
```

Switch back to the previous setup:

```shell
./install.sh --rollback
```

## Burn-in protection

Options are set in `/opt/aoostar/panels.env`, followed by `systemctl restart aoostar-panels`:

| Option                     | Default in panels.env | Description                                          |
|----------------------------|-----------------------|------------------------------------------------------|
| `--pixel-shift N`          | 5                     | Move the whole panel by one pixel every N minutes    |
| `--pixel-shift-max PX`     | 2                     | Maximum offset in each direction                     |
| `--wipe-interval N`        | 60                    | Show the inverted panel every N minutes...           |
| `--wipe-duration S`        | 3                     | ...for S seconds                                     |
| `--off-hours HH:MM-HH:MM`  | 23:00-07:00           | Switch the display off during this daily window      |

The panel rotation itself also changes most of the screen content every 10 seconds.

## Configuration reference

- `/opt/aoostar/homelab.toml`: data sources, see the comments in the file.
  - `exclude`: guest ids to hide per host.
  - `expect_running`: guest ids that must run; a stopped guest counts as a problem.
  - `hdd_temp_limits`, `ssd_temp_limits`: disk temperature warning and critical thresholds.
  - `pool_usage_limits`: pool usage warning and critical thresholds in percent.
  - `scrub_max_age_days`: a pool not scrubbed for this many days is a warning (default 40).
  - `task_max_age_days`: a TrueNAS data protection task without a successful run for this many
    days is a warning (default 8). Failed tasks are always a problem. `exclude_tasks` hides tasks
    by name.
  - `backup_max_age_hours` (per Proxmox host): a guest whose last successful backup is older is a
    warning (default 26). A guest whose most recent backup attempt failed is always a problem.
    Guests without any backup show `NONE`, but don't count as a problem; hide them with
    `backup_ignore`. The backup status is read every 5 minutes from the logs of the last 50
    backup (`vzdump`) tasks of each host: listing backup files on storages would require backup
    permissions, while task logs are readable with the read-only `PVEAuditor` role.
  - Healthy periodic snapshot tasks are combined into one row on the Data protection panel.
    Failed or stale snapshot tasks are listed individually.
  - `[pbs]`: `usage_limits` are the datastore usage warning and critical thresholds (default 80 / 90
    percent). `job_max_age_days`: a scheduled verify, sync, prune or garbage collection job without
    a run for this many days is a warning (default 8). Failed jobs and unavailable datastores are
    always a problem.
- `/opt/aoostar/panels/homelab.json`: panel layout, generated by `panels/generate.py`. Edit the
  script and run `python3 generate.py` in the panels directory to change the layout.
- Panel rotation interval: `setup.switchTime` in `homelab.json` (default 10 seconds).

### asterctl extensions used by the panels

- Multiple `--sensor-path` options; paths that don't exist yet are picked up when they appear.
- `--stale-after S`: values from sensor files not modified for S seconds are rendered as `--`.
  Keys that disappear from a sensor file are removed.
- Static captions: sensors with a label starting with `TEXT_` render their `value` field.
- Optional panel field `requires`: the panel is skipped if this sensor has no value.
- State colors: `aster-homelab` writes state values with one of the suffixes `_ok`, `_warn` or
  `_bad`. The panels place three differently colored text elements at the same position; only the
  current one has a value.

### aster-homelab sensors

Run `aster-homelab --once --console` for the full list. Main keys:

| Key                                         | Description                                       |
|---------------------------------------------|---------------------------------------------------|
| `homelab_problems`, `homelab_problem[i]`    | Problem count and the top 3 descriptions          |
| `pve_status`, `pve_node[i]_*`               | Host reachability, per-host CPU, memory, uptime   |
| `guest[i]_{id,name,tag,status,cpu_percent,mem_percent,uptime}` | Guests sorted by host and id |
| `guests_summary`, `vms_running`, `cts_running` | Running / total guests                          |
| `pve_storage[i]_*`                          | Proxmox storage usage                             |
| `truenas_{status,version,uptime}`           | TrueNAS system                                    |
| `pool[i]_{name,status,used_percent,used_level}` | Pools                                         |
| `disk[i]_{name,temp,temp_level}`, `disks_max_temp` | Disk temperatures (standby disks are not woken) |
| `truenas_alerts`, `truenas_alert[i]`        | Active warning and critical alerts                |
| `app[i]_*`, `apps_summary`                  | TrueNAS apps                                      |
| `pool[i]_scrub`, `pool[i]_scrub_{age,age_days,errors,percent,eta}` | Scrub or resilver status |
| `task[i]_{kind,name,state,age}`, `tasks_summary` | TrueNAS replication, cloud sync, rsync and snapshot tasks |
| `guest[i]_backup`, `guest[i]_backup_age_hours` | Age of the newest backup per guest            |
| `backups_summary`, `backups_oldest_{name,age}` | Guests with a fresh backup, oldest backup      |
| `pve_backup_job[i]_{node,state,age,error}`  | Last backup job (vzdump) per Proxmox node         |
| `pbs_status`, `pbs_version`                 | Backup server reachability and version            |
| `datastore[i]_{name,status,used_percent,used_level,usage,full_in,error}`, `datastores_summary` | PBS datastores |
| `pbs_job[i]_{kind,name,state,age,error}`, `pbs_jobs_summary` | PBS verify, sync, prune and garbage collection jobs |
