FROM rust:1.97-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p bess-emulator

FROM debian:bookworm-slim
COPY --from=build /src/target/release/bess-emulator /usr/local/bin/bess-emulator
EXPOSE 1502 8080
ENTRYPOINT ["bess-emulator"]
# Inside a container the bind must be 0.0.0.0; the compose file decides
# what, if anything, is exposed to the host.
CMD ["--modbus", "0.0.0.0:1502", "--http", "0.0.0.0:8080", "--mqtt-host", "mosquitto"]
