#!/usr/bin/env bash
# box-setup.sh — idempotent one-time provisioning for the ppu.toys box.
# Target: Linode Nanode (x86_64 / amd64), Ubuntu 24.04 LTS.
# Usage (as root, from the repo):  sudo bash deploy/box-setup.sh
# Safe to re-run: every step guards against redoing work. Does NOT start
# ppu-server/litestream (they need the deployed binary + filled secrets first).
set -euo pipefail

# --- Config — keep in sync with the other deploy artifacts + env template ---
PPU_USER="ppu"
PPU_GROUP="ppu"
APP_DIR="/opt/ppu"
DATA_DIR="/var/lib/ppu"
BLOB_DIR="${DATA_DIR}/blobs"
ETC_DIR="/etc/ppu"
LITESTREAM_VERSION="0.3.13"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

log() { printf '\n>>> %s\n' "$*"; }

require_root() {
	if [ "$(id -u)" -ne 0 ]; then
		echo "This script must run as root (use sudo)." >&2
		exit 1
	fi
}

create_user() {
	if id -u "${PPU_USER}" >/dev/null 2>&1; then
		log "User ${PPU_USER} already exists"
	else
		log "Creating system user ${PPU_USER}"
		useradd --system --home-dir "${APP_DIR}" --shell /usr/sbin/nologin "${PPU_USER}"
	fi
}

create_dirs() {
	log "Creating directories"
	install -d -o "${PPU_USER}" -g "${PPU_GROUP}" -m 0755 "${APP_DIR}"
	install -d -o "${PPU_USER}" -g "${PPU_GROUP}" -m 0755 "${APP_DIR}/web"
	install -d -o "${PPU_USER}" -g "${PPU_GROUP}" -m 0750 "${DATA_DIR}"
	install -d -o "${PPU_USER}" -g "${PPU_GROUP}" -m 0750 "${BLOB_DIR}"
	install -d -o root -g "${PPU_GROUP}" -m 0750 "${ETC_DIR}"
}

apt_base() {
	log "Installing base packages"
	export DEBIAN_FRONTEND=noninteractive
	apt-get update -y
	apt-get install -y ca-certificates curl gnupg debian-keyring \
		debian-archive-keyring apt-transport-https ufw unattended-upgrades
}

setup_firewall() {
	log "Configuring UFW (SSH + 80 + 443)"
	ufw allow OpenSSH
	ufw allow 80/tcp
	ufw allow 443/tcp
	ufw --force enable
}

setup_unattended_upgrades() {
	log "Enabling unattended-upgrades"
	dpkg-reconfigure -f noninteractive unattended-upgrades
	systemctl enable --now unattended-upgrades
}

install_caddy() {
	if command -v caddy >/dev/null 2>&1; then
		log "Caddy already installed"
		return
	fi
	log "Installing Caddy from official apt repo"
	curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
		| gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
	curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
		| tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null
	apt-get update -y
	apt-get install -y caddy
}

install_litestream() {
	if command -v litestream >/dev/null 2>&1; then
		log "Litestream already installed"
		return
	fi
	log "Installing Litestream ${LITESTREAM_VERSION} (amd64)"
	local deb="/tmp/litestream-${LITESTREAM_VERSION}-amd64.deb"
	curl -fsSL -o "${deb}" \
		"https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-v${LITESTREAM_VERSION}-linux-amd64.deb"
	dpkg -i "${deb}"
	rm -f "${deb}"
	# The .deb ships its own unit + /etc/litestream.yml; we override both, so
	# disable the packaged unit to avoid a second replicator racing ours.
	systemctl disable --now litestream.service 2>/dev/null || true
}

install_configs() {
	log "Installing config + systemd units from ${SCRIPT_DIR}"
	install -o root -g root -m 0644 "${SCRIPT_DIR}/litestream.yml" /etc/litestream.yml
	install -d -o root -g root -m 0755 /etc/caddy
	install -o root -g root -m 0644 "${SCRIPT_DIR}/Caddyfile" /etc/caddy/Caddyfile
	install -o root -g root -m 0644 "${SCRIPT_DIR}/ppu-server.service" /etc/systemd/system/ppu-server.service
	install -o root -g root -m 0644 "${SCRIPT_DIR}/litestream.service" /etc/systemd/system/litestream.service
}

install_env_templates() {
	local app_env="${ETC_DIR}/ppu-server.env"
	local ls_env="${ETC_DIR}/litestream.env"
	if [ -f "${app_env}" ]; then
		log "${app_env} already present — leaving as is"
	else
		log "Dropping env template ${app_env} (fill required values before deploying)"
		install -o root -g "${PPU_GROUP}" -m 0640 \
			"${SCRIPT_DIR}/ppu-server.env.example" "${app_env}"
	fi
	if [ -f "${ls_env}" ]; then
		log "${ls_env} already present — leaving as is"
	else
		log "Writing R2 credentials template ${ls_env}"
		cat >"${ls_env}" <<'EOF'
# Cloudflare R2 config for Litestream (requires an R2 bucket and API token).
# ACCOUNT_ID is your Cloudflare account id; the two keys are the R2 API token.
# All three are expanded into /etc/litestream.yml at runtime — that file stays
# static, so this env file is the ONLY place these values live.
LITESTREAM_R2_ACCOUNT_ID=
LITESTREAM_ACCESS_KEY_ID=
LITESTREAM_SECRET_ACCESS_KEY=
EOF
		chown root:"${PPU_GROUP}" "${ls_env}"
		chmod 0640 "${ls_env}"
	fi
}

enable_services() {
	log "Reloading systemd + enabling services"
	systemctl daemon-reload
	systemctl enable caddy
	# ppu-server + litestream are ENABLED (start on boot) but NOT started now:
	# they need the binary deployed and secrets filled before the first deploy.
	systemctl enable ppu-server.service
	systemctl enable litestream.service
}

main() {
	require_root
	create_user
	create_dirs
	apt_base
	setup_firewall
	setup_unattended_upgrades
	install_caddy
	install_litestream
	install_configs
	install_env_templates
	enable_services
	log "Done. Next: fill /etc/ppu/ppu-server.env and /etc/ppu/litestream.env, then trigger the deploy workflow."
}

main "$@"
