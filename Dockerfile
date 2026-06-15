FROM rust:1-bookworm AS rust-build
RUN apt-get update \
    && apt-get install -y --no-install-recommends libopus-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p lyre-app

FROM debian:bookworm-slim AS api
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libopus0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust-build /workspace/target/release/lyre /usr/local/bin/lyre
ENV LYRE_API_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["/usr/local/bin/lyre", "serve"]

FROM node:22-bookworm-slim AS frontend-build
WORKDIR /workspace/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN if [ -f package-lock.json ]; then npm ci; else npm install; fi
COPY frontend ./
RUN npm run build

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
