FROM rust:1.50.0-slim-buster

# Install build essentials
RUN apt-get update && apt-get install -y make pkg-config libssl-dev ca-certificates musl-tools wget

# Set up rust MUSL target
# (https://doc.rust-lang.org/edition-guide/rust-2018/platform-and-target-support/musl-support-for-fully-static-binaries.html)
RUN rustup target add x86_64-unknown-linux-musl

# Copy a version of the code that should be overwritten in later layers
COPY . /app
WORKDIR /app

# Warm build cache by running regular and testing builds
RUN make build build-test build-release
