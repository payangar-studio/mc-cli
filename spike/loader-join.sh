#!/usr/bin/env bash
# Throwaway experiment (#10): a Fabric or NeoForge client launched headless by
# HeadlessMC joins a dedicated server of the same loader. Removed before merge.
set -euo pipefail
LOADER=$1 # fabric | neoforge
MC=26.3
FABRIC_LOADER=0.20.1
FABRIC_API=0.166.2+26.3
NEOFORGE=26.3.0.113
HMC_VERSION=3.0.0-RC9
PORT=25565
WORK=$(realpath -m "${2:-./loader-join}")
JAVA_HOME=${JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")}
JAVA="$JAVA_HOME/bin/java"
mkdir -p "$WORK"/{server/mods,hmc,mc,game/mods}
cd "$WORK"
t0=$(date +%s)
ts() { echo "[+$(($(date +%s) - t0))s] $*"; }

if [ "$LOADER" = fabric ]; then
  inst=$(curl -fsSL https://meta.fabricmc.net/v2/versions/installer | jq -r '[.[] | select(.stable)][0].version')
  curl -fsSL -o server/fabric-server-launch.jar "https://meta.fabricmc.net/v2/versions/loader/$MC/$FABRIC_LOADER/$inst/server/jar"
  curl -fsSL -o fabric-api.jar "https://maven.fabricmc.net/net/fabricmc/fabric-api/fabric-api/$FABRIC_API/fabric-api-$FABRIC_API.jar"
  cp fabric-api.jar server/mods/; cp fabric-api.jar game/mods/
  START=(-jar fabric-server-launch.jar nogui)
  CLIENT=(fabric "$MC" "$FABRIC_LOADER")
else
  curl -fsSL -o installer.jar "https://maven.neoforged.net/releases/net/neoforged/neoforge/$NEOFORGE/neoforge-$NEOFORGE-installer.jar"
  "$JAVA" -jar installer.jar --install-server server >install.log 2>&1
  START=("@libraries/net/neoforged/neoforge/$NEOFORGE/unix_args.txt" nogui)
  # HeadlessMC cannot resolve NeoForge 26.x builds: the official installer
  # installs the client into the Minecraft directory, launched by its id.
  echo '{"profiles":{}}' >mc/launcher_profiles.json
  "$JAVA" -jar installer.jar --install-client mc >install-client.log 2>&1 || { tail -30 install-client.log; exit 1; }
  ls mc/versions
  CLIENT=("neoforge-$NEOFORGE")
fi
ts "server installed"
echo eula=true >server/eula.txt
cat >server/server.properties <<P
online-mode=false
white-list=false
enforce-secure-profile=false
server-port=$PORT
level-type=minecraft\:flat
generate-structures=false
P
rm -f server/in && mkfifo server/in
(cd server && exec "$JAVA" -Xmx2G "${START[@]}" <in >server.log 2>&1) &
SERVER_PID=$!
exec 3>server/in
timeout 300 bash -c 'until grep -q "Done (" server/server.log; do sleep 1; done'
ts "server ready: $(grep -m1 'Done (' server/server.log)"

curl -fsSL -o hmc/headlessmc.jar "https://github.com/headlesshq/headlessmc/releases/download/$HMC_VERSION/headlessmc.jar"
export HMC_FILES_LOCATION="$WORK/hmc/files"
mkdir -p "$HMC_FILES_LOCATION"
cat >"$HMC_FILES_LOCATION/config.properties" <<P
hmc.java.versions=$JAVA_HOME
hmc.files.mc=$WORK/mc
hmc.files.game=$WORK/game
hmc.files.game-for-each-version=false
hmc.jline.enabled=false
hmc.updater.enabled=false
P
printf 'onboardAccessibility:false\npauseOnLostFocus:false\nnarrator:0\n' >game/options.txt
setsid "$JAVA" -jar hmc/headlessmc.jar launch -lwjgl --offline --no-backup \
  --game "--quickPlayMultiplayer 127.0.0.1:$PORT" "${CLIENT[@]}" >client.log 2>&1 &
HMC_PID=$!
ts "client launching"
timeout 600 bash -c 'until grep -q -E "joined the game|lost connection" server/server.log; do sleep 1; done' || true
grep -E "joined the game|lost connection" server/server.log || { ts FAIL; tail -50 client.log; exit 1; }
sleep 10
echo list >&3
sleep 2
grep -E "players online|lost connection|left the game" server/server.log
kill -TERM -- "-$HMC_PID" || true
wait "$HMC_PID" || ts "headlessmc exited with $?"
echo stop >&3
wait "$SERVER_PID" || true
ts done
grep -E "joined|lost connection|players online" server/server.log || true
tail -5 client.log
