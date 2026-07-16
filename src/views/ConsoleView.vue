<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import RFB from "@novnc/novnc";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { api, type GuestKind } from "../api";
import { activeId } from "../stores/connections";

const route = useRoute();
const node = route.params.node as string;
const kind = route.params.kind as GuestKind;
const vmid = Number(route.params.vmid);
// VMs speak VNC (noVNC canvas); containers speak a plain terminal stream.
const mode: "vnc" | "term" = kind === "qemu" ? "vnc" : "term";

const screen = ref<HTMLElement | null>(null);
const kbd = ref<HTMLInputElement | null>(null);
const status = ref("connecting…");
const error = ref("");
const ctrl = ref(false);
const zoom = ref(1);
const pan = ref({ x: 0, y: 0 });

let rfb: RFB | null = null;
let term: Terminal | null = null;
let ws: WebSocket | null = null;
let pingTimer: number | undefined;
const enc = new TextEncoder();

// --- mobile input (#50) ---

const KEYSYMS: Record<string, number> = {
  Escape: 0xff1b,
  Tab: 0xff09,
  Enter: 0xff0d,
  Backspace: 0xff08,
  Delete: 0xffff,
  ArrowUp: 0xff52,
  ArrowDown: 0xff54,
  ArrowLeft: 0xff51,
  ArrowRight: 0xff53,
};
const CTRL_L = 0xffe3;

/// Press+release a key, wrapping it in a held Ctrl when the sticky
/// toggle is armed.
function vncKey(keysym: number, code = "") {
  if (!rfb) return;
  if (ctrl.value) rfb.sendKey(CTRL_L, "ControlLeft", true);
  rfb.sendKey(keysym, code);
  if (ctrl.value) {
    rfb.sendKey(CTRL_L, "ControlLeft", false);
    ctrl.value = false;
  }
}

function termSend(data: string) {
  ws?.send(`0:${enc.encode(data).length}:${data}`);
}

function sendNamedKey(name: "Escape" | "Tab") {
  if (mode === "vnc") vncKey(KEYSYMS[name], name);
  else {
    termSend(name === "Escape" ? "\x1b" : "\t");
    ctrl.value = false;
  }
}

// Soft keyboard: an offscreen input the ⌨ button focuses. A one-space
// sentinel makes Android IMEs report backspace as a shortened value.
function openKeyboard() {
  if (mode === "term") {
    term?.focus();
    return;
  }
  kbd.value!.value = " ";
  kbd.value!.focus();
}

function onKbdInput(e: Event) {
  const el = e.target as HTMLInputElement;
  const v = el.value;
  if (v.length === 0) vncKey(KEYSYMS.Backspace, "Backspace");
  // ponytail: latin1 keysyms only — CJK/emoji IME input needs a keysym table
  for (const ch of v.slice(1)) {
    const cp = ch.codePointAt(0)!;
    if (cp >= 0x20 && cp <= 0xff) vncKey(cp);
  }
  el.value = " ";
}

function onKbdKeydown(e: KeyboardEvent) {
  const ks = KEYSYMS[e.key];
  if (ks) {
    e.preventDefault();
    vncKey(ks, e.code);
  }
}

// Two-finger pinch zoom / pan, capture phase so noVNC (which eats
// single-finger touches for mouse emulation) never sees them.
let pinch: { d: number; cx: number; cy: number } | null = null;

function onTouch(e: TouchEvent) {
  if (mode !== "vnc") return;
  if (e.touches.length !== 2) {
    pinch = null;
    return;
  }
  e.preventDefault();
  e.stopPropagation();
  const [a, b] = [e.touches[0], e.touches[1]];
  const d = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  const cx = (a.clientX + b.clientX) / 2;
  const cy = (a.clientY + b.clientY) / 2;
  if (pinch) {
    zoom.value = Math.min(4, Math.max(1, (zoom.value * d) / pinch.d));
    pan.value =
      zoom.value === 1
        ? { x: 0, y: 0 }
        : { x: pan.value.x + cx - pinch.cx, y: pan.value.y + cy - pinch.cy };
  }
  pinch = { d, cx, cy };
}

function attachVnc(url: string, ticket: string) {
  rfb = new RFB(screen.value!, url, {
    credentials: { password: ticket },
    wsProtocols: ["binary"],
  });
  rfb.scaleViewport = true;
  rfb.addEventListener("connect", () => (status.value = "connected"));
  rfb.addEventListener("disconnect", () => (status.value = "disconnected"));
  rfb.addEventListener("securityfailure", () => (error.value = "VNC auth failed"));
}

function attachTerm(url: string, ticket: string, user: string) {
  term = new Terminal({ cursorBlink: true });
  term.open(screen.value!);
  ws = new WebSocket(url, ["binary"]);
  ws.binaryType = "arraybuffer";
  const dec = new TextDecoder();

  ws.onopen = () => {
    // pve-xtermjs handshake: auth line, then length-prefixed input frames,
    // "1:cols:rows:" resizes and "2" keepalives.
    ws!.send(`${user}:${ticket}\n`);
    status.value = "connected";
    term!.onData((d) => {
      let out = d;
      // Sticky Ctrl from the toolbar: fold the next letter into a control char.
      if (ctrl.value && d.length === 1) {
        const c = d.toUpperCase().charCodeAt(0);
        if (c >= 64 && c <= 95) out = String.fromCharCode(c & 31);
        ctrl.value = false;
      }
      termSend(out);
    });
    term!.onResize(({ cols, rows }) => ws?.send(`1:${cols}:${rows}:`));
    ws!.send(`1:${term!.cols}:${term!.rows}:`);
    pingTimer = window.setInterval(() => ws?.send("2"), 30_000);
    term!.focus();
  };
  ws.onmessage = (ev) => {
    term!.write(typeof ev.data === "string" ? ev.data : dec.decode(ev.data));
  };
  ws.onclose = () => (status.value = "disconnected");
  ws.onerror = () => (error.value = "websocket error");
}

onMounted(async () => {
  if (!activeId.value) {
    error.value = "No active connection.";
    return;
  }
  try {
    const info = await api.openConsole(activeId.value, node, kind, vmid, mode);
    const url = `ws://127.0.0.1:${info.port}`;
    if (mode === "vnc") attachVnc(url, info.ticket);
    else attachTerm(url, info.ticket, info.user ?? "");
  } catch (e) {
    error.value = String(e);
  }
});

onBeforeUnmount(() => {
  window.clearInterval(pingTimer);
  rfb?.disconnect();
  ws?.close();
  term?.dispose();
});
</script>

<template>
  <div class="console-page">
    <div class="head">
      <h1>Console — {{ kind === "qemu" ? "VM" : "CT" }} {{ vmid }} <small>on {{ node }}</small></h1>
      <span
        class="status"
        :class="status"
      >{{ status }}</span>
      <router-link :to="`/guests/${node}/${kind}/${vmid}`">
        Back to detail
      </router-link>
    </div>
    <p
      v-if="error"
      class="error"
    >
      {{ error }}
    </p>
    <div class="toolbar">
      <button @click="sendNamedKey('Escape')">
        Esc
      </button>
      <button @click="sendNamedKey('Tab')">
        Tab
      </button>
      <button
        :class="{ armed: ctrl }"
        @click="ctrl = !ctrl"
      >
        Ctrl
      </button>
      <button
        v-if="mode === 'vnc'"
        @click="rfb?.sendCtrlAltDel()"
      >
        Ctrl+Alt+Del
      </button>
      <button @click="openKeyboard">
        ⌨ Keyboard
      </button>
    </div>
    <div
      class="viewport"
      @touchstart.capture="onTouch"
      @touchmove.capture="onTouch"
      @touchend.capture="onTouch"
    >
      <div
        ref="screen"
        class="screen"
        :style="{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }"
      />
    </div>
    <input
      ref="kbd"
      class="kbd-input"
      autocapitalize="off"
      autocomplete="off"
      spellcheck="false"
      @input="onKbdInput"
      @keydown="onKbdKeydown"
    >
  </div>
</template>

<style scoped>
.console-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 10px;
}

.head {
  display: flex;
  align-items: center;
  gap: 16px;
}

.head h1 {
  margin-right: auto;
}

.head small {
  font-weight: normal;
  opacity: 0.6;
}

.status {
  font-size: 0.85em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #8883;
}

.status.connected {
  background: #2a72;
  color: #2a7;
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.toolbar .armed {
  border-color: #e57000;
  background: #e57000;
  color: #fff;
}

.viewport {
  flex: 1;
  min-height: 320px;
  border-radius: 6px;
  overflow: hidden;
  background: #000;
}

.screen {
  height: 100%;
  transform-origin: center;
}

/* Offscreen but focusable — focusing it summons the soft keyboard. */
.kbd-input {
  position: fixed;
  bottom: 0;
  left: 0;
  width: 1px;
  height: 1px;
  opacity: 0;
  border: none;
  padding: 0;
}

.error {
  color: #c33;
}
</style>
