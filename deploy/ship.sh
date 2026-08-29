#!/bin/sh
set -eu

test ! -f .env || . ./.env
: "${DEPLOY_HOST:?set DEPLOY_HOST}" "${DEPLOY_USER:?set DEPLOY_USER}"
target="$DEPLOY_USER@$DEPLOY_HOST"
ssh_opts="-o BatchMode=yes -o ConnectTimeout=10"

echo "ship: preparing $target"
ssh $ssh_opts "$target" 'sudo -n true'
ssh $ssh_opts "$target" 'rm -rf /tmp/ppu-web-dist /tmp/ppu-server.new /tmp/Caddyfile.new'
echo "ship: uploading server, web bundle, and Caddyfile"
scp $ssh_opts target/release/ppu-server "$target:/tmp/ppu-server.new"
scp $ssh_opts -r web/dist "$target:/tmp/ppu-web-dist"
scp $ssh_opts deploy/Caddyfile "$target:/tmp/Caddyfile.new"

echo "ship: installing and restarting production"
ssh $ssh_opts "$target" 'sudo -n sh -s' <<'REMOTE'
set -eu
caddy validate --config /tmp/Caddyfile.new --adapter caddyfile
test ! -f /opt/ppu/ppu-server || cp -a /opt/ppu/ppu-server /opt/ppu/ppu-server.prev
install -o ppu -g ppu -m 0755 /tmp/ppu-server.new /opt/ppu/ppu-server
rm -rf /opt/ppu/web/dist
install -d -o ppu -g ppu -m 0755 /opt/ppu/web
mv /tmp/ppu-web-dist /opt/ppu/web/dist
chown -R ppu:ppu /opt/ppu/web/dist
cp -a /etc/caddy/Caddyfile /etc/caddy/Caddyfile.prev
install -o root -g root -m 0644 /tmp/Caddyfile.new /etc/caddy/Caddyfile
rm -f /tmp/ppu-server.new
systemctl restart ppu-server
systemctl reload caddy
curl --fail --silent --show-error --retry 5 --retry-delay 1 http://127.0.0.1:8080/api/health
REMOTE
echo "ship: production is healthy"
