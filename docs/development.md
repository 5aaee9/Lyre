# Development and Testing

## Rust

Format and lint Rust changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Run workspace tests:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

## Frontend

Run frontend checks:

```bash
cd frontend
npm test -- --run
npm run typecheck
npm run lint
```

Frontend browser settings and state must use the Zustand store in `frontend/src/lib/settings-store.ts`. Do not manually read or write `localStorage` for user settings; persistence adapters and tests are the only direct storage plumbing exceptions.

## Documentation

Keep `README.md` concise. Put operational details, API lists, architecture notes, and development commands under `docs/`.

Update `docs/roadmap.md` when completed work changes project status or follow-up scope.
