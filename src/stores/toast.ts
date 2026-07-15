import { ref } from "vue";

export interface Toast {
  id: number;
  text: string;
  kind: "ok" | "error";
}

export const toasts = ref<Toast[]>([]);
let nextId = 1;

export function toast(text: string, kind: Toast["kind"] = "ok") {
  const id = nextId++;
  toasts.value = [...toasts.value, { id, text, kind }];
  setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }, 4000);
}
