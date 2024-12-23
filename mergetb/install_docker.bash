sudo apt-get update
sudo apt-get install ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update

sudo apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

sudo usermod -a -G docker dcog


# docker run -d --name flsignal -p 8765:8765 deryacog/flsignal:latest
# docker run -d --name traefik -p 80:80 -p 443:443 -p 8080:8080 traefik:v2.2
docker run -d -p 8765:8765 deryacog/flsignal:latest -vv


docker run -d \
  -v ./simul:/config \
  deryacog/fledger:latest \
  --config /config -vv -s ws://localhost:8765
