FROM rust:1-trixie AS rust-build
RUN apt-get update \
    && apt-get install -y --no-install-recommends libopus-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p lyre-app
RUN rustup target add wasm32-unknown-unknown \
    && cargo build --release -p lyre-noise-wasm --target wasm32-unknown-unknown

FROM debian:trixie-slim AS api
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libopus0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust-build /workspace/target/release/lyre /usr/local/bin/lyre
ENV LYRE_API_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["/usr/local/bin/lyre", "serve"]

FROM node:22-bookworm-slim AS frontend-build
WORKDIR /workspace/frontend
RUN corepack enable pnpm && corepack install -g pnpm@10.25.0
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend ./
RUN mkdir -p ./public/wasm
COPY --from=rust-build /workspace/target/wasm32-unknown-unknown/release/lyre_noise_wasm.wasm ./public/wasm/lyre_noise_wasm.wasm
RUN LYRE_USE_EXISTING_NOISE_WASM=1 pnpm run build

FROM node:22-bookworm-slim AS web
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV APP_BASE_URL=http://localhost:3000
ENV APP_API_URL=http://localhost:8080
COPY --from=frontend-build /workspace/frontend/.next/standalone ./
COPY --from=frontend-build /workspace/frontend/.next/static ./.next/static
COPY --from=frontend-build /workspace/frontend/public ./public
EXPOSE 3000
CMD ["node", "server.js"]
