FROM rust:1.99.0-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p rspigot \
    && mkdir /data

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/target/release/rspigot /usr/local/bin/rspigot
COPY --from=build --chown=nonroot:nonroot /data /data
WORKDIR /data
VOLUME /data
EXPOSE 25565
STOPSIGNAL SIGINT
ENTRYPOINT ["/usr/local/bin/rspigot"]
