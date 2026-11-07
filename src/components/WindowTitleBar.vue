<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";

const win = getCurrentWindow();
const maximized = ref(false), focused = ref(true);
let disposed = false;
const listeners: UnlistenFn[] = [];
async function syncMaximized() { maximized.value = await win.isMaximized(); }
async function control(action: "close" | "minimize" | "maximize") {
  try {
    if (action === "close") await win.close();
    else if (action === "minimize") await win.minimize();
    else { await win.toggleMaximize(); await syncMaximized(); }
  } catch (error) { console.error("Window control failed:", error); }
}
function drag(event: MouseEvent) {
  if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
  event.preventDefault();
  if (event.detail === 2) void control("maximize");
  else void win.startDragging().catch(error => console.error("Window drag failed:", error));
}
onMounted(async () => {
  try {
    await syncMaximized();
    focused.value = await win.isFocused();
    for (const subscribe of [
      () => win.onResized(() => { void syncMaximized(); }),
      () => win.onFocusChanged(event => { focused.value = event.payload; }),
    ]) {
      const stop = await subscribe();
      if (disposed) stop(); else listeners.push(stop);
    }
  } catch { /* Browser previews have no native window. */ }
});
onUnmounted(() => { disposed = true; listeners.forEach(stop => stop()); });
</script>

<template>
  <header class="window-titlebar" :class="{inactive:!focused}" aria-label="窗口标题栏" @mousedown="drag">
    <div class="window-controls" @mousedown.stop @dblclick.stop>
      <button class="traffic close" aria-label="关闭窗口" title="关闭窗口" @click="control('close')"><span><svg viewBox="0 0 12 12" aria-hidden="true"><path d="m3.5 3.5 5 5m0-5-5 5" /></svg></span></button>
      <button class="traffic minimize" aria-label="最小化窗口" title="最小化" @click="control('minimize')"><span><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 6h6" /></svg></span></button>
      <button class="traffic maximize" :aria-label="maximized?'还原窗口':'最大化窗口'" :title="maximized?'还原':'最大化'" @click="control('maximize')"><span><svg viewBox="0 0 12 12" aria-hidden="true"><path v-if="maximized" d="M3 5h2V3m4 4H7v2" /><path v-else d="M3 7v2h2m2-6h2v2" /></svg></span></button>
    </div>
    <span class="window-title">SaloHUb</span>
    <div class="titlebar-balance" aria-hidden="true" />
  </header>
</template>

<style scoped>
.window-titlebar{display:grid;grid-template-columns:100px minmax(0,1fr) 100px;align-items:center;height:36px;min-height:36px;flex-shrink:0;padding:0 12px;background:linear-gradient(180deg,#f6f6f8,#ededf0);border-bottom:1px solid #0000000c;color:#42434a;user-select:none;-webkit-user-select:none}
.window-controls{display:flex;align-items:center;gap:2px;width:max-content}.traffic{display:grid;place-items:center;width:24px;height:28px;padding:0;border:0;background:transparent;cursor:default;border-radius:6px}.traffic>span{display:grid;place-items:center;width:12px;height:12px;box-sizing:border-box;border-radius:50%;border:1px solid #0000001a;transition:background .15s}.close>span{background:#ff5f57}.minimize>span{background:#febc2e}.maximize>span{background:#28c840}.traffic svg{width:12px;height:12px;fill:none;stroke:#000000a6;stroke-width:1.3;stroke-linecap:round;stroke-linejoin:round;opacity:0}.window-controls:hover svg,.traffic:focus-visible svg{opacity:1}.traffic:focus-visible{outline:2px solid var(--primary);outline-offset:0}.traffic:active>span{filter:brightness(.88)}.window-title{text-align:center;font-size:12px;font-weight:600;letter-spacing:.15px;pointer-events:none;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.inactive .traffic:not(:hover)>span{background:#c9c9ce}.inactive .window-title{opacity:.6}
:global(:root[data-theme="dark"]) .window-titlebar{background:linear-gradient(180deg,#292a2f,#222328);border-bottom-color:#ffffff09;color:#d7d7dc}:global(:root[data-theme="dark"]) .inactive .traffic:not(:hover)>span{background:#5a5b60}
</style>
