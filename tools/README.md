# tools/

Dev-box scripts. Not shipped, not built, not on any CI path.

## `gates.py` — run every local gate, once

```sh
python tools/gates.py            # all seven, stops at the first failure
python tools/gates.py --only rust
python tools/gates.py --list
```

Same commands as CLAUDE.md's "Gates" section, one step each, in order. Adds
`~/.cargo/bin` to PATH (a fresh Windows shell does not have `cargo`), gives every
step a timeout so a hang fails instead of stalling, prints a progress bar on a
terminal and a "still running" line every 30s when piped, and ends with a
pass/fail table plus the tail of whatever broke.

## `cdp.mjs` — drive the running app

The app's GUI is checked by talking to its webview over the Chrome DevTools
Protocol, not by moving the mouse. **Windows/WebView2 only** — WebKitGTK exposes
no CDP endpoint.

```sh
node tools/cdp.mjs dev                      # `pnpm tauri dev` with CDP on :9222
node tools/cdp.mjs eval "location.hash"
node tools/cdp.mjs eval -f probe.js
node tools/cdp.mjs type --selector '.xterm-helper-textarea' "docker ps"
```

`eval` runs an expression in the page, awaits a promise, prints the value as
JSON. `type` uses the CDP `Input` domain, which produces real key events —
xterm.js ignores a `.value` assignment, so this is the only way to drive the
terminal tab.

Why not synthetic mouse/keyboard: this works with the window unfocused, needs no
screen coordinates, and cannot spray keystrokes into whatever else is on the
desktop.

Because `pnpm tauri dev` serves the frontend through Vite unbundled, an eval can
import app modules directly — so every Tauri command is callable without going
near the UI:

```sh
node tools/cdp.mjs eval '(async () => {
  const { api } = await import("/src/api.ts");
  const { activeId } = await import("/src/stores/connections.ts");
  return JSON.stringify(await api.hostPorts(activeId.value));
})()'
```

A GUI pass is usually: click a nav entry, wait, then dump route + errors + text.

```sh
node tools/cdp.mjs eval '(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const link = [...document.querySelectorAll("nav a")].find((a) => a.textContent.trim() === "Docker");
  if (!link) return "no such nav entry";
  link.click();
  await sleep(4000);
  return JSON.stringify({
    route: location.hash,
    errors: [...document.querySelectorAll(".error")].map((e) => e.textContent.trim()),
    text: document.body.innerText.replace(/[ \t]+/g, " ").trim().slice(0, 1500),
  }, null, 1);
})()'
```

This exists because #112 happened: four tabs shipped with their commands verified
over raw SSH and their views never opened, and two of the three defects failed
silently. Green gates do not mean the tab renders.

## `network-scan.py`

Ping-sweep a subnet, report used and free addresses. Used to find a new host's IP
before it is in DNS.
