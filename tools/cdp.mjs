#!/usr/bin/env node
// Drive the running app's webview over the Chrome DevTools Protocol.
//
//   node tools/cdp.mjs dev                    # launch `pnpm tauri dev` with CDP on :9222
//   node tools/cdp.mjs eval "<js expression>" # evaluate in the page, print the result
//   node tools/cdp.mjs eval -f probe.js       # same, from a file
//   node tools/cdp.mjs type "uptime"          # real key events into the focused element
//   node tools/cdp.mjs type --selector '.xterm-helper-textarea' "docker ps"
//
// Why this and not synthetic mouse/keyboard: CDP talks to the page directly, so
// it works with the window unfocused, needs no screen coordinates, and cannot
// leak stray input into whatever else is on the desktop. Vite serves the app
// unbundled in dev, so `await import("/src/api.ts")` inside an eval reaches every
// Tauri command the frontend has -- the backend is drivable, not just the DOM.
//
// WebView2 (Windows) only. WebKitGTK on Linux/macOS has no CDP endpoint.

import { readFileSync } from "node:fs";
import { spawn } from "node:child_process";

const PORT = process.env.PXX_CDP_PORT ?? "9222";

if (typeof WebSocket === "undefined") {
  die("needs Node 22+ (global WebSocket). Current: " + process.version);
}

function die(msg) {
  console.error(msg);
  process.exit(1);
}

function usage() {
  die(readFileSync(new URL(import.meta.url), "utf8")
    .split("\n")
    .filter((l) => l.startsWith("//"))
    .map((l) => l.replace(/^\/\/ ?/, ""))
    .join("\n"));
}

/** Open a CDP session against the app's page target. */
async function connect() {
  let targets;
  try {
    targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
  } catch {
    die(`no CDP endpoint on :${PORT}. Start the app with \`node tools/cdp.mjs dev\`.`);
  }
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) die("CDP is up but has no page target:\n" + JSON.stringify(targets, null, 2));

  const ws = new WebSocket(page.webSocketDebuggerUrl);
  const pending = new Map();
  let id = 0;
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    const p = pending.get(m.id);
    if (p) {
      pending.delete(m.id);
      if (m.error) p.rej(new Error(m.error.message));
      else p.res(m.result);
    }
  });
  ws.addEventListener("error", () => die("CDP websocket error"));
  await new Promise((r) => ws.addEventListener("open", r, { once: true }));

  const send = (method, params = {}) =>
    new Promise((res, rej) => {
      pending.set(++id, { res, rej });
      ws.send(JSON.stringify({ id, method, params }));
    });
  return { send, close: () => ws.close() };
}

/** Evaluate an expression in the page and return its value. */
async function evaluate(send, expression) {
  const out = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    allowUnsafeEvalBlockedByCSP: true,
  });
  if (out.exceptionDetails) {
    die("EXCEPTION: " + (out.exceptionDetails.exception?.description ?? out.exceptionDetails.text));
  }
  return out.result?.value;
}

const [cmd, ...rest] = process.argv.slice(2);

if (cmd === "dev") {
  if (process.platform !== "win32") {
    die("`dev` sets a WebView2 variable; on this platform the webview has no CDP endpoint.");
  }
  const child = spawn("pnpm", ["tauri", "dev"], {
    stdio: "inherit",
    shell: true, // pnpm is a .CMD on Windows
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  });
  child.on("exit", (code) => process.exit(code ?? 0));
} else if (cmd === "eval") {
  const expression = rest[0] === "-f" ? readFileSync(rest[1], "utf8") : rest[0];
  if (!expression) usage();
  const { send, close } = await connect();
  const value = await evaluate(send, expression);
  console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
  close();
} else if (cmd === "type") {
  // Input.* dispatches real key events, which xterm.js needs -- setting
  // .value from an eval does not reach it.
  let selector = null;
  if (rest[0] === "--selector") selector = rest.splice(0, 2)[1];
  const text = rest[0];
  if (text === undefined) usage();
  const { send, close } = await connect();
  if (selector) {
    const found = await evaluate(send, `!!document.querySelector(${JSON.stringify(selector)})?.focus()`);
    if (found === false) die(`no element matches ${selector}`);
  }
  await send("Input.insertText", { text });
  for (const type of ["keyDown", "keyUp"]) {
    await send("Input.dispatchKeyEvent", {
      type,
      key: "Enter",
      code: "Enter",
      windowsVirtualKeyCode: 13,
      ...(type === "keyDown" ? { text: "\r" } : {}),
    });
  }
  close();
} else {
  usage();
}
