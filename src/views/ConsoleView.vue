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
const status = ref("connecting…");
const error = ref("");

let rfb: RFB | null = null;
let term: Terminal | null = null;
let ws: WebSocket | null = null;
let pingTimer: number | undefined;

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
  const enc = new TextEncoder();
  const dec = new TextDecoder();

  ws.onopen = () => {
    // pve-xtermjs handshake: auth line, then length-prefixed input frames,
    // "1:cols:rows:" resizes and "2" keepalives.
    ws!.send(`${user}:${ticket}\n`);
    status.value = "connected";
    term!.onData((d) => ws?.send(`0:${enc.encode(d).length}:${d}`));
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
    <div
      ref="screen"
      class="screen"
    />
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

.screen {
  flex: 1;
  min-height: 480px;
  background: #000;
  border-radius: 6px;
  overflow: hidden;
}

.error {
  color: #c33;
}
</style>
