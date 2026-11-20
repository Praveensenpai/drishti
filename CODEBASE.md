# drushti - CODEBASE Index

**Project**: drushti - A TUI browser/downloader for 1TamilMV: WEB-DL only, original-language aware

**Current version**: 0.4.0 (tag: v0.4.0)

---

## Architecture

- **domain/**: Core business logic and data models
- **infra/**: Infrastructure integrations (reqwest, crossterm, tokio)
- **api/**: TUI UI layer (ratatui-based)

## Public API

### Key Types

- `App` - Main application state struct (`src/app.rs`)
- `MatchResult` - Search result with score and highlight indices (`src/search.rs`)
- `match_thread()` - Fuzzy search matching movie name then title (`src/search.rs`)
- `fuzzy_match()` - Subsequence fuzzy matching (`src/search.rs`)

### Key Functions

- `App::rebuild_search_results()` - Rebuild ranked search results from all loaded pages
- `App::jump_to_result()` - Jump to a search result, close overlay
- `App::set_query()` - Set search draft query and mark results as dirty
- `fuzzy_match(query, text)` - Case-insensitive subsequence matcher
- `match_thread(query, movie, title)` - Match query against thread, prefers movie name

### Key Bindings

| Key | Action |
|-----|--------|
| `/` | Open search prompt in browser screen |
| `Esc` / `q` | Close search, restore draft query |
| `Enter` | Jump to first search result |
| `j` / `k` / `Up` / `Down` | Navigate search results |

---

## Module Dependencies

```
src/main.rs          → mod search; (fuzzy search engine)
src/app.rs           → use crate::search::{match_thread, MatchResult}
src/search.rs        → fuzzy_match(), match_thread(), MatchResult struct
src/ui/mod.rs        → search overlay rendering
```

---

## Recent Changes (v0.4.0)

- **feat**: add fuzzy movie search with `/` key shortcut, `Esc`/`q` to close
- **chore**: bump version to 0.4.0