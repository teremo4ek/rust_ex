# Playwright Browsers & MCP Setup

State as of 2026-11-16.

## How the MCP runs

Interactive UI testing uses the official `playwright@claude-plugins-official` plugin, which
launches `npx @playwright/mcp@latest`. There is no project `.mcp.json` anymore (the old one with
a pinned `--executable-path` was deleted in favor of the plugin).

MCP tools attach at session start — after toggling the plugin or changing its args, restart the
session.

## Where browsers live (macOS)

Downloaded Playwright browsers are in `~/Library/Caches/ms-playwright/` — **not** `~/.cache/ms-playwright`
(that's the Linux path; on macOS it exists but stays empty).

Currently installed:

| Directory | Needed by |
|---|---|
| `chromium-1254` | plugin's alpha playwright `1.66.0-alpha-2026-11-09` (pinned by `@playwright/mcp` 0.0.85+) |
| `chromium-1255` | local `@playwright/test` 1.65.1 (see `package.json`) |
| `chromium_headless_shell-1254/1255`, `firefox-1572`, `webkit-2388` | headless runs / other engines |

The "chromium" builds are actually **Google Chrome for Testing**; binaries:

```
~/Library/Caches/ms-playwright/chromium-1254/chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing
~/Library/Caches/ms-playwright/chromium-1255/chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing
```

## Which browser the MCP actually launches

By default the MCP launches **system Google Chrome** (channel `chrome`):

- binary: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- profile: a dedicated one under `~/Library/Caches/ms-playwright-mcp/mcp-chrome-*` (the main
  Chrome profile is untouched)

To launch the *downloaded* Chromium instead: `--browser` only accepts `chrome | firefox | webkit |
msedge`, so the downloaded build is reachable **only via `--executable-path`** pointing at one of
the binaries above. That is why the old `.mcp.json` pinned `--executable-path`. To restore that
setup, re-create `.mcp.json` with the flag and disable the plugin's MCP to avoid duplicates.

## Breakage fix

If a plugin update bumps its pinned playwright and the browser fails to launch with
"Executable doesn't exist", install the new revision:

```
npx -y playwright@<pinned-alpha-version> install chromium
```

Find the required revision from the error message (`chromium-<rev>` maps to the directory name
under `~/Library/Caches/ms-playwright/`).
