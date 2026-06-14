# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lyre is high performance VOIP chat app

### Lint
Make sure pass the clippy and rustfmt test via: `cargo clippy` and `cargo fmt` after edit files.

Split modules into multiple files when files grow too large. Start split when file exceed 400 line of file (LOC).

Run `cargo nextest run --manifest-path "Cargo.toml" --workspace` test after task done.

### Workspace Crates
crates/lyre-core - 软件核心
crates/lyre-app - 软件 cli，使用 clap 实现
crates/lyre-web - Web 接口，使用 WebRPC 和 RESTFul API 和前端进行交互
crates/lyre-noise-cancelling - 噪声消除, 先支持 RNNoise / DeepFilterNet, 在服务端处理完才广播给各个客户端
crates/lyre-turn - optional embedded UDP TURN relay adapter around the MIT `turn-server` crate
crates/lyre-webrtc - dependency-isolated Rust WebRTC server session boundary around the `webrtc` crate. Direct `webrtc` imports belong only in this crate until the media termination design is complete.
frontend/ - Next.js + React 前端，使用 Tailwind CSS 和本地 shadcn-style UI primitives。前端运行时使用 `APP_BASE_URL` 表示自身公开 URL，`APP_API_URL` 表示 Rust API URL。

`lyre-core::media` owns the media relay state skeleton and DTOs. It is a control-plane/API boundary only until a real WebRTC media termination and audio processing runtime is added.

### Docker Images
打包为两个镜像：
- `lyre-api` - Rust Axum REST/WebSocket API，默认监听 8080。
- `lyre-web` - Next.js standalone 前端，默认监听 3000。



## Key Dependencies

- **Async runtime**: tokio (multi-threaded)
- **Web framework**: axum + tower
- **Concurrent maps**: `dashmap` for hot-path instance-owned maps such as live connection statistics and DNS reverse-map snooping
- **TURN REST credentials**: `hmac`, `sha1`, and `base64` generate short-lived shared-secret TURN credentials for configured TURN/TURNS ICE servers. These are third-party dependencies, not new workspace crates.
- **Embedded TURN relay**: `turn-server` is the MIT service crate from the `turn-rs` project used by `crates/lyre-turn` for optional UDP TURN relay. The GPL `turn-rs` crate is intentionally not used.
- **Server WebRTC boundary**: `lyre-webrtc` isolates the `webrtc` crate (`webrtc-rs`) behind Lyre-owned session/control types. Do not import `webrtc` directly from `lyre-core` or `lyre-web`.

## Memory Files
### docs/roadmap.md
当前的路线图，要求每次更新完代码都更新路线图，列出已经完成了什么，接下来需要做什么

## 项目要求
- No legacy fallback
- 不要吞掉底层异常或 cause/context 链。跨运行时、reload、listener、配置源、网络/系统调用等边界记录或返回错误时，必须保留下层错误信息方便排查；Rust `anyhow` 错误进入日志时优先使用 `{err:#}` / `{error:#}` 或等价完整 context 链格式，而不是只输出最外层 `Display`。
- Ignore superpowers:using-git-worktrees

## Avoid over-engineering. Only make changes that are directly requested or clearly necessary. Keep solutions simple and focused.
- Don't add features, refactor code, or make "improvements" beyond what was asked. A bug fix doesn't need surrounding code cleaned up. A simple feature doesn't need extra configurability. Don't add docstrings, comments, or type annotations to code you didn't change. Only add comments where the logic isn't self-evident.
- Don't add error handling, fallbacks, or validation for scenarios that can't happen. Trust internal code and framework guarantees. Only validate at system boundaries (user input, external APIs). Don't use feature flags or backwards-compatibility shims when you can just change the code.
- Don't create helpers, utilities, or abstractions for one-time operations. Don't design for hypothetical future requirements. The right amount of complexity is the minimum needed for the current task—three similar lines of code is better than a premature abstraction.
- Avoid backwards-compatibility hacks like renaming unused _vars, re-exporting types, adding // removed comments for removed code, etc. If you are certain that something is unused, you can delete it completely.


## DO NOT OVER-DEFEND

- Only add defensive checks (null/nil/None checks, type guards, boundary validation) at true system boundaries — public API entry points that accept external, untrusted input.
- Do not add defensive checks in internal/private functions, constructors called only by your own code, or test helpers.
- Do not add defensive copies unless the data is genuinely shared across trust boundaries.
- Omitting a defensive check is not a bug — it is a deliberate signal that the caller is trusted.

## USE MODERN LANGUAGE FEATURES

- Write idiomatic code for the language version specified by the project. Do not write code that targets an older version out of habit.
- Prefer language-level constructs that reduce boilerplate: pattern matching, destructuring, algebraic data types (sealed types, tagged unions, enums with data), data classes/records/structs, and built-in concurrency primitives.
- If the language provides exhaustiveness checking (e.g., sealed types + switch, match expressions, tagged unions), use it. Compiler-enforced completeness is better than a default/else branch that hides missing cases.
- Do not manually write what the language generates for free (toString, equality, hash, serialization).

## Code Rules

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.
