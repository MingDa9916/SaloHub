<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { InstalledApplication } from '../types/hisense';
const props=defineProps<{open:boolean}>();
const emit=defineEmits<{close:[];select:[app:InstalledApplication]}>();
const items=ref<InstalledApplication[]>([]),search=ref(''),busy=ref(false),error=ref('');
const sort=ref('name'),limit=ref(100);
watch([search,sort],()=>{limit.value=100;});
const rows=computed(()=>items.value.filter(a=>`${a.name} ${a.process_name} ${a.path} ${a.source}`.toLowerCase().includes(search.value.trim().toLowerCase())).sort((a,b)=>(sort.value==='source'?a.source.localeCompare(b.source,'zh-CN'):0)||a.name.localeCompare(b.name,'zh-CN',{numeric:true,sensitivity:'base'})||a.path.localeCompare(b.path)));
async function scan(refresh=false){if(busy.value)return;busy.value=true;error.value='';try{items.value=await invoke('hisense_scan_applications',{refresh});}catch(e){error.value=String(e);}finally{busy.value=false;}}
watch(()=>props.open,open=>{if(open){search.value='';void scan();}});
</script>
<template>
  <div v-if="open" class="app-picker-mask" @click.self="emit('close')" @keydown.esc.stop="emit('close')"><section role="dialog" aria-modal="true" aria-label="选择本机应用" class="app-picker">
    <header><div><h2>选择本机应用</h2><p>自动收集开始菜单、已安装应用、商店应用、Steam 游戏及运行中的程序。</p></div><button aria-label="关闭应用选择" @click="emit('close')">×</button></header>
    <div class="picker-tools"><input v-model="search" autofocus aria-label="搜索本机应用" placeholder="搜索名称、进程或路径…" /><select v-model="sort" aria-label="应用排序"><option value="name">按名称排序</option><option value="source">按来源排序</option></select><button :disabled="busy" @click="scan(true)">{{ busy?'扫描中…':'重新扫描' }}</button></div>
    <p v-if="error" class="picker-note">{{ error }}</p><p v-else class="picker-note">{{ busy?'正在扫描，首次可能需要数十秒…':`${rows.length} 个应用 · 点击选择` }}　绿色便携软件若未出现在列表中，可用“浏览程序”补充。</p>
    <div class="picker-list"><button v-for="app in rows.slice(0,limit)" :key="app.path" @click="emit('select',app);emit('close')"><b>{{ app.name }}</b><span>{{ app.process_name }} · {{ app.source }}</span><small>{{ app.path }}</small></button><button v-if="rows.length>limit" @click="limit+=100">显示更多（剩余 {{ rows.length-limit }} 个）</button><p v-if="!busy&&!rows.length">没有匹配的应用，可刷新扫描或手动浏览。</p></div>
  </section></div>
</template>
<style scoped>
.app-picker-mask{position:fixed;inset:0;z-index:4200;display:grid;place-items:center;background:var(--overlay);padding:24px}.app-picker{width:min(960px,94vw);max-height:86vh;display:flex;flex-direction:column;background:var(--card-bg);color:var(--text);border:1px solid var(--border);border-radius:16px;box-shadow:var(--dialog-shadow);overflow:hidden}header{padding:20px 24px;display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid var(--border)}h2{margin:0;font-size:20px}p,small,span{color:var(--text-secondary);font-size:12px}header p{margin-bottom:0}.picker-tools{display:flex;gap:10px;padding:20px 24px 0}.picker-tools input{flex:1;min-width:0}.picker-note{padding:0 24px;line-height:1.7}.picker-list{padding:0 24px 20px;overflow:auto;min-height:0}.picker-list>button{display:grid;width:100%;text-align:left;gap:5px;border-radius:0;border-width:0 0 1px;padding:13px 8px}.picker-list small{overflow-wrap:anywhere}button,input,select{font:inherit;font-size:12px;color:var(--text);background:var(--card-bg);border:1px solid var(--border);border-radius:8px;padding:9px 12px}button{cursor:pointer}button:hover{background:var(--surface-soft);color:var(--primary)}button:disabled{opacity:.6}input:focus-visible,button:focus-visible,select:focus-visible{outline:2px solid var(--primary)}
</style>
