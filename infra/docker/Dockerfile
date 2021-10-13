FROM registry.gitlab.com/visheshc14/Neo-Rust/builder:0.2.1 as builder

# Overwrite code in the base builder image
COPY . /app
WORKDIR /app

# Build again incase anything has changed
RUN make build-release

FROM scratch

# Copy the built static binary from the intermediate image
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/Neo /Neo

CMD ["/Neo"]
