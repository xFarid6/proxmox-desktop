<script setup lang="ts">
import { toasts } from "../stores/toast";
</script>

<template>
  <div class="toasts">
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="toast"
        :class="t.kind"
      >
        {{ t.text }}
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  bottom: 16px;
  right: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 100;
}

.toast {
  padding: 10px 16px;
  border-radius: 6px;
  background: #333;
  color: #fff;
  font-size: 0.9em;
  box-shadow: 0 2px 8px #0004;
  max-width: 360px;
}

.toast.ok {
  border-left: 3px solid #2a7;
}

.toast.error {
  border-left: 3px solid #c33;
}

.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 0.2s,
    transform 0.2s;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
