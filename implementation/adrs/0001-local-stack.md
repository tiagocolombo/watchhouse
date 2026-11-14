# ADR 0001 — Rust coordinator and bundled TypeScript UI

Date: 2026-11-14. Status: accepted for M0/M1. Resolves P01/P05.

## Context and decision

Keep the documented Rust proposal: edition 2024, Rust 1.90.0 pinned in rust-toolchain.toml, one Cargo package, Axum HTTP coordinator and Clap CLI. Both call Application; neither writes SQL itself. Domain state/time types have no provider, model, HTTP, or persistence dependency. Infrastructure errors live separately.

Use a small framework-free TypeScript DOM application compiled by TypeScript 5.9.3; Node 22+ builds it. Serve compile-time bundled JS, HTML, CSS, and the existing brand assets from the binary. Build web assets before Cargo. Cargo.lock and web/package-lock.json pin dependencies. There is no Node server at runtime.

Alternatives: Python would bootstrap quickly but depart from the existing distribution proposal; a separate frontend server adds packaging and origin/auth complexity. A UI framework can be introduced when the operating workspace needs it; it is unnecessary for this case/timeline slice.

## Evidence and consequences

Rust toolchain was installed and the implementation compiled locally. TypeScript strict compilation, Rust checks, CLI/HTTP integration, and Chromium workflow tests validate the package boundary. CI declares Linux/macOS/Windows Rust checks, with browser acceptance on Linux; only Linux has been exercised locally. This is not a portability certification or a packaged release.

The fake executor runs in-process and has no external effects. Worker subprocesses, real harness isolation, and cancellation remain M2/M4 work; this exception must never silently admit a real adapter.
