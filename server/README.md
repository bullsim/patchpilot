# PatchPilot server (Hetzner)

Hosts two things on `patchpilot.bullers.com`:

1. **Update host** — `/updates/latest.json` + installers, uploaded by CI. The desktop
   app's auto-updater pulls from here.
2. **Fleet dashboard + report API** — each machine POSTs a status report after every run;
   the dashboard shows them all.

The desktop app is still the engine — this server never touches your machines. It only
serves files and collects status pings.

## One-time setup on the Hetzner box

```bash
# 1. Node (if not present)
sudo apt-get install -y nodejs

# 2. App
sudo useradd -r -m -d /opt/patchpilot patchpilot || true
sudo mkdir -p /opt/patchpilot/server /var/www/patchpilot/updates /var/lib/patchpilot
sudo cp server.js /opt/patchpilot/server/
sudo chown -R patchpilot:patchpilot /opt/patchpilot /var/lib/patchpilot

# 3. systemd service
sudo cp patchpilot-dashboard.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now patchpilot-dashboard

# 4. Caddy — add Caddyfile.snippet to your shared Caddyfile, then:
sudo systemctl reload caddy
```

Point DNS `patchpilot.bullers.com` → the Hetzner IP. Caddy gets HTTPS automatically.

## How releases get here (CI)

The GitHub release workflow builds + signs installers for every platform, then a deploy job
`rsync`s the installers and `latest.json` into `/var/www/patchpilot/updates/`. It needs
these repo secrets:

- `HETZNER_SSH_KEY`  — private SSH key authorized on the box
- `HETZNER_HOST`     — e.g. `patchpilot.bullers.com` or the IP
- `HETZNER_USER`     — SSH user (must be able to write `/var/www/patchpilot/updates`)

## Verify

```bash
curl https://patchpilot.bullers.com/updates/latest.json     # update manifest
curl https://patchpilot.bullers.com/api/machines            # reported machines
# open https://patchpilot.bullers.com/ in a browser          # dashboard
```
