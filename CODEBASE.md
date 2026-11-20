# CODEBASE.md: drushti Semantic Digest

> **Notice**: AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow
```text
Entrypoint ──> CLI/Parser ──> Domain Logic ──> Infra/IO
                └─► New: FuzzySearch Engine (src/search.rs)
```

## 2. Global Constraints & Architecture Patterns
- **Primary Language**: Rust 2024 edition
- **Architectural Paradigm**: Role-based (domain/, infra/, api/cli/, tui/)
- **Hard Constraints**: <400 lines/file, <60 lines/fn, zero production unwrap(), 0 warnings.
- **Target Distribution**: Linux x86_64 standalone binary

## 3. Module & Interface Skeleton

### `src/search.rs` (Role: general, Lines: ~180)
- **Responsibility**: Core fuzzy movie-title matching engine. Subsequence scoring with case-insensitive matching and character indices for UI highlighting.
- **Types**:
  ```rust
  pub struct MatchResult { score: i64, text: String, indices: Vec<u32> }
  ```
- **Public Functions**:
  ```rust
  fn fuzzy_match(query: &str, text: &str) -> Option<MatchResult>
  fn match_thread(query: &str, movie: &str, title: &str) -> Option<MatchResult>
  ```

### `src/app.rs` (Role: general, Lines: +50)
- **Added state**: `search_open`, `search_draft`, `saved_query`, `search_results`, `result_selected`, `search_dirty`
- **Added Functions**:
  ```rust
  fn set_query(&mut self, query: String)
  fn rebuild_search_results(&mut self)
  fn jump_to_result(&mut self, idx: usize)
  ```
- **Updated**: `visible_threads()`, `thread_passes()` — bypassed when search overlay active

### `src/main.rs` (Role: general, Lines: +30)
- **Added**: `/` key binding to open fuzzy search overlay
- **New handler**: `handle_search_key()` — printable chars append, Backspace deletes, Enter commits+jumps, Esc cancels, ↑/↓ navigate results

### `src/ui/mod.rs` (Role: tui, Lines: +20)
- **Updated**: `draw()` — renders search overlay banner on top of active screen
- **New**: Search banner shows query + result count, ranked list with scores

## 4. Execution Lifecycle Trace
1. **Startup**: Entrypoint parses CLI flags & dispatches command.
2. **Execution**: Core domain logic processes inputs and evaluates rules.
   - New: User presses `/` → search overlay opens → typed query fed to `match_thread()` → ranked results displayed.
3. **Persistence / I/O**: Domain logic calls infra for disk/terminal I/O.
4. **Exit**: Graceful termination with standard exit codes.

## 5. Verification Commands
```bash
cargo build --release --target x86_64-unknown-linux-gnu
cargo test --all-targets
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```