<div align="center">

# 👁️ Drushti

### *ದೃಷ್ಟಿ · "sight"*

**A terminal browser for [1TamilMV](https://www.1tamilmv.ceo) that keeps only digital HD rips (`WEB-DL`, `HDRip`, `WEBRip`) and tells you the language the film was originally made in.**

[![Latest Release](https://img.shields.io/github/v/release/Praveensenpai/drushti?style=for-the-badge&color=cba6f7)](https://github.com/Praveensenpai/drushti/releases)
[![Rust Edition](https://img.shields.io/badge/Rust-2024%20Edition-DEA584?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)](https://github.com/Praveensenpai/drushti)
[![Architecture](https://img.shields.io/badge/Arch-x86__64-94e2d5?style=for-the-badge)](https://github.com/Praveensenpai/drushti/releases)
[![License](https://img.shields.io/badge/License-MIT-89b4fa?style=for-the-badge)](LICENSE)

[✨ Features](#-what-it-does) • [🪄 Install](#-quick-start) • [🎬 Use](#-use) • [⚙️ Configuration](#%EF%B8%8F-configuration) • [🔄 Update](#-update) • [⚠️ Disclaimer](#%EF%B8%8F-disclaimer) • [📜 License](#-license)

</div>

---

## ✨ What it does

```text
┌──────────────────────────────────────────────┐
│  1TamilMV   Kannada WEB-HD   page 1/15       │
├──────────────────────────────────────────────┤
│  #  Movie              Original  Quality     │
│  1  Heggana Muddu      Kannada   WEB-DL      │
│  2  Jailer 2           Tamil     WEB-DL      │
│  3  Vikalpa            Kannada   WEB-DL      │
└──────────────────────────────────────────────┘
```

- 🎯 **No junk rips** — `DVDRip`, `DVD`, `CAM` and theater prints are filtered out; `WEB-DL`, `HDRip` and `WEBRip` stay.
- 🌐 **Original language** — resolves the production language with Gemini plus a local heuristic, then caches the answer for a day.
- 🔍 **Fuzzy movie search** — press `/` and type a partial title such as `pddi` or `jlr2`; results rank across every loaded page.
- 🔄 **Self-healing domain** — probes the board at startup. If the domain rotated, it searches Bing, probes each `1tamilmv` candidate, and saves the working one.
- ⚡ **Fast** — pages and releases are cached on disk for `30` minutes, so switching pages is instant.
- 🎨 **Colorful CLI** — bun and uv style spinners and colored status for `list`, `resolve`, `probe` and `update`; piped output and `NO_COLOR` stay plain.
- 🖥️ **100% keyboard** — `↑`/`↓` to move, `↵` to open, `←`/`→` to change page.

> [!NOTE]
> **The board changes domains often.** Drushti checks the configured domain on launch. A failed probe triggers a Bing search, and the first candidate that serves a real forum listing becomes the new `base_url` in `config.toml`. Set `auto_discover_domain = false` to disable it. A failed search retries after one hour.

```text
┌──────────────────┐        ┌────────────────────┐        ┌─────────────────────┐
│  Startup         │        │  Configured domain │        │  Forum listing      │
│  drushti         │───────▶│  GET forum page    │───────▶│  ipsDataItem found  │
└──────────────────┘        └─────────┬──────────┘        └──────────┬──────────┘
                                      │ dead                        │ healthy
                                      ▼                             ▼
                            ┌────────────────────┐          ┌──────────────────┐
                            │  Bing search       │          │  Continue, no    │
                            │  + result text     │          │  search needed   │
                            └─────────┬──────────┘          └──────────────────┘
                                      ▼
                            ┌────────────────────┐
                            │  Probe each        │
                            │  1tamilmv domain   │
                            └─────────┬──────────┘
                                      ▼
                            ┌────────────────────┐
                            │  Save winner to    │
                            │  config.toml       │
                            └────────────────────┘
```

---

## 🚀 Quick Start

### 🪄 One-Liner Magic (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/Praveensenpai/drushti/main/install.sh | bash
```

> [!TIP]
> **One command.** The installer fetches a prebuilt binary, so Rust is not required. It falls back to building from source when no release binary is available.

<br>

### 🛠️ Building From Source

```bash
git clone https://github.com/Praveensenpai/drushti.git
cd drushti
cargo build --release
install -Dm 755 target/release/drushti ~/.local/bin/drushti
```

---

## 🎬 Use

```bash
drushti
```

Pick a language, press `↵`, and it scans the board for you.

| Key | Action |
|:---:|---|
| `↑` `↓` | move selection |
| `↵` | open thread releases / choose language |
| `←` `→` | previous / next page |
| `/` | fuzzy search loaded movies |
| `f` | quality and language filters |
| `o` | toggle original-language only |
| `[` `]` | switch forum |
| `g` `G` | first / last row |
| `p` | open the selected post in your browser |
| `u` | copy the post link to the clipboard |
| `m` | copy the selected release's magnet link |
| `M` | POST the magnet to `magnet_post_url` (off by default) |
| `s` | rescan the page |
| `?` | help |
| `q` | quit |

Non-interactive examples:

```bash
drushti list --forum kannada --page 1
drushti list --forum kannada --page 1 --json --releases
drushti resolve --forum kannada --page 1
drushti probe
```

### 🎨 Colorful output

Non-interactive commands use a spinner while pages load and color status lines: cyan headings, green checks, yellow warnings, dim URLs.

```text
  ✓ Kannada WEB-HD  page 1/15  (25 threads)
    1  Heggana Muddu      WEB-DL    Kannada     https://www.1tamilmv.ceo/...
```

> [!TIP]
> **Pipes stay clean.** Colour is enabled only in a terminal. `list --json` emits plain JSON, and `NO_COLOR=1` disables colour everywhere.

---

## ⚙️ Configuration

Settings live at `~/.config/drushti/config.toml` and are created on first run. The wizard asks for every value when no file exists.

```toml
# Board address. Overwritten automatically when discovery finds a new domain.
base_url = "https://www.1tamilmv.ceo"

# Probe the board at startup and rediscover its domain when unreachable.
auto_discover_domain = true

# Search terms and endpoint used for domain discovery.
domain_search_query = "1tamilmv"
domain_search_endpoint = "https://www.bing.com/search"

# Resolve original languages with Gemini as pages load.
resolve_on_load = true

# Disk cache lifetime for forum and thread pages, in minutes.
cache_ttl_minutes = 30

# How long a model-resolved language stays fresh, in hours.
ai_cache_hours = 24

# Quality buckets kept by the filter.
allowed_qualities = ["WebDl", "HdRip", "WebRip", "Bluray", "TvRip", "Unknown"]

# Optional endpoint that receives POSTed magnets. Empty disables it.
magnet_post_url = ""
```

### CLI Overrides

- `--config <FILE>`: use a different configuration file.
- `--base-url <URL>`: override the board address for this run.
- `--gemini-key <KEY>`: override the Gemini API key, or set `GEMINI_API_KEY`.
- `--no-cache`: always re-fetch pages.
- `--from <PAGE>` / `--to <PAGE>`: narrow the scan range.

---

## 🔄 Update

Drushti checks for a newer version once a day on first launch and updates itself in the background. To update manually:

```bash
drushti update
```

---

## ⚠️ Disclaimer

**This project is not affiliated with, endorsed by, or connected to 1TamilMV or any of its owners/operators in any way.**

Drushti is an independent, open-source tool that only reads public forum pages and helps you organise the information shown there. It does **not** host, store, upload, or distribute any media, and it does not bypass any access controls, logins, paywalls, or geo-restrictions.

The sites this tool browses may host copyrighted material. **You are solely responsible for what you access or download and for complying with the laws of your country.** The author assumes **no liability** and makes **no warranties** for any use of this software, including any legal consequences of your actions. Use it only where you have the right to access the content.

---

## 📜 License

MIT. Do what you want, no warranty.

<div align="center">
<sub>Made with ❤️ for the people who just want to watch the movie.</sub>
</div>
