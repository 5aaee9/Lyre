# Noise Settings Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users adjust noise-cancellation provider, intensity, and voice activity threshold from the frontend.

**Architecture:** Keep the existing `NoiseCancellationConfig` shape and local storage key. Add numeric controls to settings, preserve stored numeric values in the join flow, and test storage/UI/API behavior.

**Tech Stack:** Next.js, React, TypeScript, Vitest.

---

## Task 1: Storage and Join Flow

**Files:**
- Modify: `frontend/src/app/page.tsx`
- Modify: `frontend/src/lib/storage.test.ts`
- Modify: `frontend/src/lib/api.test.ts`

- [x] **Step 1: Preserve stored numeric parameters on join**

Read the full noise config into state. The join page provider selector updates only `noise.provider`. On join, send `{ provider, intensity, voice_activity_threshold }` from state.

- [x] **Step 2: Extend storage test**

Assert `readNoiseConfig()` round-trips provider, intensity, and voice activity threshold.

- [x] **Step 3: Extend API join test**

Assert `joinRoom` serializes the full noise config when provided, and add a join page test that proves provider changes preserve stored numeric parameters in the actual join request.

## Task 2: Settings Controls

**Files:**
- Modify: `frontend/src/app/settings/page.tsx`
- Add: `frontend/src/app/settings/settings-page.test.tsx`

- [x] **Step 1: Add numeric state**

Settings page reads provider, intensity, and voice activity threshold from `readNoiseConfig()`.

- [x] **Step 2: Add numeric controls**

Add number inputs:

- `Intensity`, `min=0`, `max=1`, `step=0.05`
- `Voice activity threshold`, `min=0`, `max=1`, `step=0.05`

- [x] **Step 3: Save full config**

Save writes the exact numeric values to local storage with the selected provider.

- [x] **Step 4: Add UI test**

Render settings, edit nickname/provider/intensity/threshold, click save, and assert local storage contains the exact values.

## Task 3: Docs and Verification

**Files:**
- Modify: `README.md`
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`

- [x] **Step 1: Update docs**

Document that settings supports provider, intensity, and voice activity threshold.

- [x] **Step 2: Verify**

Run:

```bash
npm test -- --run
npm run typecheck
npm run lint
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```
