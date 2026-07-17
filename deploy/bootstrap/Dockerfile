# Nœud de rendez-vous Accord (accord-bootstrap) : DHT + relais, sans compte.
# Build sans features matérielles (pas d'Opus/cpal) : aucune dépendance native.
FROM rust:1.85-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p accord-node --bin accord-bootstrap

FROM debian:bookworm-slim
RUN useradd --system --home /var/lib/accord-bootstrap --create-home accord
COPY --from=build /src/target/release/accord-bootstrap /usr/local/bin/accord-bootstrap
USER accord
ENV ACCORD_BOOTSTRAP_STATE=/var/lib/accord-bootstrap
# Port UDP P2P (+ écouteur TCP de repli sur le même port).
EXPOSE 48016/udp 48016/tcp
ENTRYPOINT ["/usr/local/bin/accord-bootstrap"]
