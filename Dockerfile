FROM debian:bullseye-slim

ARG VERSION
ARG TARGETARCH

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy both possible Linux binaries from artifacts
COPY artifacts/fpkgi-server-v${VERSION}-x86_64-unknown-linux-gnu/fpkgi-server-v${VERSION}-x86_64-unknown-linux-gnu \
     artifacts/fpkgi-server-v${VERSION}-aarch64-unknown-linux-gnu/fpkgi-server-v${VERSION}-aarch64-unknown-linux-gnu \
     /usr/local/bin/

# Select the correct binary based on TARGETARCH and rename it to fpkgi-server
RUN if [ "$TARGETARCH" = "amd64" ]; then \
      mv /usr/local/bin/fpkgi-server-v${VERSION}-x86_64-unknown-linux-gnu /usr/local/bin/fpkgi-server; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
      mv /usr/local/bin/fpkgi-server-v${VERSION}-aarch64-unknown-linux-gnu /usr/local/bin/fpkgi-server; \
    fi && \
    chmod +x /usr/local/bin/fpkgi-server

EXPOSE 8000
ENTRYPOINT ["fpkgi-server"]
CMD ["host", "--port", "8000", "--packages", "/data/packages:pkgs", "--url", "http://localhost:8000", "--out", "/data/jsons:jsons", "--icons", "/data/icons:icons"]
