FROM rustlang/rust:nightly-alpine AS builder

RUN apk add --no-cache openssl-dev pkgconfig build-base

RUN cargo new --bin YouTubeTLDR
WORKDIR /YouTubeTLDR

COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

COPY ./src ./src
COPY ./static ./static

RUN cargo build --release --no-default-features --features rustls-tls

FROM alpine:latest

RUN apk add --no-cache openssl

COPY --from=builder /YouTubeTLDR/target/release/YouTubeTLDR /usr/local/bin/YouTubeTLDR

COPY ./static /app/static

WORKDIR /app

EXPOSE 8000

CMD ["/usr/local/bin/YouTubeTLDR"]
