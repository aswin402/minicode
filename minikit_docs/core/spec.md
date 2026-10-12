# Technical Specifications & Invariant Rules 📐

## Core Engineering Invariants
1. **Zero Crash Invariant**: Zero unhandled panics or `.unwrap()` in production paths.
2. **Verification Barrier**: Compile checks and tests must pass before completing work.
3. **Portability Invariant**: Pure-system dependencies without native build friction.

## Technical Interface Contracts
- Protocol schemas, CLI arguments, and error representations for `minicode`.
