FROM rust:1.76-alpine3.18 as builder
RUN apk add --no-cache musl-dev

WORKDIR /usr/src/rust-app
RUN USER=root cargo new --bin --vcs none consumer-control-api
WORKDIR /usr/src/rust-app/consumer-control-api

# Copy the Cargo manifest files
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Cache the dependencies
RUN cargo build --release
RUN rm src/*.rs

# Copy your actual source code and the docs directory
COPY ./src ./src
# Copy the docs directory to the image
COPY ./docs ./docs  

# Compile the application
RUN touch src/main.rs && cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && \
    apt-get install -y libpq5 ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/rust-app/consumer-control-api/target/release/consumer-control-api /usr/local/bin/consumer-control-api
COPY --from=builder /usr/src/rust-app/consumer-control-api/docs /usr/local/bin/docs

# Set an environment variable to tell your app where the docs are located
ENV DOCS_DIR /usr/local/bin/docs

CMD ["consumer-control-api", "-p", "80"]
