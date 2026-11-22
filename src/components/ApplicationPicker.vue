<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { InstalledApplication } from '../types/hisense';
const props=defineProps<{open:boolean}>();
const emit=defineEmits<{close:[];select:[app:InstalledApplication]}>();
const items=ref<InstalledApplication[]>([]),search=ref(''),busy=ref(false),error=ref('');
const sort=ref('name'),limit=ref(100);
const list = ref<HTMLElement>();
const searchInput = ref<HTMLInputElement>();
const icons = ref(new Map<string, string | null>());
const visiblePaths = new Set<string>();
let observer: IntersectionObserver | undefined;
let fetchingIcons = false;
let scanRevision = 0;
let previousFocus: HTMLElement | null = null;

async function loadVisibleIcons() {
  if (!props.open || fetchingIcons || !visiblePaths.size) return;
  const paths = [...visiblePaths].filter(path => !icons.value.has(path)).slice(0, 16);
  if (!paths.length) return;
  fetchingIcons = true;
  const revision = scanRevision;
  try {
    const result = await invoke<Record<string, string>>("hisense_application_icons", { paths });
    if (revision === scanRevision) {
      for (const path of paths) {
        const image = result[path];
        icons.value.set(path, image ? "data:image/png;base64," + image : null);
      }
    }
  } catch {
    if (revision === scanRevision) for (const path of paths) icons.value.set(path, null);
  } finally {
    fetchingIcons = false;
    void loadVisibleIcons();
  }
}

function initial(app: InstalledApplication) {
  const name = app.name.trim() || app.process_name;
  return Array.from(name)[0]?.toLocaleUpperCase() || "·";
}
watch([search,sort],()=>{limit.value=100;});
const rows=computed(()=>items.value.filter(a=>`${a.name} ${a.process_name} ${a.path} ${a.source}`.toLowerCase().includes(search.value.trim().toLowerCase())).sort((a,b)=>(sort.value==='source'?a.source.localeCompare(b.source,'zh-CN'):0)||a.name.localeCompare(b.name,'zh-CN',{numeric:true,sensitivity:'base'})||a.path.localeCompare(b.path)));
const displayed = computed(() => rows.value.slice(0, limit.value));

async function scan(refresh = false) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    items.value = await invoke("hisense_scan_applications", { refresh });
    if (refresh) {
      scanRevision++;
      icons.value.clear();
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

watch([() => props.open, displayed], async () => {
  observer?.disconnect();
  observer = undefined;
  visiblePaths.clear();
  await nextTick();
  if (!props.open || !list.value) return;
  const root = list.value;
  const nextObserver = new IntersectionObserver(entries => {
    if (observer !== nextObserver) return;
    for (const entry of entries) {
      const path = (entry.target as HTMLElement).dataset.appPath;
      if (!path) continue;
      if (entry.isIntersecting) visiblePaths.add(path);
      else visiblePaths.delete(path);
    }
    void loadVisibleIcons();
  }, { root, rootMargin: "110px 0px" });
  observer = nextObserver;
  root.querySelectorAll("[data-app-path]").forEach(row => nextObserver.observe(row));
}, { flush: "post" });

watch(() => props.open, async open => {
  if (!open) {
    previousFocus?.focus();
    return;
  }
  previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  search.value = "";
  limit.value = 100;
  void scan();
  await nextTick();
  searchInput.value?.focus();
});

onUnmounted(() => {
  observer?.disconnect();
  visiblePaths.clear();
  scanRevision++;
});
</script>
<template>
  <div v-if="open" class="app-picker-mask" @click.self="emit('close')" @keydown.esc.stop="emit('close')"><section role="dialog" aria-modal="true" aria-label="选择本机应用" class="app-picker">
    <header><div><h2>选择本机应用</h2><p>自动收集开始菜单、已安装应用、商店应用、Steam 游戏及运行中的程序。</p></div><button aria-label="关闭应用选择" @click="emit('close')">×</button></header>
    <div class="picker-tools"><input ref="searchInput" v-model="search" aria-label="搜索本机应用" placeholder="搜索名称、进程或路径…" /><select v-model="sort" aria-label="应用排序"><option value="name">按名称排序</option><option value="source">按来源排序</option></select><button :disabled="busy" @click="scan(true)">{{ busy?'扫描中…':'重新扫描' }}</button></div>
    <p v-if="error" class="picker-note">{{ error }}</p><p v-else class="picker-note">{{ busy?'正在扫描，首次可能需要数十秒…':`${rows.length} 个应用 · 点击选择` }}　绿色便携软件若未出现在列表中，可用“浏览程序”补充。</p>
    <div ref="list" class="picker-list">
      <button v-for="app in displayed" :key="app.path" :data-app-path="app.path" class="picker-row"
        @click="emit('select', app); emit('close')">
        <span class="app-icon" aria-hidden="true">
          <img v-if="icons.get(app.path)" :src="icons.get(app.path) || undefined" alt="" @error="icons.set(app.path, null)">
          <span v-else>{{ initial(app) }}</span>
        </span>
        <span class="app-copy">
          <strong>{{ app.name.trim() || app.process_name }}</strong>
          <span>{{ app.process_name }} · {{ app.source }}</span>
          <small :title="app.path">{{ app.path }}</small>
        </span>
      </button>
      <button v-if="rows.length > limit" class="load-more" @click="limit += 100">显示更多（剩余 {{ rows.length - limit }} 个）</button>
      <p v-if="!busy && !rows.length">没有匹配的应用，可刷新扫描或手动浏览。</p>
    </div>
  </section></div>
</template>
<style scoped>
.app-picker-mask{position:fixed;inset:0;z-index:4200;display:grid;place-items:center;background:var(--overlay);padding:24px}.app-picker{width:min(960px,94vw);max-height:86vh;display:flex;flex-direction:column;background:var(--card-bg);color:var(--text);border:1px solid var(--border);border-radius:16px;box-shadow:var(--dialog-shadow);overflow:hidden}header{padding:20px 24px;display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid var(--border)}h2{margin:0;font-size:20px}p,small,span{color:var(--text-secondary);font-size:12px}header p{margin-bottom:0}.picker-tools{display:flex;gap:10px;padding:20px 24px 0}.picker-tools input{flex:1;min-width:0}.picker-note{padding:0 24px;line-height:1.7}.picker-list{padding:0 24px 20px;overflow:auto;min-height:0}.picker-row{display:grid;grid-template-columns:44px minmax(0,1fr);align-items:center;width:100%;text-align:left;gap:13px;border-radius:9px;padding:10px;margin:3px 0;background:var(--card-bg)}.app-icon{display:grid;place-items:center;width:42px;height:42px;border-radius:10px;background:var(--surface-soft);color:var(--primary);font-size:18px;font-weight:700}.app-icon img{display:block;width:30px;height:30px;object-fit:contain}.app-icon span{font:inherit;color:inherit}.app-copy{display:grid;gap:3px;min-width:0}.app-copy strong{color:var(--text);font-size:13px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.app-copy small{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.load-more{display:block;margin:12px auto}button,input,select{font:inherit;font-size:12px;color:var(--text);background:var(--card-bg);border:1px solid var(--border);border-radius:8px;padding:9px 12px}button{cursor:pointer}button:hover{background:var(--surface-soft);color:var(--primary)}button:disabled{opacity:.6}input:focus-visible,button:focus-visible,select:focus-visible{outline:2px solid var(--primary)}
</style>
