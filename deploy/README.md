# Aequitas Core — Deployment

This directory contains the hardened systemd unit and the host-level steps required to make the posture permanent.

## Files

- `Aequitas_Core.service` — systemd unit with namespace, filesystem, and memory hardening.

## Install the service

```bash
sudo cp deploy/Aequitas_Core.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now Aequitas_Core
```

## Permanent read-only mount (/etc/fstab)

Add the following line so the application directory cannot be remounted writable:

```
/opt/Aequitas_Core    /opt/Aequitas_Core    none    bind,ro    0    0
```

Apply and verify:

```bash
sudo mount -o remount,ro /opt/Aequitas_Core
sudo findmnt --verify
```

## Verification

```bash
# Service hardening flags
systemctl show Aequitas_Core --property=ProtectSystem,MemoryDenyWriteExecute,RestrictNamespaces

# Confirm directory is read-only
touch /opt/Aequitas_Core/test_write_lock
# Expected: "touch: cannot touch ... Read-only file system"
```

## What is enforced

- `ProtectSystem=strict` + `ReadOnlyPaths` on `/opt/Aequitas_Core`
- `RestrictNamespaces=true`
- `MemoryDenyWriteExecute=true`
- `NoNewPrivileges=true`
- Writable paths limited to state and log directories
- Loader environment sanitization inside the binary (`LD_*` stripped at boot)
- In-process hard fail if the target directory is writable

The combination of the unit, the fstab bind-ro entry, and the in-process gates is the permanent posture.
