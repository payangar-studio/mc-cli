#!/usr/bin/env bash
# Spike (#9), throwaway: a vanilla client launched headless by HeadlessMC joins a
# vanilla dedicated server, stays N ticks, then both stop.
# Needs: bash, curl, jq, Java 25 (JAVA_HOME or java on PATH), linux x64.
# Usage: join.sh [work dir]   Env: MC_VERSION, HMC_VERSION, TICKS, PORT.
set -euo pipefail

MC_VERSION=${MC_VERSION:-26.3}
HMC_VERSION=${HMC_VERSION:-3.0.0-RC9}
TICKS=${TICKS:-200}
PORT=${PORT:-25565}
WORK=$(realpath -m "${1:-./headlessmc-spike}")
JAVA_HOME=${JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")}
JAVA="$JAVA_HOME/bin/java"

mkdir -p "$WORK"/{server,hmc,mc,game}
cd "$WORK"
t0=$(date +%s)
ts() { echo "[+$(($(date +%s) - t0))s] $*"; }

# 1. Vanilla dedicated server, offline mode.
if [ ! -f server/server.jar ]; then
  url=$(curl -fsSL https://piston-meta.mojang.com/mc/game/version_manifest_v2.json |
    jq -r --arg v "$MC_VERSION" '.versions[] | select(.id == $v) | .url')
  curl -fsSL "$(curl -fsSL "$url" | jq -r .downloads.server.url)" -o server/server.jar
fi
echo eula=true >server/eula.txt
cat >server/server.properties <<EOF
online-mode=false
enforce-secure-profile=false
server-port=$PORT
level-type=minecraft\:flat
generate-structures=false
spawn-protection=0
EOF
rm -f server/in && mkfifo server/in
(cd server && exec "$JAVA" -Xmx2G -jar server.jar nogui <in >server.log 2>&1) &
SERVER_PID=$!
exec 3>server/in # keep the fifo open: the server reads console commands from it
ts "server starting (pid $SERVER_PID)"
timeout 300 bash -c 'until grep -q "Done (" server/server.log; do sleep 1; done'
ts "server ready: $(grep -m1 'Done (' server/server.log)"

# 2. HeadlessMC, isolated in the work dir (its files, the .minecraft, the game dir).
if [ ! -x hmc/headlessmc ]; then
  curl -fsSL -o hmc/headlessmc \
    "https://github.com/headlesshq/headlessmc/releases/download/$HMC_VERSION/headlessmc-launcher-linux-x64"
  chmod +x hmc/headlessmc
fi
export HMC_FILES_LOCATION="$WORK/hmc/files"
mkdir -p "$HMC_FILES_LOCATION"
cat >"$HMC_FILES_LOCATION/config.properties" <<EOF
hmc.java.versions=$JAVA_HOME
hmc.files.mc=$WORK/mc
hmc.files.game=$WORK/game
hmc.files.game-for-each-version=false
hmc.jline.enabled=false
hmc.updater.enabled=false
EOF
printf 'onboardAccessibility:false\npauseOnLostFocus:false\nnarrator:0\n' >game/options.txt

# 3. Client: headless (LWJGL patched), offline account, straight to the server.
hmc/headlessmc launch -lwjgl --offline --no-backup \
  --game "--quickPlayMultiplayer localhost:$PORT" "$MC_VERSION" >client.log 2>&1 &
HMC_PID=$!
ts "client launching (headlessmc pid $HMC_PID)"
timeout 600 bash -c 'until grep -q "joined the game" server/server.log; do sleep 1; done'
ts "JOIN: $(grep -m1 'joined the game' server/server.log)"

# 4. Stay N ticks, then check the player is still there.
sleep $((TICKS / 20))
echo list >&3
sleep 2
if grep -E "lost connection|left the game" server/server.log; then
  ts "FAIL: player left before $TICKS ticks"
  exit 1
fi
ts "after $TICKS ticks: $(grep 'players online' server/server.log | tail -1)"

# 5. Stop: the client first (a vanilla client has no quit argument: SIGTERM runs
# its shutdown hook), then the server through its console.
GAME_PID=$(pgrep -P "$HMC_PID" || true)
ts "stopping client (game pid ${GAME_PID:-none})"
kill -TERM ${GAME_PID:-$HMC_PID}
rc=0; wait "$HMC_PID" || rc=$?
ts "headlessmc exited with $rc"
timeout 30 bash -c 'until grep -q "left the game" server/server.log; do sleep 1; done'
ts "$(grep -m1 'left the game' server/server.log)"
echo stop >&3
rc=0; wait "$SERVER_PID" || rc=$?
ts "server exited with $rc"
