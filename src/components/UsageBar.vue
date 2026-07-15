<script setup lang="ts">
defineProps<{ label: string; value: number; detail?: string }>();
</script>

<template>
  <div class="usage">
    <div class="usage-head">
      <span>{{ label }}</span>
      <span class="detail">{{ detail ?? `${value}%` }}</span>
    </div>
    <div class="track">
      <div
        class="fill"
        :class="{ warn: value >= 80, crit: value >= 95 }"
        :style="{ width: `${Math.min(value, 100)}%` }"
      />
    </div>
  </div>
</template>

<style scoped>
.usage {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.usage-head {
  display: flex;
  justify-content: space-between;
  font-size: 0.85em;
}

.detail {
  opacity: 0.7;
}

.track {
  height: 6px;
  border-radius: 3px;
  background: #8883;
  overflow: hidden;
}

.fill {
  height: 100%;
  background: #2a7;
  border-radius: 3px;
}

.fill.warn {
  background: #e5a000;
}

.fill.crit {
  background: #c33;
}
</style>
