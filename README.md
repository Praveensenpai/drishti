<div align="center">

# 👁️ Drishti

### *ದೃಷ್ಟಿ — "sight"*

**A beautiful terminal browser for [1TamilMV](https://www.1tamilmv.io) — digital HD rips only (WEB‑DL / HDRip / WEBRip), no DVDRip, CAM or theater prints — and only the original language.**

[![Latest Release](https://img.shields.io/github/v/release/Praveensenpai/drishti?style=for-the-badge&color=cba6f7)](https://github.com/Praveensenpai/drishti/releases)
[![Platform](https://img.shields.io/badge/Platform-Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)](https://github.com/Praveensenpai/drishti)
[![License](https://img.shields.io/badge/License-MIT-89b4fa?style=for-the-badge)](LICENSE)

[Install](#-install) • [Use](#-use) • [Update](#-update) • [Disclaimer](#-disclaimer) • [License](#-license)

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

- 🎯 **No junk rips** — DVDRip, DVD, CAM / CAMHD and theater prints are filtered out; WEB‑DL, HDRip and WEBRip stay.
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
> **One command. No fuss.** The installer drops in a prebuilt binary (no Rust needed), or builds from source as a fallback, and puts `drishti` on your `PATH`.

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
| `p` | open the selected **post** in your browser (magnet links live there) |
| `↵` | open the releases (sizes) for the selected post |
| `m` | copy the selected release's magnet link |
| `M` | POST the selected release's magnet to the configured `magnet_post_url` (optional, off by default) |
| `?` | help |
| `q` | quit |

---

## 🔄 Update

Drishti checks for a newer version once a day on first launch and updates itself in the background. To update manually:

```bash
drishti update
```

---

## ⚠️ Disclaimer

**This project is not affiliated with, endorsed by, or connected to 1TamilMV or any of its owners/operators in any way.**

Drishti is an independent, open-source tool that only reads public forum pages and helps you organise the information shown there. It does **not** host, store, upload, or distribute any media, and it does not bypass any access controls, logins, paywalls, or geo-restrictions.

The sites this tool browses may host copyrighted material. **You are solely responsible for what you access or download and for complying with the laws of your country.** The author assumes **no liability** and makes **no warranties** for any use of this software, including any legal consequences of your actions. Use it only where you have the right to access the content.

---

## 📜 License

MIT — do what you want, no warranty.

<div align="center">
<sub>Made with ❤️ for the people who just want to watch the movie.</sub>
</div>