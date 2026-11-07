<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openFile, save as saveFile } from "@tauri-apps/plugin-dialog";
import HisenseAssignmentLibrary from "../components/HisenseAssignmentLibrary.vue";
import HisenseProfileManager from "../components/HisenseProfileManager.vue";
import InputMethodSettingsDialog from "../components/InputMethodSettingsDialog.vue";
import VoiceShortcutComposer from "../components/VoiceShortcutComposer.vue";
import { useHisenseStore } from "../stores/hisense";
import { HISENSE_BUTTONS, type HisenseAction, type HisenseConfig, type HisenseMacro, type HisenseProfile } from "../types/hisense";
import type { AudioDevice } from "../types";
import { keyLabel } from "../utils/shortcut";
import HisenseRemotePreview from "../components/HisenseRemotePreview.vue";
import remotePhoto from "../assets/hisense-remote.png";
import { IME_PRESETS, type ImePreset } from "../utils/imePreset";

const bridge = useHisenseStore();
const config = ref<HisenseConfig | null>(null);
const ready = ref(false), busy = ref(false), saveState = ref("saved"), message = ref("");
const audio = ref<AudioDevice[]>([]), selected = ref("ok");
type Trigger = 1 | 2 | 3 | 4 | "long";
const editingProfile = ref("default"), profileManagerOpen = ref(false);
const libraryTab = ref<"commands"|"keys"|"macros"|null>(null);
const libraryTarget = ref<{id:string;slot:Trigger}>({id:"ok",slot:1});
const editingProfileName = computed(()=>editingProfile.value==="default"?"桌面默认":config.value?.profiles?.[editingProfile.value]?.name||"桌面默认");
const selectedTrigger = ref<Trigger>(1), manualShortcut = ref(false), transferBusy = ref(false), resetRequested = ref(false);
const triggers: Array<[Trigger, string]> = [[1, "单击"], [2, "双击"], [3, "三击"], [4, "四连击"], ["long", "长按"]];
const triggerLabel = computed(() => triggers.find(([id]) => id === selectedTrigger.value)![1]);
const realtime = (action: HisenseAction) => ["PointerConfirm", "PointerAi", "TouchDrag", "VoiceInput"].includes(action.type) || (action.type === "Macro" && ["Hold","Sequence"].includes(config.value?.macros?.[action.value]?.mode||""));
function binding(id: string, slot: Trigger = selectedTrigger.value): HisenseAction {
  const p=config.value?.profiles?.[editingProfile.value];
  const scope=p&&Object.prototype.hasOwnProperty.call(p.button_bindings,id)?p:config.value;
  return (slot===1?scope?.button_bindings[id]:slot==="long"?scope?.long_press_bindings?.[id]:scope?.multi_click_bindings?.[id]?.[slot])||{type:"None"};
}
const selectedAction = computed(() => binding(selected.value));
const realtimeSelected = computed(() => realtime(binding(selected.value, 1)));
const shortcutKeys = computed(() => selectedAction.value.type === "SingleKey" ? [selectedAction.value.value] : selectedAction.value.type === "ComboKey" ? selectedAction.value.value : []);
const interval = computed({ get: () => config.value?.multi_click_interval_ms ?? 300, set: (n: number) => { if (config.value) config.value.multi_click_interval_ms = Math.max(150, Math.min(800, Math.round(Number(n) || 300))); } });
function writeBinding(id: string, action: HisenseAction, slot: Trigger = selectedTrigger.value) {
  if(!config.value)return;
  const p=config.value.profiles?.[editingProfile.value];
  if(p&&!Object.prototype.hasOwnProperty.call(p.button_bindings,id)) {
    p.button_bindings[id]=JSON.parse(JSON.stringify(config.value.button_bindings[id]||{type:"None"}));
    p.long_press_bindings||={};p.multi_click_bindings||={};
    if(config.value.long_press_bindings?.[id])p.long_press_bindings[id]=JSON.parse(JSON.stringify(config.value.long_press_bindings[id]));
    if(config.value.multi_click_bindings?.[id])p.multi_click_bindings[id]=JSON.parse(JSON.stringify(config.value.multi_click_bindings[id]));
  }
  const scope=p||config.value;
  scope.long_press_bindings||={};scope.multi_click_bindings||={};
  if(slot===1){scope.button_bindings[id]=action;if(realtime(action)){delete scope.long_press_bindings[id];delete scope.multi_click_bindings[id];}}
  else if(slot==="long")scope.long_press_bindings[id]=action;
  else {scope.multi_click_bindings[id]||={};scope.multi_click_bindings[id]![slot]=action;}
}
function restoreInheritedKey(){const p=config.value?.profiles?.[editingProfile.value];if(p){delete p.button_bindings[selected.value];delete p.long_press_bindings[selected.value];delete p.multi_click_bindings[selected.value];}}
function openLibrary(tab:"commands"|"keys"|"macros"){libraryTarget.value={id:selected.value,slot:selectedTrigger.value};libraryTab.value=tab;}
function assignFromLibrary(action:HisenseAction){writeBinding(libraryTarget.value.id,action,libraryTarget.value.slot);}
function cleanMissingReferences(){if(!config.value)return;const cfg=config.value;
  const clean=(map:Record<string,HisenseAction>)=>{for(const [id,a] of Object.entries(map)){if((a.type==="Macro"&&!cfg.macros?.[a.value])||(a.type==="SelectProfile"&&a.value&&a.value!=="default"&&!cfg.profiles?.[a.value]))map[id]={type:"None"};}};
  for(const scope of [cfg,...Object.values(cfg.profiles||{})]){clean(scope.button_bindings);clean(scope.long_press_bindings||{});for(const slots of Object.values(scope.multi_click_bindings||{}))clean(slots as Record<string,HisenseAction>);}
}
function updateMacros(macros:Record<string,HisenseMacro>){if(config.value){config.value.macros=macros;cleanMissingReferences();}}
function updateProfiles(profiles:Record<string,HisenseProfile>){if(!config.value)return;config.value.profiles=profiles;if(editingProfile.value!=="default"&&!profiles[editingProfile.value])editingProfile.value="default";if(config.value.fixed_profile&&config.value.fixed_profile!=="default"&&!profiles[config.value.fixed_profile])config.value.fixed_profile=null;cleanMissingReferences();}
function updateProfileSettings(enabled:boolean,fixed:string|null){if(config.value){config.value.profile_switching_enabled=enabled;config.value.fixed_profile=fixed;}}
watch(editingProfile,()=>{selectedTrigger.value=1;manualShortcut.value=false;});

function triggerCount(id: string) { return triggers.filter(([slot]) => binding(id, slot).type !== "None").length; }
function applyManualShortcut(keys: number[]) { writeBinding(selected.value, keys.length === 1 ? { type: "SingleKey", value: keys[0] } : { type: "ComboKey", value: keys }); manualShortcut.value = false; }
function mouseDirection(event: Event) { const [dx, dy] = (event.target as HTMLSelectElement).value.split(",").map(Number); if (selectedAction.value.type === "MouseMove") { selectedAction.value.value.dx = dx as -1|0|1; selectedAction.value.value.dy = dy as -1|0|1; } }
watch(selected, () => { selectedTrigger.value = 1; manualShortcut.value = false; });
watch(selectedTrigger, () => { manualShortcut.value = false; });
const search = ref(""), hovered = ref<string | null>(null);
const currentRow = computed(() => rows.value.find(row => row.id === selected.value) || rows.value[0]);
const filteredRows = computed(() => rows.value.filter(row => `${row.name} ${triggers.map(([slot]) => actionLabel(binding(row.id, slot))).join(" ")}`.toLowerCase().includes(search.value.trim().toLowerCase())));
const saveLabel = computed(() => ({ saved: "更改自动保存", saving: "正在保存…", dirty: "等待保存…", error: "保存失败 · 点击重试" })[saveState.value]);
const symbols: Record<string, string> = { power: "⏻", source: "⇥", pointer: "◎", ai: "AI", tv: "TV", shortcut: "➤", up: "↑", down: "↓", left: "←", right: "→", ok: "OK", back: "↩", home: "⌂", mic: "●", settings: "⚙", menu: "≡", volume_up: "+", volume_down: "−", pointer_ok: "OK", pointer_right: "◉" };
function selectButton(id: string) { selected.value = id; search.value = ""; }
type SettingsPanel = "pointer" | "voice" | "bridge" | "receiver";
const activeDialog = ref<SettingsPanel | null>(null), dialogRef = ref<HTMLElement | null>(null);
const dialogCopy = {
  pointer: { title: "指向与防抖", description: "调整移动手感、精细点按与确定键的触摸行为。" },
  voice: { title: "输入法设置", description: "为海信遥控器设置输入法快捷键与接收器麦克风。" },
  bridge: { title: "按键桥接", description: "管理海信接收器的桥接服务，映射设置会保留。" },
  receiver: { title: "接收器管理", description: "启用海信独立映射，或恢复接收器原驱动。" },
};
let dialogTrigger: HTMLElement | null = null;
function openSettings(panel: SettingsPanel) { activeDialog.value = panel; }
async function closeSettings() {
  if (busy.value || imeSaving.value) return;
  if (captureTarget.value === "$voice") await stopCapture();
  activeDialog.value = null;
  void saveNow();
}
function onDialogKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") { event.preventDefault(); void closeSettings(); return; }
  if (event.key !== "Tab" || !dialogRef.value) return;
  const items = Array.from(dialogRef.value.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex="0"]'));
  const first = items[0], last = items[items.length - 1];
  if (!first) { event.preventDefault(); return; }
  if (event.shiftKey && (document.activeElement === first || document.activeElement === dialogRef.value)) { event.preventDefault(); last.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
watch(activeDialog, async (panel, previous) => {
  if (panel) {
    if (!previous) dialogTrigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    await nextTick();
    dialogRef.value?.focus({ preventScroll: true });
  } else {
    await nextTick();
    if (dialogTrigger?.isConnected) dialogTrigger.focus({ preventScroll: true });
    dialogTrigger = null;
  }
});
const captureTarget = ref<string | null>(null);
let sessionId: number | null = null, poll: ReturnType<typeof setInterval> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null, captureTimer: ReturnType<typeof setInterval> | null = null;
let unlistenVisual: UnlistenFn | undefined, unlistenProfile: UnlistenFn | undefined;
let lastSaved = "", saveTask: Promise<void> | null = null, disposed = false;
const label = (id: string) => HISENSE_BUTTONS.find(([key]) => key === id)?.[1] || id;
const rows = computed(() => HISENSE_BUTTONS.map(([id, name]) => ({ id, name, action: binding(id,1) })));
const imeApplyHint = ref("");
const imeSaving = ref(false);
const activeImePreset = computed<ImePreset | null>(() => {
  const c = config.value;
  if (!c || !(c.voice_profile in IME_PRESETS)) return null;
  const id = c.voice_profile as ImePreset, p = IME_PRESETS[id];
  return c.voice_mode === p.triggerMode && c.voice_keys.length === p.shortcutVks.length
    && c.voice_keys.every((key, index) => key === p.shortcutVks[index]) ? id : null;
});
const chargingPower = computed(() => bridge.status.connected ? bridge.status.charging_power_connected ?? null : null);
const chargingLabel = computed(() => !bridge.status.connected ? "—" : chargingPower.value===true ? "充电电源已连接" : chargingPower.value===false ? "充电电源未连接" : "待更新");
const voiceShortcut = computed(() => config.value?.voice_keys.map(keyLabel).join(" + ") || "未设置");
const pointerModeLabel = computed(() => ({ touch: "1/2 · 原始触摸", move: "2/2 · 长按移动" })[bridge.status.pointer_mode] || "1/2 · 原始触摸");
const appliedVoice = computed(() => bridge.status.voice_active ? "正在传声" : "等待语音按键");
const lastButton = computed(() => bridge.status.last_button ? label(bridge.status.last_button) : bridge.status.last_source || "等待按键");
function actionKind(action: HisenseAction) { return action.type === "SingleKey" || action.type === "ComboKey" ? "Keyboard" : action.type; }
function actionLabel(action: HisenseAction) {
  if(action.type==="Macro")return `宏 · ${config.value?.macros?.[action.value]?.name||'未找到'}`;
  if(action.type==="SelectProfile")return action.value?`配置 · ${action.value==='default'?'桌面默认':config.value?.profiles?.[action.value]?.name||'未找到'}`:'按应用自动切换';
  if (action.type === "SingleKey") return keyLabel(action.value);
  if (action.type === "ComboKey") return action.value.map(keyLabel).join(" + ");
  if (action.type === "MouseMove") return `鼠标${action.value.dx < 0 ? '←' : action.value.dx > 0 ? '→' : action.value.dy < 0 ? '↑' : '↓'} ${action.value.step}px`;
  return ({ FocusChatGpt: "打开并聚焦 ChatGPT", VoiceInput: "使用语音设置中的预设", MouseClick: "鼠标左键", MouseRightClick: "鼠标右键", TouchClick: "触屏点按", PointerConfirm: "确定键（随模式）", TouchDrag: "按住触摸拖动", PointerAi: "循环切换两种模式", ToggleHighlighter: "荧光笔开关", CycleProfile: "循环切换配置文件", StopMacros: "停止所有宏", None: "不执行动作", TextInput: "输入文字", LaunchApp: "启动应用" })[action.type];
}
function setAction(id: string, kind: string) {
  if (!config.value) return;
  if(kind==="Macro"){openLibrary("macros");return;}
  const actions: Record<string, HisenseAction> = { FocusChatGpt: { type: "FocusChatGpt" }, MouseMove: { type: "MouseMove", value: { dx: 1, dy: 0, step: 10, accelerate: true } }, Keyboard: { type: "SingleKey", value: 0x20 }, MouseClick: { type: "MouseClick" }, MouseRightClick: { type: "MouseRightClick" }, TouchClick: { type: "TouchClick" }, PointerConfirm: { type: "PointerConfirm" }, TouchDrag: { type: "TouchDrag" }, PointerAi: { type: "PointerAi" }, ToggleHighlighter: { type: "ToggleHighlighter" }, CycleProfile: { type: "CycleProfile" }, StopMacros: { type: "StopMacros" }, SelectProfile: { type: "SelectProfile",value:"" }, VoiceInput: { type: "VoiceInput" }, None: { type: "None" }, TextInput: { type: "TextInput", value: "" }, LaunchApp: { type: "LaunchApp", value: "" } };
  writeBinding(id, actions[kind]);
}
function setText(id: string, text: string) { const a = binding(id); if (a?.type === "TextInput" || a?.type === "LaunchApp") a.value = text; }
function textValue(action: HisenseAction) { return action.type === "TextInput" || action.type === "LaunchApp" ? action.value : ""; }
function sourceFor(id: string) { return Object.values(config.value?.button_sources || {}).includes(id); }
async function refresh() { try { await bridge.refresh(); } catch (e) { if (!disposed) message.value = String(e); } }
async function saveNow() {
  if (saveTask) { await saveTask; if (saveState.value === "error") return; return saveNow(); }
  if (!config.value) return;
  const serialized = JSON.stringify(config.value);
  if (serialized === lastSaved) { saveState.value = "saved"; return; }
  saveState.value = "saving";
  const task = invoke("hisense_save_config", { config: JSON.parse(serialized) }).then(() => { lastSaved = serialized; saveState.value = "saved"; }).catch((e) => { saveState.value = "error"; message.value = `保存失败，设备仍使用上次成功保存的设置：${e}`; });
  saveTask = task;
  await task;
  if (saveTask === task) saveTask = null;
  if (saveState.value !== "error" && config.value && JSON.stringify(config.value) !== lastSaved) await saveNow();
}
watch(config, () => { if (!ready.value) return; saveState.value = "dirty"; if (saveTimer) clearTimeout(saveTimer); saveTimer = setTimeout(() => { void saveNow(); }, 350); }, { deep: true });
async function control(start: boolean) { busy.value = true; message.value = ""; try { await invoke(start ? "hisense_start" : "hisense_stop"); await refresh(); } catch (e) { message.value = String(e); } finally { busy.value = false; } }
async function setup(enabled: boolean) { busy.value = true; message.value = ""; try { message.value = await invoke<string>("hisense_setup_receiver", { enabled }); await refresh(); } catch (e) { message.value = String(e); } finally { busy.value = false; } }
async function learn(id: string | null) {
  message.value = "";
  try { if (id) { await saveNow(); if (saveState.value === "error") return; } await invoke("hisense_learn", { button: id }); await refresh(); }
  catch (e) { message.value = String(e); }
}
let lastLearning: string | null = null;
watch(() => bridge.status.learning_button, async (id) => {
  if (lastLearning && !id && config.value) {
    try { const learned = await invoke<HisenseConfig>("hisense_config"); config.value.button_sources = learned.button_sources; } catch (e) { message.value = String(e); }
  }
  lastLearning = id;
});
async function stopCapture() {
  if (captureTimer) clearInterval(captureTimer); captureTimer = null;
  const id = sessionId; sessionId = null; captureTarget.value = null;
  if (id != null) await invoke("capture_shortcut_stop", { sessionId: id }).catch(() => {});
}
async function capture(id: string) {
  await stopCapture(); if (id !== "$voice") selected.value = id; const slot = selectedTrigger.value; message.value = "";
  try {
    sessionId = await invoke<number>("capture_shortcut_start"); captureTarget.value = id;
    const expected = sessionId;
    captureTimer = setInterval(async () => {
      try {
        const result = await invoke<{ sessionId: number; keys: number[] } | null>("capture_shortcut_poll", { sessionId: expected });
        if (!result?.keys.length || result.sessionId !== sessionId || !config.value) return;
        if (id === "$voice") { config.value.voice_keys = result.keys; config.value.voice_profile = "custom"; }
        else writeBinding(id, result.keys.length === 1 ? { type: "SingleKey", value: result.keys[0] } : { type: "ComboKey", value: result.keys }, slot);
        await stopCapture(); await saveNow();
      } catch (e) { message.value = String(e); await stopCapture(); }
    }, 70);
  } catch (e) { message.value = String(e); }
}
async function transferMappings(kind: "import" | "export" | "reset") {
  if (transferBusy.value || captureTarget.value || !config.value) return;
  transferBusy.value = true; message.value = "";
  try {
    await saveNow();
    if (saveState.value === "error") return;
    if (kind === "export") {
      const path = await saveFile({ defaultPath: "nexus-prime-hisense-mappings.json", filters: [{ name: "海信按键映射", extensions: ["json"] }] });
      if (!path) return;
      await invoke("hisense_export_mappings", { path });
      message.value = "海信映射已导出；不包含按键学习、语音和指向设置。";
    } else {
      let next: HisenseConfig;
      if (kind === "import") {
        const path = await openFile({ multiple: false, directory: false, filters: [{ name: "海信按键映射", extensions: ["json"] }] });
        if (!path || Array.isArray(path)) return;
        next = await invoke<HisenseConfig>("hisense_import_mappings", { path });
      } else { next = await invoke<HisenseConfig>("hisense_reset_mappings", { profile: editingProfile.value }); }
      ready.value = false;
      if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; }
      config.value = next; lastSaved = JSON.stringify(next); if(editingProfile.value!=="default"&&!next.profiles?.[editingProfile.value])editingProfile.value="default";
      selectedTrigger.value = 1; manualShortcut.value = false; resetRequested.value = false;
      await nextTick(); ready.value = true; saveState.value = "saved";
      message.value = kind === "import" ? "海信映射已导入；指向、语音和按键学习数据已保留。" : "已恢复默认按键映射；指向、语音和按键学习数据已保留。";
    }
  } catch (e) { message.value = `${kind === "import" ? "导入" : kind === "export" ? "导出" : "重置"}失败，当前设置已保留：${e}`; }
  finally { transferBusy.value = false; }
}
async function applyPreset(preset: ImePreset) {
  if (!config.value || imeSaving.value) return;
  imeSaving.value = true; imeApplyHint.value = "";
  const p = IME_PRESETS[preset];
  config.value.voice_keys = [...p.shortcutVks]; config.value.voice_mode = p.triggerMode; config.value.voice_profile = preset;
  config.value.button_bindings.mic={type:"VoiceInput"};
  delete config.value.long_press_bindings?.mic;delete config.value.multi_click_bindings?.mic;
  if(preset==="codex")config.value.button_bindings.home={type:"FocusChatGpt"};
  try { await saveNow(); imeApplyHint.value = saveState.value === "error" ? "应用失败，当前编辑已保留，请重试。" : p.applyHint; }
  finally { imeSaving.value = false; }
}
const trailPresets = [
  { name: "柔和青光", color: "#22d3ee", width: 6, duration: 650, opacity: 66, glow: 50 },
  { name: "红色激光", color: "#ff4050", width: 3, duration: 350, opacity: 95, glow: 20 },
  { name: "演示荧光", color: "#facc15", width: 12, duration: 1100, opacity: 75, glow: 75 },
  { name: "轻盈紫光", color: "#c084fc", width: 5, duration: 500, opacity: 45, glow: 35 },
];
function matchesTrailPreset(p: typeof trailPresets[number]) {
  const c = config.value;
  return c && c.highlight_trail_color.toLowerCase() === p.color && c.highlight_trail_width_px === p.width
    && c.highlight_trail_duration_ms === p.duration && c.highlight_trail_opacity_percent === p.opacity && c.highlight_trail_glow_percent === p.glow;
}
function applyTrailPreset(p: typeof trailPresets[number]) {
  if (!config.value) return;
  Object.assign(config.value, { touch_indicator_enabled: true, highlight_trail_enabled: true,
    highlight_trail_color: p.color, highlight_trail_width_px: p.width, highlight_trail_duration_ms: p.duration,
    highlight_trail_opacity_percent: p.opacity, highlight_trail_glow_percent: p.glow });
}
type VisualFlags = Pick<HisenseConfig, "highlight_trail_enabled" | "touch_indicator_enabled">;
function applyRemoteVisualSettings(flags: VisualFlags) {
  if (!config.value || typeof flags.highlight_trail_enabled !== "boolean" || typeof flags.touch_indicator_enabled !== "boolean") return;
  // Native key toggles are already persisted. Keep unrelated pending edits dirty.
  const patch = { highlight_trail_enabled: flags.highlight_trail_enabled, touch_indicator_enabled: flags.touch_indicator_enabled };
  if (lastSaved) lastSaved = JSON.stringify({ ...JSON.parse(lastSaved), ...patch });
  Object.assign(config.value, patch);
}
function blockCapture(e: KeyboardEvent) { if (captureTarget.value) { e.preventDefault(); e.stopPropagation(); } }
onMounted(async () => {
  window.addEventListener("keydown", blockCapture, true); window.addEventListener("keyup", blockCapture, true);
  try {
    const unlisten = await listen<VisualFlags>("hisense-visual-settings", event => applyRemoteVisualSettings(event.payload));
    if (disposed) { unlisten(); return; }
    unlistenVisual = unlisten;
  } catch { /* Config remains editable if event subscription is unavailable. */ }
  try {
    unlistenProfile=await listen<{fixed_profile:string|null;profile_switching_enabled:boolean}>("hisense-profile-settings",event=>{
      if(!config.value)return;const patch=event.payload;
      if(lastSaved)lastSaved=JSON.stringify({...JSON.parse(lastSaved),...patch});Object.assign(config.value,patch);
    });
    if(disposed){unlistenProfile();return;}
  }catch { /* Stored settings are reloaded on the next visit. */ }
  try { config.value = await invoke<HisenseConfig>("hisense_config"); lastSaved = JSON.stringify(config.value); ready.value = true; await refresh(); }
  catch (e) { message.value = String(e); }
  try { audio.value = (await invoke<AudioDevice[]>("get_audio_devices")).filter(d => d.is_input); } catch { /* microphone can still be autodetected by the backend */ }
  poll = setInterval(() => { void refresh(); }, 1000);
});
onUnmounted(() => { void saveNow(); if (bridge.status.learning_button) void invoke("hisense_learn", { button: null }); disposed = true; unlistenVisual?.(); unlistenProfile?.(); if (poll) clearInterval(poll); if (saveTimer) clearTimeout(saveTimer); void stopCapture(); window.removeEventListener("keydown", blockCapture, true); window.removeEventListener("keyup", blockCapture, true); });
</script>

<template>
  <div class="hisense-page">
    <header class="page-heading">
      <h1>设备控制台</h1>
      <div class="header-actions"><span :class="['connection', { online: bridge.status.connected }]"><i class="status-dot" />{{ bridge.status.connected ? '已连接' : bridge.status.running ? '连接中' : '未连接' }}</span><button :disabled="busy" @click="control(!bridge.status.running)">{{ busy ? '处理中…' : bridge.status.connected ? '断开连接' : bridge.status.running ? '暂停连接' : '连接接收器' }}</button></div>
    </header>
    <p v-if="message" class="notice" role="status">{{ message }}</p>
    <p v-if="bridge.status.error" class="notice warning" role="status">{{ bridge.status.error }}</p>
    <div v-if="!config" class="panel">正在读取海信设置…</div>
    <template v-else>
      <section class="panel device-overview">
        <div class="device-primary">
          <img :src="remotePhoto" alt="海信灵指遥控器" />
          <div><span :class="['connection', { online: bridge.status.connected }]"><i class="status-dot" />{{ bridge.status.connected ? '接收器已连接' : '等待接收器' }}</span><h2>海信灵指遥控器</h2><p class="model">CRF3C65 / DRF3U65</p><div class="device-tags"><span>USB 接收器</span><span>UWB 指向</span><span>触摸控制</span></div></div>
        </div>
        <dl class="device-data">
          <div class="device-power"><dt>充电状态</dt><dd :class="{'online':chargingPower===true}"><span class="power-symbol" aria-hidden="true">{{ chargingPower===true?'ϟ':'⌁' }}</span>{{ chargingLabel }}</dd><small v-if="bridge.status.connected&&chargingPower===null">插拔充电线后自动更新</small></div>
          <div><dt>当前指向模式</dt><dd>{{ pointerModeLabel }}</dd></div>
          <div><dt>语音状态</dt><dd :class="{ online: bridge.status.voice_active }">{{ appliedVoice }}</dd></div>
          <div><dt>最近按键</dt><dd>{{ lastButton }}</dd></div>
          <div><dt>荧光笔</dt><dd>{{ config.highlight_trail_enabled ? '已开启' : '已关闭' }}</dd></div>
          <div><dt>精细点按</dt><dd>{{ config.adaptive_sensitivity ? `慢移降速 · ${Math.round(config.precision_ratio * 100)}%` : '固定灵敏度' }}</dd></div>
        </dl>
      </section>
      <section class="panel services">
        <div class="section-heading"><h2>服务状态</h2><span class="hint">设备状态实时更新</span></div>
        <div class="service-grid">
          <div class="service-item"><span class="service-icon">⌁</span><div><strong>USB 接收器</strong><small :class="{ online: bridge.status.connected }">{{ bridge.status.connected ? '已连接' : '未连接' }}</small></div></div>
          <div class="service-item"><span class="service-icon">⌘</span><div><strong>按键桥接</strong><small :class="{ online: bridge.status.running }">{{ bridge.status.running ? '运行中' : '已暂停' }}</small></div></div>
          <div class="service-item"><span class="service-icon">◎</span><div><strong>指向与触摸</strong><small :class="{ online: bridge.status.running && config.pointer_enabled }">{{ !config.pointer_enabled ? '已关闭' : bridge.status.running ? '已启用' : '等待连接' }}</small></div></div>
          <div class="service-item"><span class="service-icon">◉</span><div><strong>语音路由</strong><small :class="{ online: bridge.status.voice_active }">{{ bridge.status.voice_error ? '需要处理' : appliedVoice }}</small></div></div>
        </div>
      </section>
      <section class="profile-bar panel"><div><span>当前生效</span><strong>{{ bridge.status.active_profile_name||'桌面默认' }}</strong><small v-if="bridge.status.active_application">{{ bridge.status.active_application }}</small></div><label>正在编辑<select v-model="editingProfile" aria-label="编辑应用配置"><option value="default">桌面默认</option><option v-for="[id,p] in Object.entries(config.profiles||{})" :key="id" :value="id">{{ p.name }}{{ p.enabled?'':'（已停用）' }}</option></select></label><button @click="profileManagerOpen=true">管理游戏与应用配置</button></section>
      <section id="hisense-mapping" class="mappings">
        <div class="section-heading mapping-heading"><div><h2>按键映射</h2><p class="hint">点击遥控器上的按键，为它设置键盘、触摸或语音动作。</p></div><button :class="['save-state', { error: saveState === 'error' }]" :disabled="saveState === 'saved' || saveState === 'saving'" @click="saveNow">{{ saveState === 'saved' ? '✓ ' : '' }}{{ saveLabel }}</button></div>
        <div v-if="captureTarget" class="capture-banner" role="status">正在录入 {{ captureTarget === '$voice' ? '语音快捷键' : label(captureTarget) }}，请按目标键或组合键。<button @click="stopCapture">取消录入</button></div>
        <div v-if="bridge.status.learning_button" class="capture-banner" role="status">请只按遥控器“{{ label(bridge.status.learning_button) }}”，等待接收按键数据…<button @click="learn(null)">取消学习</button></div>
        <div class="mapping-layout">
          <aside class="panel remote-panel">
            <div class="section-heading"><h3>遥控器预览</h3><span class="hint">点击按键定位</span></div>
            <HisenseRemotePreview :selected-id="selected" :hover-id="hovered" @select="selectButton" @hover="hovered = $event" />
            <div class="preview-caption"><span>当前选择</span><strong>{{ label(selected) }}</strong><small>{{ actionLabel(currentRow.action) }}</small></div>
            <div class="channel-switch"><span>指向模式附加通道</span><div><button :class="{ active: selected === 'pointer_ok' }" @click="selectButton('pointer_ok')">指向确认</button><button :class="{ active: selected === 'pointer_right' }" @click="selectButton('pointer_right')">指向副键</button></div></div>
            <p class="editing-scope">{{ editingProfileName }} · {{ editingProfile==='default'?'默认映射':config.profiles?.[editingProfile]?.button_bindings[selected]?'独立映射':'继承桌面默认' }}<button v-if="editingProfile!=='default'&&config.profiles?.[editingProfile]?.button_bindings[selected]" @click="restoreInheritedKey">恢复继承</button></p>
            <div class="mapping-editor" :class="{ expanded: manualShortcut }" :data-editing-button="currentRow.id">
              <div class="editor-heading"><span class="key-symbol">{{ symbols[currentRow.id] }}</span><div><h3>{{ currentRow.name }}<span class="editor-label">编辑映射</span></h3><p class="hint">{{ currentRow.id.startsWith('pointer_') ? '遥控器指向开启时使用的独立输入通道' : '更改后自动保存，无需重启服务' }}</p></div></div>
              <div class="trigger-picker" role="group" aria-label="海信映射触发方式"><button v-for="[slot, name] in triggers" :key="slot" type="button" :data-trigger="slot" :class="{ active: selectedTrigger === slot }" :aria-pressed="selectedTrigger === slot" :disabled="captureTarget !== null || transferBusy || (slot !== 1 && realtimeSelected)" @click="selectedTrigger = slot">{{ name }}<i v-if="binding(currentRow.id, slot).type !== 'None'" /></button></div>
              <p v-if="realtimeSelected" class="hint trigger-note">当前为即时按住动作，直接响应按下与松开。改为键盘、鼠标点按等基础动作后，可设置独立连击和长按。切回即时动作时会清除本键的独立触发设置。</p>
              <p v-else class="hint trigger-note">{{ selectedTrigger === 'long' ? `长按阈值：${['volume_up','volume_down'].includes(currentRow.id) ? 400 : 280} ms；键盘、语音和拖动保持到松开，文字和应用执行一次；不再触发单击。` : '仅配置单击时立即响应；配置长按后，短按在松开时执行；配置连击后，只执行最终次数对应的动作。' }}</p>
              <div class="editor-fields">
            <select class="action-select" :disabled="transferBusy || captureTarget !== null" :value="actionKind(selectedAction)" :aria-label="`${currentRow.name}映射功能`" @change="setAction(currentRow.id, ($event.target as HTMLSelectElement).value)"><option value="Keyboard">键盘按键 / 快捷键</option><option v-if="selectedTrigger === 1 || selectedTrigger === 'long'" value="VoiceInput">语音输入</option><option value="MouseClick">鼠标左键</option><option value="MouseRightClick">鼠标右键</option><option value="TouchClick">触屏点按</option><option v-if="selectedTrigger === 1" value="PointerConfirm">确定键（随模式）</option><option v-if="selectedTrigger === 1 || selectedTrigger === 'long'" value="TouchDrag">按住触摸拖动</option><option v-if="selectedTrigger === 1" value="PointerAi">循环切换两种模式</option><option value="ToggleHighlighter">荧光笔开关</option><option value="CycleProfile">循环切换配置文件</option><option value="StopMacros">停止所有宏</option><option value="SelectProfile">按应用自动切换</option><option value="Macro">宏…</option><option value="MouseMove">鼠标方向移动</option><option value="FocusChatGpt">打开并聚焦 ChatGPT</option><option value="TextInput">输入文字</option><option value="LaunchApp">启动应用</option><option value="None">不执行</option></select>
            <div class="binding-value"><input v-if="selectedAction.type === 'TextInput' || selectedAction.type === 'LaunchApp'" :value="textValue(selectedAction)" :aria-label="`${currentRow.name}动作内容`" :placeholder="selectedAction.type === 'TextInput' ? '要输入的文字' : '应用程序完整路径'" @input="setText(currentRow.id, ($event.target as HTMLInputElement).value)" /><span v-else>{{ actionLabel(selectedAction) }}</span></div>
            <div class="assignment-entries" role="group" aria-label="命令按键和宏"><button :disabled="transferBusy||captureTarget!==null" @click="openLibrary('commands')">命令</button><button :disabled="transferBusy||captureTarget!==null" @click="openLibrary('keys')">按键</button><button :disabled="transferBusy||captureTarget!==null" @click="openLibrary('macros')">宏</button></div>
            <div class="row-actions"><button v-if="actionKind(selectedAction) === 'Keyboard'" :disabled="captureTarget !== null" @click="capture(currentRow.id)">录入</button><button v-if="actionKind(selectedAction) === 'Keyboard'" :disabled="captureTarget !== null" @click="manualShortcut = !manualShortcut">手动组合</button><button :disabled="captureTarget !== null || selectedAction.type === 'None'" @click="writeBinding(currentRow.id, { type: 'None' })">清除</button><button class="learn-button" v-if="!currentRow.id.startsWith('pointer_')" :disabled="!bridge.status.connected || captureTarget !== null || bridge.status.learning_button !== null" title="识别实体按键的数据来源，不会修改映射功能" @click="learn(currentRow.id)">学习键码</button></div>

              </div>
              <div v-if="selectedAction.type === 'MouseMove'" class="mouse-options"><label>方向<select aria-label="鼠标移动方向" :value="`${selectedAction.value.dx},${selectedAction.value.dy}`" @change="mouseDirection"><option value="0,-1">向上</option><option value="0,1">向下</option><option value="-1,0">向左</option><option value="1,0">向右</option></select></label><label>步幅（px）<input v-model.number="selectedAction.value.step" aria-label="鼠标移动步幅" type="range" min="1" max="100" /><strong>{{ selectedAction.value.step }} px</strong></label><label class="switch"><input v-model="selectedAction.value.accelerate" type="checkbox" />按住逐渐加速</label></div>
              <VoiceShortcutComposer v-if="manualShortcut" :initial-keys="shortcutKeys" :button-label="currentRow.name" :slot-label="triggerLabel" @apply="applyManualShortcut" @cancel="manualShortcut = false" />
            </div>
          </aside>
          <section class="panel mapping-list-panel">
            <div class="mapping-list-head"><div><h3>映射列表</h3><p class="hint">18 个实体按键 · 2 个指向通道 · 即时保存</p></div><div class="mapping-tools"><button :disabled="transferBusy || captureTarget !== null" @click="transferMappings('export')">导出配置</button><button :disabled="transferBusy || captureTarget !== null" @click="transferMappings('import')">导入配置</button><button :disabled="transferBusy || captureTarget !== null" @click="resetRequested = !resetRequested">一键重置</button><input v-model="search" type="search" aria-label="搜索海信按键映射" placeholder="搜索按键或功能" /></div></div>
            <div v-if="resetRequested" class="reset-confirm" role="alert"><p>重置“{{ editingProfileName }}”的按键？仅重置当前配置，保留宏库、其他应用配置、指向、语音和学习设置。</p><button :disabled="transferBusy" @click="transferMappings('reset')">重置此配置</button><button :disabled="transferBusy" @click="resetRequested = false">取消</button></div>
            <div class="mapping-grid">
              <button v-for="row in filteredRows" :key="row.id" type="button" :class="['mapping-row', { selected: selected === row.id }]" :data-button-id="row.id" :aria-pressed="selected === row.id" @click="selected = row.id" @mouseenter="hovered = row.id" @mouseleave="hovered = null" @focus="hovered = row.id" @blur="hovered = null">
                <span class="key-symbol" aria-hidden="true">{{ symbols[row.id] }}</span>
                <span class="button-name"><strong>{{ row.name }}</strong><small :class="{ observed: bridge.status.observed_buttons.includes(row.id) }">{{ bridge.status.observed_buttons.includes(row.id) ? '已收到数据' : sourceFor(row.id) ? '已识别 · 待按键' : '待识别' }}<span v-if="triggerCount(row.id) > 1"> · {{ triggerCount(row.id) }} 种触发</span></small></span>
                <span class="key-binding" :title="actionLabel(row.action)">{{ row.action.type === 'VoiceInput' ? '语音输入' : actionLabel(row.action) }}</span><span class="chevron" aria-hidden="true">›</span>
              </button>
            </div>
            <p v-if="!filteredRows.length" class="empty-state">没有找到匹配的按键或功能。</p>
            <label class="interval-control">连击间隔 <strong>{{ interval }} ms</strong><input v-model.number="interval" aria-label="海信连击间隔" type="range" min="150" max="800" step="50" /></label>
            <p class="hint list-hint">ⓘ 选择图片或列表中的任意按键即可编辑；所有按键均可设为语音输入。</p>
            <p class="hint learning-hint">选择功能后会自动保存，已识别的按键无需再学习。“学习键码”仅用于首次识别或纠正识别；若只发送红外，需硬件支持后才能接收。</p>
          </section>
        </div>
      </section>
      <section class="panel quick-section">
        <div class="section-heading"><h2>快捷操作</h2><span class="hint">海信遥控器设置</span></div>
        <div class="quick-grid">
          <button @click="openSettings('pointer')"><strong>指向与防抖</strong><small>灵敏度、慢移精度、触摸模式</small></button>
          <button @click="openSettings('voice')"><strong>输入法设置</strong><small>输入法预设、快捷键与麦克风</small></button>
          <button @click="openSettings('bridge')"><strong>按键桥接</strong><small>{{ bridge.status.running ? '运行中 · 管理连接' : '已暂停 · 管理连接' }}</small></button>
          <button @click="openSettings('receiver')"><strong>接收器管理</strong><small>启用独立映射或恢复驱动</small></button>
        </div>
      </section>
      <Teleport to="body">
        <div v-if="activeDialog && activeDialog !== 'voice'" class="settings-backdrop" @click.self="closeSettings">
          <section ref="dialogRef" class="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="hisense-settings-title" aria-describedby="hisense-settings-description" tabindex="-1" @keydown="onDialogKeydown">
            <header class="settings-dialog-head"><div><h3 id="hisense-settings-title">{{ dialogCopy[activeDialog].title }}</h3><p id="hisense-settings-description">{{ dialogCopy[activeDialog].description }}</p></div><button type="button" class="dialog-close" :aria-label="`关闭${dialogCopy[activeDialog].title}`" :disabled="busy" @click="closeSettings"><svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" /></svg></button></header>
            <div class="settings-dialog-body">
              <p v-if="message" class="notice" role="status">{{ message }}</p>
              <div v-if="captureTarget === '$voice'" class="capture-banner" role="status">正在录入语音快捷键，请按目标键或组合键。<button @click="stopCapture">取消录入</button></div>
        <section v-if="activeDialog === 'pointer'" id="hisense-pointer" class="panel pointer-panel">
          <div class="section-heading"><h2>指向控制</h2><label class="switch"><input v-model="config.pointer_enabled" type="checkbox" />启用</label></div>
          <p class="hint">以当前鼠标位置开始跟随，反向移动即可离开屏幕边缘。</p>
          <label class="slider-label">移动灵敏度 <strong>{{ config.sensitivity.toFixed(1) }} 倍</strong><input v-model.number="config.sensitivity" aria-label="移动灵敏度" type="range" min="0.5" max="4" step="0.1" /></label>
          <label class="switch" style="margin-top:16px"><input v-model="config.adaptive_sensitivity" type="checkbox" />慢移自动降低灵敏度</label>
          <label class="slider-label">慢移倍率 <strong>{{ Math.round(config.precision_ratio * 100) }}% · {{ (config.sensitivity * config.precision_ratio).toFixed(2) }} 倍</strong><input v-model.number="config.precision_ratio" :disabled="!config.adaptive_sensitivity" aria-label="慢移倍率" type="range" min="0.1" max="0.75" step="0.05" /></label>
          <p class="hint">慢移时进入精细瞄准，快速移动恢复上方倍率；过渡自动完成。慢移倍率越低，微调越细。</p>
          <label class="slider-label">防抖强度 <strong>{{ config.smoothing_ms }} ms</strong><input v-model.number="config.smoothing_ms" aria-label="防抖强度" type="range" min="0" max="80" step="1" /></label>
          <label class="slider-label">微动过滤 <strong>{{ config.deadzone_pixels.toFixed(1) }} px</strong><input v-model.number="config.deadzone_pixels" aria-label="微动过滤" type="range" min="0" max="12" step="0.5" /></label>
          <p class="hint">微动过滤抑制手部细小抖动；防抖越强越平稳。快速移动会自动减轻平滑，保持跟手。</p>
          <label class="switch"><input v-model="config.auto_connect" type="checkbox" />下次启动自动连接海信接收器</label>
          <label class="switch"><input v-model="config.touch_indicator_enabled" type="checkbox" />显示触摸圆点，触摸按下时扩散</label>
          <div class="indicator-appearance"><label>圆点颜色<input v-model="config.indicator_color" aria-label="触摸圆点颜色" type="color" :disabled="!config.touch_indicator_enabled" /></label><label>按下与扩散颜色<input v-model="config.indicator_pressed_color" aria-label="触摸按下颜色" type="color" :disabled="!config.touch_indicator_enabled" /></label></div>
          <div class="trail-settings">
            <div class="section-heading"><h3>荧光笔</h3><label class="switch"><input v-model="config.highlight_trail_enabled" aria-label="开启荧光轨迹" type="checkbox" />开启荧光轨迹</label></div>
            <p class="hint">默认开启，移动时留下渐隐光迹。可将任意遥控按键映射为“荧光笔开关”，切换时左上角显示提示。</p>
            <div class="trail-presets" aria-label="荧光笔预设"><button v-for="p in trailPresets" :key="p.name" :class="{ active: matchesTrailPreset(p) }" :aria-pressed="!!matchesTrailPreset(p)" @click="applyTrailPreset(p)"><i :style="{ background: p.color }" />{{ p.name }}</button></div>
            <p class="hint">选择预设会开启圆点和荧光笔；也可在下方单独调节，修改自动保存。</p>
            <fieldset :disabled="!config.highlight_trail_enabled" class="trail-controls">
              <label class="trail-color">轨迹颜色<input v-model="config.highlight_trail_color" aria-label="荧光轨迹颜色" type="color" /></label>
              <label class="slider-label">轨迹粗细 <strong>{{ config.highlight_trail_width_px }} px</strong><input v-model.number="config.highlight_trail_width_px" aria-label="荧光轨迹粗细" type="range" min="2" max="20" step="1" /></label>
              <label class="slider-label">消失时长 <strong>{{ (config.highlight_trail_duration_ms / 1000).toFixed(2) }} 秒</strong><input v-model.number="config.highlight_trail_duration_ms" aria-label="荧光轨迹时长" type="range" min="150" max="2000" step="50" /></label>
              <label class="slider-label">不透明度 <strong>{{ config.highlight_trail_opacity_percent }}%</strong><input v-model.number="config.highlight_trail_opacity_percent" aria-label="荧光轨迹不透明度" type="range" min="10" max="100" step="1" /></label>
              <label class="slider-label">发光强度 <strong>{{ config.highlight_trail_glow_percent }}%</strong><input v-model.number="config.highlight_trail_glow_percent" aria-label="荧光轨迹发光强度" type="range" min="0" max="100" step="1" /></label>
            </fieldset>
            <div class="trail-footer"><span class="hint">关闭触摸圆点时一起隐藏，不影响点按和滑动。</span><button @click="applyTrailPreset(trailPresets[0])">恢复荧光笔默认</button></div>
          </div>
          <label class="slider-label">确定键长按阈值 <strong>{{ config.pointer_hold_ms }} ms</strong><input v-model.number="config.pointer_hold_ms" aria-label="确定键长按阈值" type="range" min="150" max="1000" step="50" /></label>
          <p class="hint">UWB 当前模式：<strong>{{ pointerModeLabel }}</strong>；绑定“循环切换两种模式”的按键每按一次切换到下一种。</p>
          <p class="hint">原始触摸：圆点自由跟随，确定按下即触摸；长按移动：短按点按、长按只移动。启动默认原始触摸，遥控器自身指向仍需开启。</p>
        </section>
      <section v-else-if="activeDialog === 'receiver'" id="hisense-receiver" class="panel receiver-tools"><div><h2>接收器管理</h2><p class="hint">首次启用独立映射需要管理员确认。只配置海信接收器，电脑键盘保持原样；启用后请让 Nexus-Prime 在后台运行。</p></div><button :disabled="busy" @click="setup(true)">启用独立按键映射</button><button :disabled="busy" @click="setup(false)">恢复接收器原驱动</button></section>
        <section v-else-if="activeDialog === 'bridge'" class="bridge-panel">
          <div><span class="dialog-eyebrow">HISENSE · USB BRIDGE</span><h2>海信按键桥接</h2><p class="hint">服务运行时接收遥控器按键，并按已保存的配置执行动作。暂停后，指向和按键映射暂时停止，设置仍然保留。</p><p class="hint">连接接收器后可恢复；原来的 UWB 模式、触摸和慢移精度设置会继续使用。</p></div>
          <aside class="bridge-callout"><span :class="['connection', { online: bridge.status.running }]">{{ bridge.status.running ? '桥接运行中' : '桥接已暂停' }}</span><strong>{{ bridge.status.connected ? '接收器已连接' : '等待接收器' }}</strong><p class="hint">{{ busy ? '正在处理，请稍候…' : bridge.status.running ? '按键不响应时可先暂停，再重新启动。' : '启动后会重新连接海信接收器。' }}</p><button class="dialog-primary" :disabled="busy" @click="control(!bridge.status.running)">{{ busy ? '处理中…' : bridge.status.running ? '暂停按键桥接' : '启动按键桥接' }}</button></aside>
        </section>

            </div>
            <footer class="settings-dialog-footer"><span :class="{ 'inline-error': saveState === 'error' }">{{ saveLabel }}</span><button v-if="saveState === 'error'" @click="saveNow">重试保存</button><button class="dialog-primary" :disabled="busy" @click="closeSettings">完成</button></footer>
          </section>
        </div>
      </Teleport>
        <HisenseAssignmentLibrary :open="libraryTab!==null" :initial-tab="libraryTab||'commands'" :macros="config.macros||{}" :profiles="config.profiles||{}" :button-label="label(libraryTarget.id)" :slot-label="triggers.find(([id])=>id===libraryTarget.slot)?.[1]||'单击'" :base-slot="libraryTarget.slot===1" :allow-held="libraryTarget.slot===1||libraryTarget.slot==='long'" :disabled="transferBusy||captureTarget!==null" @close="libraryTab=null" @assign="assignFromLibrary" @macros-change="updateMacros" />
        <HisenseProfileManager :open="profileManagerOpen" :profiles="config.profiles||{}" :enabled="config.profile_switching_enabled!==false" :fixed="config.fixed_profile||null" :editing="editingProfile" :active-name="bridge.status.active_profile_name||'桌面默认'" @close="profileManagerOpen=false" @change="updateProfiles" @settings="updateProfileSettings" @select="editingProfile=$event" />
        <InputMethodSettingsDialog :open="activeDialog === 'voice'" :config-ready="ready" :saving="imeSaving || saveState === 'saving'" :apply-hint="imeApplyHint" :active-preset="activeImePreset" remote-name="海信遥控器" @close="closeSettings" @apply="applyPreset">
          <template #settings><div class="voice-extra" id="hisense-voice"><div class="section-heading"><h3>海信语音与麦克风</h3><span :class="['voice-state', { active: bridge.status.voice_active }]">{{ appliedVoice }}</span></div><p class="hint">自定义快捷键与接收器麦克风设置，修改后自动保存。</p>
            <p v-if="captureTarget === '$voice'" class="capture-banner" role="status">正在录入语音快捷键，请按目标键或组合键。<button @click="stopCapture">取消录入</button></p>
          <div class="shortcut"><span>{{ voiceShortcut }}</span><button :disabled="captureTarget !== null" @click="capture('$voice')">录入快捷键</button></div>
          <label>快捷键方式<select v-model="config.voice_mode" @change="config.voice_profile = 'custom'" aria-label="语音快捷键方式"><option value="Hold">按下保持，松开释放</option><option value="Toggle">按下时发送一次</option></select></label>
          <label>接收器麦克风<select v-model="config.microphone_name" aria-label="海信麦克风"><option :value="null">自动寻找海信 / Audio-HID</option><option v-for="device in audio" :key="device.id" :value="device.name">{{ device.name }}</option></select></label>
          <div class="meter" role="meter" aria-label="海信麦克风电平" :aria-valuenow="Math.round(bridge.status.voice_level * 100)" aria-valuemin="0" aria-valuemax="100"><i :style="{ width: `${bridge.status.voice_level * 100}%` }" /></div>
          <label class="slider-label">麦克风增益 <strong>{{ config.microphone_gain_db }} dB</strong><input v-model.number="config.microphone_gain_db" aria-label="海信麦克风增益" type="range" min="0" max="24" step="1" /></label>
          <p class="hint">传声仍随按键按下和松开。需要在输入法中选择 CABLE Output；音频有电平不代表输入法已识音。</p>
          <p v-if="bridge.status.voice_error" class="inline-error">{{ bridge.status.voice_error }}</p>
            <p v-if="message" class="notice" role="status">{{ message }}</p><p class="hint" role="status">{{ saveLabel }}</p>
          </div></template>
        </InputMethodSettingsDialog>

    </template>
  </div>
</template>
<style scoped>
.hisense-page{max-width:1280px;margin:0 auto;padding:28px 32px 40px;color:var(--text)}
.page-heading,.section-heading,.header-actions,.shortcut,.receiver-tools{display:flex;align-items:center;justify-content:space-between;gap:14px}.page-heading{margin-bottom:24px}.eyebrow{font-size:10px;letter-spacing:1.5px;color:var(--text-secondary);margin:0 0 8px}h1{font-size:29px;letter-spacing:-.7px;margin:0 0 7px}h2{font-size:16px;margin:0}.sub,.hint{color:var(--text-secondary);line-height:1.65;font-size:12px}.sub{margin:0}.hint{margin:10px 0}.overview{display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-bottom:20px}.panel{padding:22px;border:1px solid var(--border);border-radius:18px;background:var(--card-bg);box-shadow:0 3px 20px #00000003}.connection,.voice-state{font-size:11px;padding:7px 10px;border-radius:30px;background:var(--surface-soft);color:var(--text-secondary)}.online,.voice-state.active{color:var(--success)}label{display:block;font-size:12px;margin-top:13px}label select{display:block;width:100%;margin-top:7px}.switch{display:flex;align-items:center;gap:8px;margin:0;font-size:12px}.slider-label{margin-top:21px}.slider-label strong{float:right;font-size:12px;font-weight:600}input[type=range]{width:100%;margin-top:13px;accent-color:var(--primary)}.pointer-panel>.switch{margin-top:18px}button,select,input:not([type=range]):not([type=checkbox]){font:inherit;border:1px solid var(--border);background:var(--surface-soft);color:var(--text);border-radius:9px;padding:8px 11px;font-size:12px}button{cursor:pointer;white-space:nowrap}button:hover:not(:disabled){border-color:var(--primary);color:var(--primary)}button:disabled{opacity:.5;cursor:default}.shortcut{margin-top:12px;padding:9px 11px;background:var(--surface-soft);border-radius:10px;font-size:12px}.meter{height:5px;border-radius:6px;background:var(--surface-soft);overflow:hidden;margin-top:18px}.meter i{display:block;height:100%;background:var(--success);transition:width .15s}.mappings{margin-bottom:20px}.mapping-grid{display:grid;grid-template-columns:1fr;gap:1px;margin-top:14px}.mapping-row{display:grid;grid-template-columns:145px 165px minmax(120px,1fr) 112px;gap:14px;align-items:center;padding:12px 8px;border-top:1px solid var(--border);border-radius:4px}.mapping-row.selected{background:var(--surface-soft)}.button-name{display:flex;flex-direction:column;gap:5px;font-size:12px}.button-name small{font-size:10px;color:var(--text-secondary)}.button-name small.observed{color:var(--success)}.binding-value{min-width:0;font-size:12px;color:var(--text-secondary)}.binding-value span{overflow-wrap:anywhere}.binding-value input{width:100%;box-sizing:border-box}.row-actions{display:flex;justify-content:flex-end;gap:6px}.row-actions button{padding:6px 9px}.learning-hint{max-width:900px}.notice,.capture-banner{padding:12px 16px;background:var(--surface-soft);border:1px solid var(--border);border-radius:10px;font-size:12px;line-height:1.7;margin:0 0 16px}.capture-banner{display:flex;justify-content:space-between;align-items:center;margin-top:14px}.inline-error,.warning{color:var(--danger);font-size:12px}.receiver-tools>div{flex:1}.receiver-tools .hint{margin-bottom:0}@media(max-width:960px){.hisense-page{padding:22px 20px}.overview{gap:14px}.panel{padding:18px}.header-actions{flex-direction:column;align-items:flex-end}.mapping-row{grid-template-columns:125px 155px minmax(90px,1fr) 106px;gap:10px}.receiver-tools{flex-wrap:wrap}.receiver-tools>div{flex-basis:100%}}@media(max-width:740px){.overview{grid-template-columns:1fr}.mapping-row{grid-template-columns:120px 1fr}.binding-value{grid-column:1}.row-actions{grid-column:2}.page-heading{align-items:flex-start}}

.hisense-page { max-width: 1260px; padding: 0; }
.page-heading { margin-bottom: 20px; }
.panel { border-radius: 14px; box-shadow: var(--shadow-sm); }
.device-overview { display: grid; grid-template-columns: 1.1fr 1fr; padding: 24px; min-height: 180px; margin-bottom: 16px; }
.device-primary { display: flex; align-items: center; gap: 24px; padding-right: 24px; border-right: 1px solid var(--border); }
.device-primary img { width: 62px; height: 170px; object-fit: cover; border-radius: 8px; }
.device-primary h2 { font-size: 21px; margin: 12px 0 5px; }
.model { font-size: 11px; color: var(--text-secondary); margin: 0; letter-spacing: .8px; }
.device-primary .connection { padding: 0; background: transparent; font-size: 12px; display: inline-flex; gap: 8px; align-items: center; }
.status-dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; box-shadow: 0 0 0 4px #10b98118; }
.device-tags { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 15px; }
.device-tags span { padding: 6px 9px; background: var(--surface-soft); color: var(--text-secondary); border-radius: 6px; font-size: 10px; }
.device-data { margin: 0; padding-left: 30px; display: grid; grid-template-columns: 1fr 1fr; gap: 24px; align-content: center; }
.device-data dt { color: var(--text-secondary); font-size: 12px; margin-bottom: 8px; }
.device-data dd { margin: 0; font-size: 14px; font-weight: 650; overflow-wrap: anywhere; }
.services, .quick-section { margin-bottom: 20px; }
.section-heading > .hint { margin: 0; }
.service-grid { display: grid; grid-template-columns: repeat(4,minmax(0,1fr)); gap: 12px; margin-top: 20px; }
.service-item { display: flex; gap: 12px; align-items: center; padding: 16px 12px; border: 1px solid var(--border); background: var(--surface-soft); border-radius: 10px; min-width: 0; }
.service-icon, .key-symbol { display: grid; place-items: center; flex-shrink: 0; width: 34px; height: 36px; border-radius: 9px; color: var(--primary); background: #3478f60c; font-size: 17px; }
.service-item strong { font-size: 12px; display: block; margin-bottom: 6px; }
.service-item small { color: var(--text-secondary); font-size: 11px; }
.service-item small.online { color: var(--success); }
.mapping-heading { margin: 24px 0 14px; }
.mapping-heading h2 { font-size: 20px; }
.mapping-heading .hint { margin-bottom: 0; }
.save-state:disabled { opacity: 1; color: var(--success); background: #10b9810b; border-color: #10b98112; }
.save-state.error { color: var(--danger); }
.mapping-layout { display: grid; grid-template-columns: 320px minmax(0,1fr); gap: 16px; align-items: stretch; }
.remote-panel { display: flex; flex-direction: column; padding: 18px; }
.remote-panel .section-heading { margin-bottom: 10px; }
.remote-panel .hint { font-size: 10px; }
h3 { font-size: 15px; margin: 0; }
.preview-caption { display: grid; gap: 7px; padding: 13px; background: #3478f60c; border: 1px solid #3478f625; border-radius: 10px; margin-top: 14px; }
.preview-caption > span, .preview-caption small { font-size: 11px; color: var(--text-secondary); line-height: 1.6; }
.preview-caption strong { font-size: 14px; color: var(--primary); }
.channel-switch { margin-top: 16px; }
.channel-switch > span { color: var(--text-secondary); font-size: 10px; }
.channel-switch > div { display: flex; gap: 7px; margin-top: 9px; }
.channel-switch button { flex: 1; padding: 7px; font-size: 11px; }
.channel-switch .active { border-color: var(--primary); color: var(--primary); }
.mapping-list-panel { min-width: 0; padding: 20px; }
.mapping-list-head { display: flex; align-items: center; justify-content: space-between; gap: 14px; margin-bottom: 16px; }
.mapping-list-head .hint { margin: 7px 0 0; font-size: 11px; }
.mapping-list-head input { width: 190px; min-width: 0; }
.mapping-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 0; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; margin-top: 0; }
.mapping-row { display: flex; gap: 10px; padding: 13px 11px; border: 0; border-bottom: 1px solid var(--border); border-radius: 0; background: var(--card-bg); text-align: left; min-width: 0; white-space: normal; min-height: 72px; }
.mapping-row:nth-child(odd) { border-right: 1px solid var(--border); }
.mapping-row:nth-last-child(-n+2) { border-bottom: 0; }
.mapping-row:hover, .mapping-row.selected { background: #3478f608; color: var(--text); }
.mapping-row.selected { box-shadow: inset 3px 0 var(--primary); }
.mapping-row .key-symbol { width: 30px; height: 32px; font-size: 12px; color: var(--text-secondary); }
.mapping-row.selected .key-symbol { color: var(--primary); background: #3478f61a; }
.button-name { gap: 5px; min-width: 40px; flex-shrink: 0; }
.button-name strong { font-size: 12px; }
.key-binding { margin-left: auto; border: 1px solid var(--border); padding: 5px 7px; border-radius: 7px; font-size: 10px; color: var(--text-secondary); max-width: 110px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; box-shadow: 0 2px 0 #16203309; }
.chevron { color: var(--text-secondary); font-size: 19px; }
.mapping-editor { margin: 0 0 16px; border: 1px solid #3478f633; border-radius: 12px; padding: 16px; background: #3478f605; }
.editor-heading { display: flex; gap: 10px; align-items: center; margin-bottom: 14px; }
.editor-heading .hint { margin: 5px 0 0; font-size: 10px; }
.editor-label { margin-left: 10px; color: var(--text-secondary); font-size: 10px; font-weight: 400; }
.editor-fields { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.editor-fields > select { max-width: 100%; }
.editor-fields .binding-value { flex: 1; min-width: 140px; }
.editor-fields .row-actions { margin-left: auto; }
.list-hint { margin: 14px 0 0; font-size: 11px; }
.learning-hint { font-size: 10px; margin-bottom: 0; }
.empty-state { padding: 20px; font-size: 12px; color: var(--text-secondary); text-align: center; }
.quick-grid { display: grid; grid-template-columns: repeat(4,minmax(0,1fr)); gap: 12px; margin-top: 20px; }
.quick-grid button { padding: 16px; background: var(--card-bg); text-align: left; white-space: normal; }
.quick-grid strong { display: block; margin-bottom: 10px; font-size: 12px; }
.quick-grid small { color: var(--text-secondary); font-size: 10px; line-height: 1.6; }
.overview { scroll-margin-top: 20px; }
.pointer-panel, .voice-panel, .receiver-tools { scroll-margin-top: 20px; }
@media (max-width: 1150px) {
 .mapping-layout { grid-template-columns: 290px minmax(0,1fr); }
 .mapping-list-panel { padding: 15px; }
 .remote-panel { align-self: start; }
}
@media (max-width: 950px) {
 .mapping-grid { grid-template-columns: 1fr; }
 .mapping-row:nth-child(odd) { border-right: 0; }
 .mapping-row:nth-last-child(2) { border-bottom: 1px solid var(--border); }
}
@media (max-width: 850px) {
 .hisense-page { padding: 0; }
 .device-overview { grid-template-columns: 1fr; gap: 24px; }
 .device-primary { border-right: 0; border-bottom: 1px solid var(--border); padding: 0 0 20px; }
 .device-data { padding-left: 0; }
 .service-grid, .quick-grid { grid-template-columns: repeat(2,minmax(0,1fr)); }
 .mapping-heading { align-items: flex-start; }
 .mapping-heading .save-state { font-size: 10px; }
 .mapping-list-head { align-items: stretch; flex-direction: column; }
 .mapping-list-head input { width: 100%; box-sizing: border-box; }
}
@media (max-width: 640px) {
 .mapping-layout { grid-template-columns: minmax(0,1fr); }
 .remote-panel :deep(.hisense-remote) { width: 165px; }
 .remote-panel { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: start; }
 .remote-panel .section-heading { grid-column: 1/-1; }
 .remote-panel :deep(.hisense-remote) { grid-row: span 2; }
 .preview-caption, .channel-switch { margin-top: 0; }
 .channel-switch > div { flex-direction: column; }
 .overview { grid-template-columns: 1fr; }
 .device-primary h2 { font-size: 18px; }
 .page-heading { flex-wrap: wrap; }
 .header-actions { flex-direction: row; }
}

/* 与小米输入法设置使用相同的居中浮层、标题栏和圆角卡片。 */
.settings-backdrop { position: fixed; inset: 0; z-index: 3200; display: grid; place-items: center; padding: 24px 32px; background: var(--overlay); color: var(--text); }
.settings-dialog { width: min(920px, calc(100vw - 64px)); max-height: min(800px, calc(100dvh - 64px)); min-height: 0; display: grid; grid-template-rows: auto minmax(0,1fr) auto; overflow: hidden; box-sizing: border-box; border: 1px solid var(--border); border-radius: 14px; background: var(--card-bg); box-shadow: var(--dialog-shadow); outline: none; animation: settings-dialog-in .16s ease-out; }
.settings-dialog-head { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 18px 20px 14px; border-bottom: 1px solid var(--border); }
.settings-dialog-head h3 { font-size: 16px; margin: 0; }
.settings-dialog-head p { font-size: 12px; color: var(--text-secondary); margin: 5px 0 0; line-height: 1.6; }
.settings-dialog .dialog-close { width: 32px; height: 32px; padding: 7px; display: grid; place-items: center; flex-shrink: 0; border-radius: 9px; background: var(--surface-raised); }
.dialog-close svg { width: 17px; height: 17px; }
.settings-dialog-body { min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: 22px; }
.settings-dialog-body > .panel { border: 0; border-radius: 0; box-shadow: none; padding: 0; background: transparent; }
.settings-dialog-body .receiver-tools { display: flex; flex-wrap: wrap; justify-content: flex-start; }
.settings-dialog-body .receiver-tools > div { flex-basis: 100%; }
.settings-dialog-footer { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-top: 1px solid var(--border); background: var(--surface-soft); }
.settings-dialog-footer > span { margin-right: auto; font-size: 11px; color: var(--text-secondary); }
.settings-dialog button.dialog-primary { background: var(--primary); color: #fff; border-color: var(--primary); font-weight: 600; }
.settings-dialog button:focus-visible, .settings-dialog input:focus-visible, .settings-dialog select:focus-visible { outline: 2px solid var(--primary); outline-offset: 3px; }
.bridge-panel { display: grid; grid-template-columns: 1fr 1fr; gap: 28px; min-height: 260px; align-items: center; }
.dialog-eyebrow { display: block; font-size: 11px; color: var(--primary); margin-bottom: 12px; letter-spacing: 1px; }
.bridge-callout { background: var(--surface-soft); border: 1px solid var(--border); border-radius: 12px; padding: 22px; display: flex; align-items: flex-start; flex-direction: column; gap: 12px; }
.bridge-callout strong { font-size: 17px; }
.bridge-callout .hint { margin: 0; }
@keyframes settings-dialog-in { from { opacity: 0; transform: scale(.96); } to { opacity: 1; transform: scale(1); } }
@media (max-width: 640px) { .settings-backdrop { padding: 12px; } .settings-dialog { width: calc(100vw - 24px); max-height: calc(100dvh - 24px); } .settings-dialog-head, .settings-dialog-body { padding: 16px; } .bridge-panel { grid-template-columns: 1fr; gap: 16px; } .settings-dialog-body .receiver-tools button { white-space: normal; } }
@media (prefers-reduced-motion: reduce) { .settings-dialog { animation: none; } }

.mapping-list-head { align-items: flex-start; flex-wrap: wrap; }
.mapping-tools { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin-left: auto; }
.mapping-tools button { padding: 7px 9px; font-size: 10px; }
.mapping-tools input { max-width: 170px; }
.trigger-picker { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 8px; }
.trigger-picker button { display: inline-flex; align-items: center; gap: 7px; padding: 7px 10px; background: var(--card-bg); }
.trigger-picker button.active { border-color: var(--primary); background: #3478f60c; color: var(--primary); }
.trigger-picker i { width: 5px; height: 5px; border-radius: 50%; background: var(--primary); }
.trigger-note { font-size: 10px; margin: 8px 0 14px; }
.interval-control { display: grid; grid-template-columns: auto auto minmax(100px,1fr); align-items: center; gap: 12px; font-size: 11px; margin-top: 18px; }
.interval-control strong { color: var(--primary); }
.interval-control input { margin: 0; }
.mouse-options { display: flex; flex-wrap: wrap; align-items: center; gap: 18px; margin-top: 12px; }
.mouse-options label { margin: 0; }
.mouse-options input[type=range] { max-width: 180px; }
.reset-confirm { padding: 12px; margin-bottom: 12px; background: var(--surface-soft); border: 1px solid var(--border); border-radius: 10px; font-size: 12px; }
.reset-confirm p { margin: 0 0 10px; line-height: 1.6; }
.reset-confirm button + button { margin-left: 8px; }

@media (min-width: 980px) and (min-height: 700px) {
 .mapping-editor:not(.expanded) { position: sticky; top: 12px; z-index: 2; background: var(--card-bg); box-shadow: 0 8px 24px #1720330b; }
}

.indicator-appearance { display: flex; flex-wrap: wrap; gap: 16px 22px; align-items: center; padding: 14px; margin-top: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-soft); }
.indicator-appearance label { display: flex; align-items: center; gap: 10px; margin: 0; }
.indicator-appearance input[type=color] { width: 46px; height: 32px; padding: 3px; cursor: pointer; }
.trail-settings { margin: 20px 0; padding: 18px; background: var(--surface-soft); border: 1px solid var(--border); border-radius: 12px; }
.trail-settings h3 { margin: 0; font-size: 15px; }
.trail-settings .section-heading { flex-wrap: wrap; gap: 12px; }
.trail-presets { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.trail-presets button { display: flex; align-items: center; gap: 7px; padding: 8px 10px; }
.trail-presets button.active { border-color: var(--primary); color: var(--primary); background: #3478f60a; }
.trail-presets i { width: 10px; height: 10px; border-radius: 50%; box-shadow: 0 0 6px currentColor; }
.trail-controls { border: 0; padding: 0; margin: 0; min-width: 0; }
.trail-controls:disabled { opacity: .5; }
.trail-color { display: flex; align-items: center; gap: 12px; margin: 14px 0; }
.trail-color input { width: 48px; height: 32px; padding: 3px; }
.trail-footer { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px; margin-top: 14px; }
.trail-footer .hint { margin: 0; }
.page-heading h1 { margin: 0; font-size: 24px; letter-spacing: -.4px; }
.header-actions { flex-direction: row; align-items: center; gap: 3px; padding: 4px; border: 1px solid var(--border); border-radius: 999px; background: var(--card-bg); box-shadow: 0 4px 16px #16203305; }
.header-actions .connection { display: flex; align-items: center; gap: 8px; padding: 9px 12px; }
.header-actions .connection.online { background: #ecfdf5; }
.header-actions button { padding: 9px 13px; border: 0; border-radius: 999px; color: var(--danger); background: #fff3f4; }
.device-data { grid-template-columns: repeat(2, minmax(0,1fr)); row-gap: 20px; }
.device-battery dd { display: flex; align-items: center; gap: 9px; }
.battery-shell { display: inline-block; width: 22px; height: 11px; border: 1.5px solid currentColor; border-radius: 3px; color: var(--text-secondary); padding: 1px; box-sizing: border-box; }
.battery-shell i { display: block; height: 100%; background: currentColor; border-radius: 1px; }
.battery-shell.is-green { color: var(--success); }.battery-shell.is-yellow { color: var(--warning); }.battery-shell.is-red { color: var(--danger); }
.remote-panel .mapping-editor { margin: 18px 0 0; padding: 14px; min-width: 0; }
.remote-panel .editor-heading { gap: 8px; align-items: flex-start; }
.remote-panel .editor-label { display: block; margin: 5px 0 0; }
.remote-panel .editor-fields { display: grid; grid-template-columns: minmax(0,1fr); gap: 10px; }
.remote-panel .editor-fields > select { width: 100%; min-width: 0; box-sizing: border-box; }
.remote-panel .editor-fields .binding-value { min-width: 0; }
.remote-panel .editor-fields .row-actions { margin-left: 0; flex-wrap: wrap; justify-content: flex-start; }
.remote-panel .trigger-picker { display: grid; grid-template-columns: repeat(3,minmax(0,1fr)); }
.remote-panel .trigger-picker button { justify-content: center; padding: 7px 5px; }
.remote-panel .mouse-options { flex-direction: column; align-items: stretch; }
.voice-extra { max-width: 100%; min-width: 0; }.voice-extra .section-heading { flex-wrap: wrap; }
.voice-extra .shortcut { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
@media(max-width:740px) { .remote-panel .mapping-editor { grid-column: 1/-1; } .header-actions { gap: 1px; } .header-actions .connection, .header-actions button { padding: 8px 10px; } }
.battery-shell { position: relative; }.battery-shell.has-power { color: var(--success); }
.battery-shell b { position: absolute; left: 7px; top: -4px; font: bold 13px/15px sans-serif; background: var(--card-bg); }
.battery-detail { display: block; margin-top: 7px; color: var(--text-secondary); font-size: 10px; font-weight: 400; }
.profile-bar{display:flex;align-items:center;justify-content:space-between;gap:18px;flex-wrap:wrap;margin-bottom:18px;padding:16px 20px}.profile-bar>div{display:grid;gap:5px}.profile-bar span,.profile-bar small{font-size:10px;color:var(--text-secondary)}.profile-bar strong{font-size:14px}.profile-bar label{margin:0;font-size:11px;display:flex;align-items:center;gap:10px}.profile-bar label select{margin:0;min-width:160px;max-width:260px}.editing-scope{display:flex;align-items:center;justify-content:space-between;gap:8px;font-size:10px;color:var(--text-secondary);margin:16px 0 0}.editing-scope button{padding:4px 7px;font-size:10px}.assignment-entries{display:flex;gap:5px;padding:4px;background:var(--surface-soft);border:1px solid var(--border);border-radius:10px}.assignment-entries button{flex:1;color:var(--primary);background:var(--card-bg);font-weight:600}.device-power dd{display:flex;align-items:center;gap:8px}.power-symbol{font-size:19px}.device-power small{display:block;margin-top:7px;font-size:10px;color:var(--text-secondary)}
.profile-bar label { white-space: nowrap; }.profile-bar label select { width: auto; }
</style>
