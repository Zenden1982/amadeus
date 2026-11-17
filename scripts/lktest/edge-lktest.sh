#!/bin/bash
# Временный нагрузочный тест LiveKit на edge VPS. Запуск: sudo -n bash edge-lktest.sh start|stop|status
# Всё временное: порты и сервер сами снимаются через 2 часа (systemd-run), даже если про тест забыли.
set -euo pipefail
DIR=/opt/livekit-test
TS=$(date +%Y%m%d-%H%M%S)
PORTS=("7880/tcp" "7881/tcp" "7882/udp")

stop() {
  for p in "${PORTS[@]}"; do ufw --force delete allow "$p" >/dev/null 2>&1 || true; done
  systemctl stop lk-test lk-mon 2>/dev/null || true
  echo "stopped; ufw:"; ufw status | grep -E '788[0-2]' || echo "  (правил 7880-7882 нет)"
}

case "${1:-}" in
start)
  [ -f /tmp/livekit-test.tar.gz ] && [ -f /tmp/lktest.yaml ] || { echo "нет /tmp/livekit-test.tar.gz или /tmp/lktest.yaml"; exit 1; }
  install -d -m 700 $DIR
  tar -xzf /tmp/livekit-test.tar.gz -C $DIR livekit-server
  install -m 600 /tmp/lktest.yaml $DIR/livekit.yaml; shred -u /tmp/lktest.yaml
  cp -a /etc/ufw /etc/ufw.bak-$TS-lktest
  for p in "${PORTS[@]}"; do ufw allow "$p" comment 'lk-test (временно)'; done
  # автооткат через 2 часа
  install -m 700 "$0" $DIR/edge-lktest.sh
  systemd-run --unit=lk-test-rollback --on-active=2h /bin/bash $DIR/edge-lktest.sh stop
  systemd-run --unit=lk-test -p RuntimeMaxSec=2h $DIR/livekit-server --config $DIR/livekit.yaml
  # монитор: CPU (us sy id st) и трафик eth каждую секунду
  systemd-run --unit=lk-mon -p RuntimeMaxSec=2h /bin/bash -c 'IF=$(ip route show default | awk "{print \$5; exit}"); while true; do echo "$(date +%T) $(vmstat 1 2 | tail -1 | awk "{print \"us=\"\$13,\"sy=\"\$14,\"id=\"\$15,\"st=\"\$17}") $(awk -v i=$IF: "\$1==i{print \"rx=\"\$2,\"tx=\"\$10}" /proc/net/dev)"; done > /tmp/lk-mon.log'
  sleep 2; systemctl is-active lk-test lk-mon; ufw status | grep -E '788[0-2]'
  ;;
stop) stop ;;
status) systemctl is-active lk-test lk-mon || true; tail -3 /tmp/lk-mon.log 2>/dev/null; ufw status | grep -E '788[0-2]' || true ;;
*) echo "usage: $0 start|stop|status"; exit 1 ;;
esac
