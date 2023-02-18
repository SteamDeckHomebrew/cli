FROM debian:unstable-slim

ARG VERSION

RUN apt-get update && apt-get install -y curl

WORKDIR /bin
RUN curl -L -o decky "https://github.com/SteamDeckHomebrew/cli/releases/download/$VERSION/decky"
RUN chmod +x decky
RUN ldd decky

ENTRYPOINT ["/bin/decky"]
