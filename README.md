<div align="center">

# 👁️ Drishti

### *ದೃಷ್ಟಿ — "sight"*

**A beautiful terminal browser for [1TamilMV](https://www.1tamilmv.io) — WEB‑DL only, and only the original language.**

[![Latest Release](https://img.shields.io/github/v/release/Praveensenpai/drishti?style=for-the-badge&color=cba6f7)](https://github.com/Praveensenpai/drishti/releases)
[![Platform](https://img.shields.io/badge/Platform-Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)](https://github.com/Praveensenpai/drishti)
[![License](https://img.shields.io/badge/License-MIT-89b4fa?style=for-the-badge)](LICENSE)

[Install](#-install) • [Use](#-use) • [Update](#-update) • [License](#-license)

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

- 🎯 **WEB‑DL only** — no DVDRip, no theater prints, no cam rips. Ever.
- 🌐 **Original language** — knows which language a film was *actually* made in, so you can hide the dubbed copies.
- 🧠 **Smart detection** — reads multi‑audio tags instantly, asks Gemini only when it must, and remembers the answer.
- ⚡ **Fast** — pages and releases are cached for 30 minutes, so switching pages is instant.
- 🖥️ **100% keyboard** — `↑`/`↓` to move, `↵` to open, `←`/`→` to change page.

---

## 🚀 Install

```bash
curl -fsSL https://raw.githubusercontent.com/Praveensenpai/drishti/main/install.sh | bash
```

> [!TIP]
> **One command. No fuss.** The installer sets up Rust if needed, builds Drishti, and puts `drishti` on your `PATH`.

---

## 🎬 Use

```bash
drishti
```

That's it. Pick a language, press `↵`, and it scans the board for you.

| Key | Action |
|:---:|---|
| `↑` `↓` | move |
| `↵` | open thread / choose language |
| `←` `→` | previous / next page |
| `o` | toggle original‑language only |
| `f` | filters |
| `d` | downloads |
| `?` | help |
| `q` | quit |

---

## 🔄 Update

Drishti checks for a newer version once a day on first launch and updates itself in the background. To update manually:

```bash
drishti update
```

---

## 📜 License

MIT — do what you want, no warranty.

<div align="center">
<sub>Made with ❤️ for the people who just want to watch the movie.</sub>
</div>