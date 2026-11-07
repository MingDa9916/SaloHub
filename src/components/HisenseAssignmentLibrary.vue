<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ASSIGNMENT_COMMANDS, KEY_CATALOG, type Command } from "../utils/hisenseAssignments";
import { keyLabel } from "../utils/shortcut";
import VoiceShortcutComposer from "./VoiceShortcutComposer.vue";
import type { HisenseAction, HisenseMacro, HisenseProfile, MacroMode, MacroStep } from "../types/hisense";
type Tab = "commands" | "keys" | "macros";
const props = defineProps<{ open:boolean; initialTab:Tab; initialKeys?:number[]; macros:Record<string,HisenseMacro>; profiles:Record<string,HisenseProfile>; buttonLabel:string; slotLabel:string; baseSlot:boolean; allowHeld:boolean; disabled:boolean }>();
const emit=defineEmits<{close:[];assign:[action:HisenseAction];macrosChange:[macros:Record<string,HisenseMacro>]} >();
const tab=ref<Tab>("commands"),search=ref(""),category=ref("全部"),root=ref<HTMLElement|null>(null);
const keyMode = ref<"single"|"capture"|"manual">("single"), shortcutRecording = ref(false);
let shortcutSession: number | null = null, shortcutPoller: ReturnType<typeof setInterval> | null = null;
let shortcutStarting = false;
async function stopShortcutCapture() {
  if (shortcutPoller) clearInterval(shortcutPoller);
  shortcutPoller = null;
  const id = shortcutSession; shortcutSession = null; shortcutRecording.value = false;
  if (id !== null) await invoke("capture_shortcut_stop", { sessionId: id }).catch(() => {});
}
async function captureShortcut() {
  if (shortcutStarting || shortcutRecording.value || props.disabled) return;
  shortcutStarting = true; shortcutRecording.value = true; message.value = "";
  try {
    const id = await invoke<number>("capture_shortcut_start");
    if (disposed || !props.open || tab.value !== "keys" || keyMode.value !== "capture" || !shortcutRecording.value) {
      await invoke("capture_shortcut_stop", { sessionId: id }); return;
    }
    shortcutSession = id;
    let pollingShortcut = false;
    shortcutPoller = setInterval(async () => {
      if (pollingShortcut || shortcutSession !== id) return;
      pollingShortcut = true;
      try {
        const result = await invoke<{ sessionId:number; keys:number[] } | null>("capture_shortcut_poll", { sessionId: id });
        if (shortcutSession !== id || !result?.keys.length || result.sessionId !== id) return;
        await stopShortcutCapture();
        if (!disposed && props.open && tab.value === "keys" && keyMode.value === "capture") assignKeys(result.keys);
      } catch (e) { message.value = String(e); await stopShortcutCapture(); }
      finally { pollingShortcut = false; }
    }, 70);
  } catch (e) { message.value = String(e); await stopShortcutCapture(); }
  finally { shortcutStarting = false; }
}
function assignKeys(keys:number[]) { if(keys.length) assign(keys.length===1?{type:"SingleKey",value:keys[0]}:{type:"ComboKey",value:keys}); }
async function selectKeyMode(mode: "single"|"capture"|"manual") { await stopShortcutCapture(); keyMode.value = mode; message.value = ""; }
const draft=ref<HisenseMacro|null>(null),editingId=ref<string|null>(null),segment=ref<"press"|"hold"|"release">("press");
const message=ref(""),deleteRequested=ref(false),recording=ref(false),keyToAdd=ref(65),edgeToAdd=ref("tap"),mouseToAdd=ref(false),delayToAdd=ref(50);
let session:number|null=null,poller:ReturnType<typeof setInterval>|null=null,focused:HTMLElement|null=null,disposed=false,polling=false;
const modes:Array<{id:MacroMode;icon:string;name:string;hint:string}>=[
  {id:"Once",icon:"→",name:"不重复",hint:"每次触发执行一次，松开后继续完成。"},
  {id:"Hold",icon:"↻",name:"按住时重复",hint:"按住时循环，松开立即停止并释放按键。"},
  {id:"Toggle",icon:"⏻",name:"触发",hint:"按一次循环执行，再按一次停止。"},
  {id:"Sequence",icon:"▣",name:"序列",hint:"分别设置按下、按住循环和松开三个阶段。"},
];
const allCommands=computed<Command[]>(()=>[...ASSIGNMENT_COMMANDS,...Object.entries(props.profiles).filter(([,p])=>p.enabled).map(([id,p])=>({category:"配置文件",name:`切换到 ${p.name}`,action:{type:"SelectProfile",value:id} as HisenseAction}))]);
const categories=computed(()=>["全部",...new Set((tab.value==="commands"?allCommands.value:KEY_CATALOG).map(item=>item.category))]);
const commands=computed(()=>allCommands.value.filter(c=>(category.value==="全部"||c.category===category.value)&&`${c.name} ${(c as Command).keys?.map(keyLabel).join(" ")||""}`.toLowerCase().includes(search.value.toLowerCase())));
const keys=computed(()=>KEY_CATALOG.filter(k=>(category.value==="全部"||k.category===category.value)&&`${k.label} ${k.category}`.toLowerCase().includes(search.value.toLowerCase())));
const macros=computed(()=>Object.entries(props.macros).filter(([,m])=>m.name.toLowerCase().includes(search.value.toLowerCase())));
const steps=computed(()=>draft.value?.[segment.value]||[]);
const keyGroups=[...new Set(KEY_CATALOG.map(k=>k.category))];
function selectTab(value:Tab){void stopShortcutCapture();keyMode.value="single";tab.value=value;category.value="全部";search.value="";draft.value=null;message.value="";}
function commandDisabled(action:HisenseAction){return props.disabled||(!props.allowHeld&&["PointerAi","VoiceInput"].includes(action.type))||(!props.baseSlot&&action.type==="PointerAi");}
function assign(action:HisenseAction){if(commandDisabled(action))return;emit("assign",action);emit("close");}
function create(){editingId.value=null;segment.value="press";message.value="";draft.value={name:"新宏",mode:"Once",color:"#3478f6",use_default_delay:true,default_delay_ms:50,show_key_transitions:true,press:[],hold:[],release:[]};}
function edit(id:string){editingId.value=id;segment.value="press";deleteRequested.value=false;draft.value=JSON.parse(JSON.stringify(props.macros[id]));}
function useMacro(id:string){const m=props.macros[id];if(!props.allowHeld&&["Hold","Sequence"].includes(m.mode))return;assign({type:"Macro",value:id});}
function save(bind:boolean){
  if(!draft.value||recording.value)return;
  const d=draft.value;
  if(!d.name.trim()||!((d.mode==="Sequence"?[...d.press,...d.hold,...d.release]:d.press).length)){message.value="请填写名称并添加至少一个步骤。";return;}
  if([d.press,d.hold,d.release].some(s=>s.length>256)){message.value="每个阶段最多 256 步。";return;}
  const id=editingId.value||crypto.randomUUID();const saved={...JSON.parse(JSON.stringify(d)),name:d.name.trim()} as HisenseMacro;
  emit("macrosChange",{...props.macros,[id]:saved});editingId.value=id;message.value="宏已更新，配置会自动保存。";
  if(bind){emit("assign",{type:"Macro",value:id});emit("close");}else draft.value=null;
}
function removeMacro(){if(!editingId.value)return;if(!deleteRequested.value){deleteRequested.value=true;return;}const next={...props.macros};delete next[editingId.value];emit("macrosChange",next);draft.value=null;deleteRequested.value=false;}
function addKey(){if(!draft.value||steps.value.length>252)return;const key=Number(keyToAdd.value);if(edgeToAdd.value!=="up")steps.value.push({type:"KeyDown",value:key});if(edgeToAdd.value!=="down")steps.value.push({type:"KeyUp",value:key});}
function addMouse(){if(steps.value.length>252)return;steps.value.push({type:"MouseDown",value:mouseToAdd.value},{type:"MouseUp",value:mouseToAdd.value});}
function move(index:number,delta:number){const other=index+delta;if(other<0||other>=steps.value.length)return;[steps.value[index],steps.value[other]]=[steps.value[other],steps.value[index]];}
function title(step:MacroStep){if(step.type==="Delay")return `延迟 ${step.value} ms`;if(step.type==="MouseDown"||step.type==="MouseUp")return `${step.value?"右":"左"}键${draft.value?.show_key_transitions?(step.type==="MouseDown"?"按下":"抬起"):""}`;return `${keyLabel(Number(step.value))}${draft.value?.show_key_transitions?(step.type==="KeyDown"?" ↓ 按下":" ↑ 抬起"):""}`;}
async function poll(stop=false){if(session==null||polling)return;polling=true;try{const result=await invoke<{active:boolean;steps:MacroStep[]}>("hisense_macro_record_poll",{session,stop});if(draft.value)draft.value[segment.value]=result.steps;recording.value=result.active;if(!result.active){if(poller)clearInterval(poller);poller=null;session=null;}}catch(e){message.value=String(e);}finally{polling=false;}}
async function startRecording(){if(!draft.value)return;message.value="";try{session=await invoke<number>("hisense_macro_record_start");if(disposed){await invoke("hisense_macro_record_poll",{session,stop:true});return;}draft.value[segment.value]=[];recording.value=true;poller=setInterval(()=>{void poll();},200);}catch(e){message.value=String(e);}}
async function stopRecording(){if(poller)clearInterval(poller);poller=null;while(polling)await new Promise(resolve=>setTimeout(resolve,10));await poll(true);recording.value=false;}
async function close(){await stopShortcutCapture();await stopRecording();emit("close");}
async function stopMacros(){await invoke("hisense_stop_macros");message.value="已停止所有运行中的宏。";}
function keyboard(e:KeyboardEvent){if(e.key==="Escape"&&!recording.value){e.preventDefault();void close();}if(e.key==="Tab"&&root.value){const items=[...root.value.querySelectorAll<HTMLElement>('button:not([disabled]),input:not([disabled]),select:not([disabled]),[tabindex="0"]')];const first=items[0],last=items[items.length-1];if(e.shiftKey&&(document.activeElement===first||document.activeElement===root.value)){e.preventDefault();last?.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}}}
watch(()=>props.open,async open=>{if(open){focused=document.activeElement as HTMLElement;selectTab(props.initialTab);await nextTick();root.value?.focus();}else{await stopShortcutCapture();await stopRecording();focused?.focus();} });
onUnmounted(()=>{disposed=true;void stopShortcutCapture();if(poller)clearInterval(poller);if(session!=null)void invoke("hisense_macro_record_poll",{session,stop:true});});
</script>

<template>
  <div v-if="open" class="assignment-backdrop" @click.self="close">
    <section ref="root" role="dialog" aria-modal="true" aria-labelledby="assignment-title" tabindex="-1" class="assignment-dialog" @keydown="keyboard">
      <header><div><h2 id="assignment-title">分配功能</h2><p>{{ buttonLabel }} · {{ slotLabel }}</p></div><nav aria-label="分配功能分类"><button v-for="[id,label] in [['commands','命令'],['keys','按键'],['macros','宏']]" :key="id" :disabled="recording||shortcutRecording" :class="{active:tab===id}" @click="selectTab(id as Tab)">{{ label }}</button></nav><button class="close" aria-label="关闭功能库" @click="close">×</button></header>
      <p v-if="message" class="library-message" role="status">{{ message }}</p>
      <div v-if="!draft" class="library-body">
        <aside><input v-model="search" aria-label="搜索分配功能" :placeholder="tab==='commands'?'搜索命令':tab==='keys'?'搜索按键':'搜索宏'" /><template v-if="tab!=='macros'"><button v-for="c in categories" :key="c" :class="{active:category===c}" @click="category=c">{{ c }}</button></template><template v-else><button class="primary" @click="create">＋ 创建新宏</button><button @click="stopMacros">停止所有宏</button><p>宏可以重复使用，分配到不同遥控按键或应用配置。</p></template></aside>
        <main>
          <template v-if="tab==='commands'"><button v-for="c in commands" :key="c.category+c.name" class="catalog-item" :disabled="commandDisabled(c.action)" @click="assign(c.action)"><span><strong>{{ c.name }}</strong><small>{{ c.category }}</small></span><kbd>{{ c.keys?.map(keyLabel).join(' + ')||'遥控器功能' }}</kbd><b>＋</b></button><p v-if="!commands.length">没有找到匹配命令。</p></template>
          <section v-else-if="tab==='keys'" class="key-assignment">
            <div class="key-mode-tabs" role="group" aria-label="按键设置方式">
              <button :class="{active:keyMode==='single'}" :disabled="shortcutRecording" @click="selectKeyMode('single')">选择单键</button>
              <button :class="{active:keyMode==='capture'}" :disabled="shortcutRecording" @click="selectKeyMode('capture')">录入快捷键</button>
              <button :class="{active:keyMode==='manual'}" :disabled="shortcutRecording" @click="selectKeyMode('manual')">手动组合</button>
            </div>
            <div v-if="keyMode==='single'" class="key-catalog"><button v-for="key in keys" :key="key.value" :disabled="disabled" @click="assign({type:'SingleKey',value:key.value})"><kbd>{{ key.label }}</kbd><small>{{ key.category }}</small></button><p v-if="!keys.length">没有找到匹配按键。</p></div>
            <section v-else-if="keyMode==='capture'" class="shortcut-capture-panel" aria-label="录入电脑快捷键">
              <h3>录入快捷键</h3><p class="hint">点击开始后，在电脑键盘上按目标键或组合键，捕获成功后会自动分配。此处录入一组快捷键；多步操作请使用宏。</p>
              <p v-if="shortcutRecording" role="status">正在等待键盘输入…</p>
              <button v-if="shortcutRecording" @click="stopShortcutCapture">取消录入</button><button v-else class="primary" :disabled="disabled||shortcutStarting" @click="captureShortcut">开始录入</button>
            </section>
            <VoiceShortcutComposer v-else :initial-keys="initialKeys||[]" :button-label="buttonLabel" :slot-label="slotLabel" @apply="assignKeys" @cancel="selectKeyMode('single')" />
          </section>
          <template v-else><section v-for="[id,m] in macros" :key="id" class="macro-item"><i :style="{background:m.color}" /><span><strong>{{ m.name }}</strong><small>{{ modes.find(mode=>mode.id===m.mode)?.name }} · {{ m.press.length+m.hold.length+m.release.length }} 步</small></span><button @click="edit(id)">编辑</button><button class="primary" :disabled="disabled||(!allowHeld&&['Hold','Sequence'].includes(m.mode))" @click="useMacro(id)">分配</button></section><p v-if="!macros.length" class="empty">还没有宏，点击“创建新宏”开始。</p><p v-if="!allowHeld" class="hint">连击可分配“不重复”和“触发”宏；按住重复与序列请分配到单击或长按。</p></template>
        </main>
      </div>
      <div v-else class="macro-editor">
        <div class="macro-toolbar"><label>宏名称<input v-model="draft.name" maxlength="80" aria-label="宏名称" /></label><label>宏颜色<input v-model="draft.color" type="color" aria-label="宏颜色" /></label><button :disabled="recording" @click="draft=null">返回宏列表</button></div>
        <div class="mode-options"><button v-for="mode in modes" :key="mode.id" :disabled="recording" :class="{active:draft.mode===mode.id}" :title="mode.hint" @click="draft.mode=mode.id;segment='press'"><b>{{ mode.icon }}</b>{{ mode.name }}</button></div>
        <p class="hint">{{ modes.find(mode=>mode.id===draft?.mode)?.hint }}</p>
        <div class="macro-options"><label><input v-model="draft.use_default_delay" type="checkbox" />使用标准延迟</label><label><input v-model.number="draft.default_delay_ms" aria-label="宏标准延迟" type="number" min="0" max="10000" :disabled="!draft.use_default_delay" /> ms</label><label><input v-model="draft.show_key_transitions" type="checkbox" />显示按键按下／抬起</label></div>
        <p v-if="draft.use_default_delay" class="hint">操作之间使用标准延迟；列表中的单独延迟在关闭此选项后生效。</p>
        <div v-if="draft.mode==='Sequence'" class="segment-tabs"><button v-for="[id,label] in [['press','按下时'],['hold','按住时循环'],['release','松开时']]" :key="id" :disabled="recording" :class="{active:segment===id}" @click="segment=id as 'press'|'hold'|'release'">{{ label }}</button></div>
        <div class="step-tools"><button :class="{recording}" @click="recording?stopRecording():startRecording()">{{ recording?'■ 停止录制':'● 录制键盘（替换本段）' }}</button><select v-model.number="keyToAdd" aria-label="宏步骤按键" :disabled="recording"><optgroup v-for="group in keyGroups" :key="group" :label="group"><option v-for="key in KEY_CATALOG.filter(k=>k.category===group)" :key="key.value" :value="key.value">{{ key.label }}</option></optgroup></select><select v-model="edgeToAdd" aria-label="宏按键方式" :disabled="recording"><option value="tap">点按</option><option value="down">按下</option><option value="up">抬起</option></select><button :disabled="recording||steps.length>252" @click="addKey">添加按键</button><select v-model="mouseToAdd" aria-label="宏鼠标按键" :disabled="recording"><option :value="false">鼠标左键</option><option :value="true">鼠标右键</option></select><button :disabled="recording||steps.length>252" @click="addMouse">添加鼠标点按</button><input v-model.number="delayToAdd" aria-label="宏延迟步长" type="number" min="0" max="60000" :disabled="recording" /><button :disabled="recording||steps.length>=256" @click="steps.push({type:'Delay',value:Math.max(0,Math.min(60000,delayToAdd))})">添加延迟</button></div>
        <p v-if="recording" class="library-message">正在录制键盘，键盘事件仅用于录制；请点击“停止录制”结束。最长 120 秒，每段最多 256 步。</p>
        <ol class="step-list"><li v-for="(step,index) in steps" :key="index"><span>{{ index+1 }}</span><strong>{{ title(step) }}</strong><input v-if="step.type==='Delay'" v-model.number="step.value" type="number" min="0" max="60000" :disabled="recording" :aria-label="`第${index+1}步延迟`" /><button :disabled="recording||index===0" aria-label="上移步骤" @click="move(index,-1)">↑</button><button :disabled="recording||index===steps.length-1" aria-label="下移步骤" @click="move(index,1)">↓</button><button :disabled="recording" aria-label="删除步骤" @click="steps.splice(index,1)">×</button></li><li v-if="!steps.length" class="empty">录制键盘，或在上方添加按键、鼠标点按与延迟。</li></ol>
        <footer><button v-if="editingId" :disabled="recording" class="danger" @click="removeMacro">{{ deleteRequested?'确认删除宏及其分配':'删除宏' }}</button><button :disabled="recording" @click="save(false)">保存宏</button><button class="primary" :disabled="recording||disabled||(!allowHeld&&['Hold','Sequence'].includes(draft.mode))" @click="save(true)">保存并分配</button></footer>
      </div>
    </section>
  </div>
</template>

<style scoped>
.key-mode-tabs{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;margin-bottom:18px}.key-mode-tabs button{min-height:38px}.shortcut-capture-panel{padding:22px;border:1px solid var(--border);border-radius:12px;background:var(--surface-soft)}.shortcut-capture-panel h3{margin-top:0}.shortcut-capture-panel p{font-size:12px;line-height:1.7}.assignment-backdrop{position:fixed;inset:0;z-index:3500;background:var(--overlay);display:grid;place-items:center;padding:24px}.assignment-dialog{width:min(1000px,calc(100vw - 48px));max-height:calc(100dvh - 48px);height: min(760px,calc(100dvh - 48px));background:var(--card-bg);border:1px solid var(--border);border-radius:16px;box-shadow:var(--dialog-shadow);display:flex;flex-direction:column;overflow:hidden;color:var(--text)}header{display:flex;align-items:center;gap:24px;padding:20px 24px;border-bottom:1px solid var(--border)}h2{font-size:19px;margin:0}header p{font-size:12px;color:var(--text-secondary);margin:6px 0 0}nav{display:flex;gap:5px;margin:auto;border-radius:999px;background:var(--surface-soft);padding:4px}.close{font-size:25px!important;border:0!important}button,input,select{font:inherit;color:var(--text);background:var(--card-bg);border:1px solid var(--border);border-radius:8px;padding:8px 11px;font-size:12px;box-sizing:border-box}button{cursor:pointer}button:disabled{opacity:.45;cursor:default}button:hover:not(:disabled),button.active{border-color:var(--primary);color:var(--primary);background:color-mix(in srgb,var(--primary) 7%,var(--card-bg))}.primary{background:var(--primary);border-color:var(--primary);color:white}.primary:hover:not(:disabled){color:white;background:var(--primary)}nav button{border:0;border-radius:999px;min-width:70px}nav button.active{background:var(--primary);color:white}.library-body{display:grid;grid-template-columns:210px minmax(0,1fr);flex:1;min-height:0}.library-body aside{padding:20px;border-right:1px solid var(--border);display:flex;flex-direction:column;gap:8px;overflow:auto}.library-body aside input{width:100%;margin-bottom:12px}.library-body aside button{text-align:left}.library-body aside p,.hint,small{color:var(--text-secondary);font-size:12px;line-height:1.65}.library-body main{padding:20px;overflow:auto;min-height:0}.catalog-item{display:flex;width:100%;align-items:center;gap:18px;text-align:left;padding:13px;border:0;border-bottom:1px solid var(--border);border-radius:0}.catalog-item span{flex:1}.catalog-item small,.macro-item small{display:block;font-size:10px;margin-top:4px}kbd{font:12px ui-monospace,monospace;color:var(--text-secondary)}.catalog-item b{color:var(--primary)}.key-catalog{display:grid;grid-template-columns:repeat(auto-fill,minmax(110px,1fr));gap:9px}.key-catalog button{padding:17px 8px}.key-catalog small{display:block;font-size:10px;margin-top:8px}.macro-item{display:flex;gap:12px;align-items:center;padding:16px 0;border-bottom:1px solid var(--border)}.macro-item>i{width:11px;height:35px;border-radius:6px}.macro-item>span{flex:1}.empty{padding:32px 12px;text-align:center;color:var(--text-secondary);font-size:13px}.macro-editor{padding:22px;overflow:auto;flex:1;min-height:0}.macro-toolbar,.macro-options,.step-tools,footer,.segment-tabs{display:flex;gap:10px;align-items:center;flex-wrap:wrap}.macro-toolbar>label:first-child{flex:1}.macro-toolbar label{font-size:12px;display:flex;align-items:center;gap:10px}.macro-toolbar input[type=text],.macro-toolbar input:not([type]){flex:1}.macro-toolbar input[type=color]{width:48px;height:34px;padding:3px}.mode-options{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-top:20px}.mode-options button{display:flex;flex-direction:column;gap:10px;align-items:center;padding:18px 10px}.mode-options b{font-size:25px}.macro-options label{display:flex;gap:6px;align-items:center;font-size:12px}.macro-options input[type=number],.step-tools input{width:88px}.segment-tabs{margin:16px 0}.step-tools{padding:14px;background:var(--surface-soft);border-radius:10px;margin:18px 0}.recording{color:var(--danger);border-color:var(--danger)}.step-list{list-style:none;margin:0;padding:0;max-height:280px;overflow:auto;border:1px solid var(--border);border-radius:10px}.step-list li{display:flex;align-items:center;gap:10px;padding:8px 12px;border-bottom:1px solid var(--border);font-size:12px}.step-list li>span{color:var(--text-secondary);width:24px}.step-list strong{flex:1;font-weight:500}.step-list input{width:100px}.step-list button{padding:5px 9px}footer{justify-content:flex-end;margin-top:20px}.danger{color:var(--danger);margin-right:auto}.library-message{margin:12px 20px 0;padding:12px;background:var(--surface-soft);border-radius:9px;font-size:12px;line-height:1.6}button:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--primary);outline-offset:2px}@media(max-width:700px){.library-body{grid-template-columns:150px minmax(0,1fr)}header{gap:10px;padding:15px}nav button{min-width:45px}.catalog-item{flex-wrap:wrap}.mode-options{gap:5px}.macro-toolbar{align-items:flex-start}.macro-toolbar label{flex-wrap:wrap}.step-list{max-height:240px}}
</style>
