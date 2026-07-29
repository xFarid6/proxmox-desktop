<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { api } from "../api";
import { activeId } from "../stores/connections";

// Same pve-xtermjs wire protocol as ConsoleView.vue's term mode -- the
// backend bridge (ssh_console.rs) speaks it too, so this is the same
// wiring, just against an SSH shell instead of a Proxmox termproxy.

const route = useRoute();
const node = route.params.node as string;

const screen = ref<HTMLElement | null>(null);
const status = ref("connecting…");
const error = ref("");
const ctrl = ref(false);

let term: Terminal | null = null;
let ws: WebSocket | null = null;
let pingTimer: number | undefined;
const enc = new TextEncoder();

function termSend(data: string) {
  ws?.send(`0:${enc.encode(data).length}:${data}`);
}

function sendNamedKey(name: "Escape" | "Tab") {
  termSend(name === "Escape" ? "\x1b" : "\t");
  ctrl.value = false;
}

onMounted(async () => {
  if (!activeId.value) {
    error.value = "No active connection.";
    return;
  }
  try {
    const info = await api.openSshShell(activeId.value);
    term = new Terminal({ cursorBlink: true });
    term.open(screen.value!);
    ws = new WebSocket(`ws://127.0.0.1:${info.port}`, ["binary"]);
    ws.binaryType = "arraybuffer";
    const dec = new TextDecoder();

    ws.onopen = () => {
      ws!.send(`${info.user ?? ""}:${info.ticket}\n`);
      status.value = "connected";
      term!.onData((d) => {
        let out = d;
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
  } catch (e) {
    error.value = String(e);
  }
});

onBeforeUnmount(() => {
  window.clearInterval(pingTimer);
  ws?.close();
  term?.dispose();
});
</script>

<template>
  <div class="ssh-page">
    <div class="head">
      <h1>SSH shell — {{ node }}</h1>
      <span
        class="status"
        :class="status"
      >{{ status }}</span>
      <router-link to="/dashboard">
        Back to dashboard
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
    </div>
    <div
      ref="screen"
      class="screen"
    />
  </div>
</template>

<style scoped>
.ssh-page {
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

.screen {
  flex: 1;
  min-height: 320px;
  border-radius: 6px;
  overflow: hidden;
  background: #000;
}

.error {
  color: #c33;
}
</style>
