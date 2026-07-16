import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import "./style.css";

// Stamp thead labels onto cells so CSS can collapse tables to cards on
// small viewports (td::before { content: attr(data-label) }).
function labelCells(el: HTMLElement) {
  el.setAttribute("data-cards", "");
  const heads = [...el.querySelectorAll("thead th")].map((th) => th.textContent?.trim() ?? "");
  el.querySelectorAll("tbody tr").forEach((tr) => {
    [...tr.children].forEach((td, i) => td.setAttribute("data-label", heads[i] ?? ""));
  });
}

createApp(App)
  .use(router)
  .directive("cards", { mounted: labelCells, updated: labelCells })
  .mount("#app");
