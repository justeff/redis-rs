FROM docker.io/library/rust:latest
RUN apt-get update -qq && apt-get install -y -qq redis-server >/dev/null && redis-server --version
RUN cargo install cargo-nextest --locked -q && cargo nextest --version
