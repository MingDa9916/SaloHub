<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open as openFile, save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  HISENSE_BUTTONS, type HisenseAction, type HisenseConfig, type HisenseMacro,
  type HisenseProfile, type InstalledApplication, type MacroStep,
} from "../types/hisense";
import { keyLabel } from "../utils/shortcut";
import ApplicationPicker from "./ApplicationPicker.vue";
import HisenseAssignmentLibrary from "./HisenseAssignmentLibrary.vue";
import HisenseRemotePreview from "./HisenseRemotePreview.vue";

type Scope = Pick<HisenseProfile, "button_bindings" | "long_press_bindings" | "multi_click_bindings">;
type MappingDocument = Scope & {
  format: "nexus-prime-hisense-mappings";
  schema_version: 1;
  profiles: Record<string, HisenseProfile>;
  macros: Record<string, HisenseMacro>;
  profile_switching_enabled: boolean;
  fixed_profile: string | null;
  multi_click_interval_ms: number;
};
type MacroDocument = { format: "salohub-macros"; schema_version: 1; macros: Record<string, HisenseMacro> };
type Resolution = { original: string; target: string | null; message: string; profile_id?: string };
type ParseResult = { document: MappingDocument | MacroDocument; config: HisenseConfig; applications: Resolution[] };
type Trigger = 1 | 2 | 3 | 4 | "long";
type LibraryTab = "commands" | "keys" | "macros";

const props = defineProps<{ open: boolean; initialProfile?: string; initialButton?: string; initialTrigger?: Trigger }>();
const emit = defineEmits<{ close: []; applied: [config: HisenseConfig] }>();
const draft = ref<MappingDocument>();
const tab = ref<"mapping" | "macros">("mapping");
const profile = ref("default");
const button = ref("ok");
const trigger = ref<Trigger>(1);
const search = ref("");
const macroId = ref("");
const library = ref<{ tab: LibraryTab; macro?: string; manage?: boolean }>();
const picker = ref<{ row?: Resolution }>();
const busy = ref(false);
const dirty = ref(false);
const discard = ref(false);
const message = ref("");
const failed = ref(false);
const resolutions = ref<Resolution[]>([]);
const dialog = ref<HTMLElement>();
let returnFocus: HTMLElement | null = null;

const triggers: Array<[Trigger, string]> = [[1, "单击"], [2, "双击"], [3, "三击"], [4, "四连击"], ["long", "长按"]];
const simpleActions = [
  ["PointerConfirm", "确定键（随指向模式）"], ["TouchClick", "触屏点按"],
  ["TouchDrag", "按住触摸拖动"], ["MouseClick", "鼠标左键"], ["MouseRightClick", "鼠标右键"],
  ["PointerAi", "循环切换指向模式"], ["ToggleHighlighter", "荧光笔开关"], ["VoiceInput", "语音输入"],
  ["CycleProfile", "循环切换配置"], ["StopMacros", "停止所有宏"], ["FocusChatGpt", "打开并聚焦 ChatGPT"],
] as const;
const modeNames = { Once: "不重复", Hold: "按住时重复", Toggle: "触发切换", Sequence: "序列" };
const none: HisenseAction = { type: "None" };
const scope = computed(() => profile.value === "default" ? undefined : draft.value?.profiles[profile.value]);
const inherited = computed(() => !!scope.value && !(button.value in scope.value.button_bindings));
const macro = computed(() => draft.value?.macros[macroId.value]);
const profileName = computed(() => scope.value?.name ?? "桌面默认");
const buttonName = computed(() => HISENSE_BUTTONS.find(([id]) => id === button.value)?.[1] ?? button.value);
const triggerName = computed(() => triggers.find(([id]) => id === trigger.value)?.[1] ?? "");
const action = computed(() => binding(button.value, trigger.value));
const keyboardKeys = computed(() => action.value.type === "SingleKey" ? [action.value.value] : action.value.type === "ComboKey" ? action.value.value : []);
const macroStages = computed(() => {
  const value = macro.value;
  if (!value) return [];
  if (value.mode !== "Sequence") return [{ name: "执行步骤", steps: value.press }];
  return [
    { name: "按下", steps: value.press },
    { name: "按住", steps: value.hold },
    { name: "松开", steps: value.release },
  ];
});

function binding(id: string, slot: Trigger): HisenseAction {
  const current = scope.value && id in scope.value.button_bindings ? scope.value : draft.value;
  if (slot === 1) return current?.button_bindings[id] ?? none;
  if (slot === "long") return current?.long_press_bindings[id] ?? none;
  return current?.multi_click_bindings[id]?.[slot] ?? none;
}

function held(value: HisenseAction) {
  return ["PointerConfirm", "PointerAi", "TouchDrag", "VoiceInput"].includes(value.type)
    || (value.type === "Macro" && ["Hold", "Sequence"].includes(draft.value?.macros[value.value]?.mode ?? ""));
}

function canAssign(value: HisenseAction) {
  if (trigger.value === 1) return true;
  if (value.type === "None") return true;
  if (held(binding(button.value, 1))) return false;
  if (trigger.value === "long") return value.type !== "PointerConfirm" && value.type !== "PointerAi";
  return !held(value);
}

function shortPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function actionLabel(value: HisenseAction): string {
  switch (value.type) {
    case "None": return "未分配";
    case "SingleKey": return keyLabel(value.value);
    case "ComboKey": return value.value.map(keyLabel).join(" + ");
    case "TextInput": return value.value ? `输入文字 · ${value.value}` : "输入文字";
    case "LaunchApp": return value.value ? `启动 · ${shortPath(value.value)}` : "启动应用";
    case "Macro": return `宏 · ${draft.value?.macros[value.value]?.name ?? "未找到"}`;
    case "SelectProfile": return value.value
      ? `切换到 ${draft.value?.profiles[value.value]?.name ?? "桌面默认"}`
      : "按应用自动切换";
    case "MouseMove": return "移动鼠标";
    default: return simpleActions.find(([type]) => type === value.type)?.[1] ?? value.type;
  }
}

function stepLabel(step: MacroStep) {
  switch (step.type) {
    case "LaunchApp": return `启动 ${shortPath(step.value)}`;
    case "Delay": return `等待 ${step.value} ms`;
    case "KeyDown": return `按下 ${keyLabel(step.value)}`;
    case "KeyUp": return `抬起 ${keyLabel(step.value)}`;
    case "MouseDown": return step.value ? "按下鼠标右键" : "按下鼠标左键";
    case "MouseUp": return step.value ? "抬起鼠标右键" : "抬起鼠标左键";
  }
}

const buttonRows = computed(() => HISENSE_BUTTONS.map(([id, name]) => ({
  id, name, summary: actionLabel(binding(id, 1)),
  extra: triggers.slice(1).filter(([slot]) => binding(id, slot).type !== "None").length,
})).filter(row => `${row.name} ${row.summary}`.toLowerCase().includes(search.value.trim().toLowerCase())));
const macroRows = computed(() => Object.entries(draft.value?.macros ?? {}).filter(([, value]) =>
  value.name.toLowerCase().includes(search.value.trim().toLowerCase())));

function markChanged() {
  dirty.value = true;
  discard.value = false;
  failed.value = false;
  message.value = "";
}

function setAction(value: HisenseAction) {
  if (!draft.value || !canAssign(value)) {
    message.value = "这个动作与当前触发方式不兼容。";
    failed.value = true;
    return;
  }
  const id = button.value;
  const current = scope.value ?? draft.value;
  if (inherited.value) {
    current.button_bindings[id] = draft.value.button_bindings[id] ?? none;
    const long = draft.value.long_press_bindings[id];
    if (long) current.long_press_bindings[id] = long;
    const multi = draft.value.multi_click_bindings[id];
    if (multi) current.multi_click_bindings[id] = { ...multi };
  }
  if (trigger.value === 1) {
    current.button_bindings[id] = value;
    if (held(value)) {
      delete current.long_press_bindings[id];
      delete current.multi_click_bindings[id];
    }
  } else if (trigger.value === "long") {
    current.long_press_bindings[id] = value;
  } else {
    const slots = current.multi_click_bindings[id] ??= {};
    slots[trigger.value] = value;
  }
  markChanged();
}

function restoreInheritance() {
  if (!scope.value) return;
  delete scope.value.button_bindings[button.value];
  delete scope.value.long_press_bindings[button.value];
  delete scope.value.multi_click_bindings[button.value];
  trigger.value = 1;
  markChanged();
}

function updateMacros(values: Record<string, HisenseMacro>) {
  if (!draft.value) return;
  draft.value.macros = values;
  forEachAction(value => value.type === "Macro" && !Object.prototype.hasOwnProperty.call(values, value.value) ? none : value);
  if (!Object.prototype.hasOwnProperty.call(values, macroId.value)) macroId.value = Object.keys(values)[0] ?? "";
  markChanged();
}

function forEachAction(edit: (value: HisenseAction) => HisenseAction) {
  if (!draft.value) return;
  for (const current of [draft.value, ...Object.values(draft.value.profiles)]) {
    for (const bindings of [current.button_bindings, current.long_press_bindings]) {
      for (const [id, value] of Object.entries(bindings)) bindings[id] = edit(value);
    }
    for (const slots of Object.values(current.multi_click_bindings)) {
      for (const count of [2, 3, 4] as const) {
        const value = slots[count];
        if (value) slots[count] = edit(value);
      }
    }
  }
}

function chooseButton(id: string) {
  button.value = id;
  trigger.value = 1;
}

function chooseTab(value: "mapping" | "macros") {
  tab.value = value;
  search.value = "";
  if (value === "macros" && !macro.value) macroId.value = Object.keys(draft.value?.macros ?? {})[0] ?? "";
}

function selectApp(app: InstalledApplication) {
  const row = picker.value?.row;
  const path = app.launch_id ? `shell:AppsFolder\\${app.launch_id}` : app.path;
  if (!row) {
    setAction({ type: "LaunchApp", value: path });
    return;
  }
  if (!draft.value) return;
  if (row.profile_id) {
    const item = draft.value.profiles[row.profile_id];
    if (item) item.process_name = app.process_name;
  } else {
    forEachAction(value => value.type === "LaunchApp" && value.value === row.original ? { type: "LaunchApp", value: path } : value);
    for (const item of Object.values(draft.value.macros)) {
      for (const step of [...item.press, ...item.hold, ...item.release]) {
        if (step.type === "LaunchApp" && step.value === row.original) step.value = path;
      }
    }
  }
  resolutions.value = resolutions.value.map(item => item === row
    ? { ...item, target: row.profile_id ? app.process_name : path, message: "已选择本机应用" }
    : item);
  markChanged();
}

function ensureSelection() {
  if (!draft.value?.profiles[profile.value]) profile.value = "default";
  if (!macro.value) macroId.value = Object.keys(draft.value?.macros ?? {})[0] ?? "";
}

async function load() {
  busy.value = true;
  try {
    draft.value = await invoke<MappingDocument>("hisense_mapping_document", { macrosOnly: false });
    ensureSelection();
  } catch (error) {
    failed.value = true;
    message.value = `无法读取配置：${error}`;
  } finally {
    busy.value = false;
  }
}

async function importFile() {
  if (!draft.value || busy.value) return;
  busy.value = true;
  try {
    const path = await openFile({ multiple: false, filters: [{ name: "映射或宏文件", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    const text = await invoke<string>("hisense_read_mapping_file", { path });
    const result = await invoke<ParseResult>("hisense_parse_mappings", { text, apply: false });
    const imported = result.document;
    if (imported.format === "salohub-macros") {
      // The backend returns the merged library. Take only the IDs in the imported file.
      const incoming = JSON.parse(text) as MacroDocument;
      const macros = { ...draft.value.macros };
      for (const id of Object.keys(incoming.macros)) macros[id] = imported.macros[id];
      draft.value.macros = macros;
      macroId.value = Object.keys(incoming.macros)[0] ?? macroId.value;
      chooseTab("macros");
    } else {
      draft.value = imported;
      chooseTab("mapping");
    }
    ensureSelection();
    resolutions.value = result.applications;
    markChanged();
    message.value = "已载入配置预览。确认后点击“应用更改”。";
  } catch (error) {
    failed.value = true;
    message.value = `导入失败：${error}`;
  } finally {
    busy.value = false;
  }
}

async function exportFile() {
  if (!draft.value || busy.value) return;
  busy.value = true;
  try {
    const macrosOnly = tab.value === "macros";
    const path = await saveFile({
      defaultPath: macrosOnly ? "SaloHUb-macros.json" : "SaloHUb-mappings.json",
      filters: [{ name: "配置文件", extensions: ["json"] }],
    });
    if (!path) return;
    const exported: MappingDocument | MacroDocument = macrosOnly
      ? { format: "salohub-macros", schema_version: 1, macros: draft.value.macros }
      : draft.value;
    await invoke("hisense_export_document", { path, text: JSON.stringify(exported, null, 2) });
    message.value = "配置已导出。";
    failed.value = false;
  } catch (error) {
    failed.value = true;
    message.value = `导出失败：${error}`;
  } finally {
    busy.value = false;
  }
}

async function apply() {
  if (!draft.value || busy.value || !dirty.value) return;
  busy.value = true;
  try {
    const result = await invoke<ParseResult>("hisense_parse_mappings", {
      text: JSON.stringify(draft.value), apply: true, resolveApplications: false,
    });
    if (result.document.format === "nexus-prime-hisense-mappings") draft.value = result.document;
    ensureSelection();
    dirty.value = false;
    discard.value = false;
    failed.value = false;
    message.value = "已应用到遥控器。";
    emit("applied", result.config);
  } catch (error) {
    failed.value = true;
    message.value = `应用失败：${error}`;
  } finally {
    busy.value = false;
  }
}

function close() {
  if (busy.value) return;
  if (dirty.value) discard.value = true;
  else emit("close");
}

function dialogKeys(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    close();
  }
  if (event.key !== "Tab") return;
  const controls = Array.from(dialog.value?.querySelectorAll<HTMLElement>("button:not(:disabled), select:not(:disabled), input:not(:disabled), textarea:not(:disabled)") ?? [])
    .filter(control => control.getClientRects().length > 0);
  const first = controls[0], last = controls[controls.length - 1];
  if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.value)) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}

watch(() => props.open, async open => {
  if (!open) {
    library.value = undefined;
    picker.value = undefined;
    returnFocus?.focus();
    return;
  }
  returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  draft.value = undefined;
  profile.value = props.initialProfile ?? "default";
  button.value = props.initialButton ?? "ok";
  trigger.value = props.initialTrigger ?? 1;
  chooseTab("mapping");
  dirty.value = false;
  discard.value = false;
  message.value = "";
  failed.value = false;
  resolutions.value = [];
  await nextTick();
  dialog.value?.focus();
  await load();
});
</script>

<template>
  <div v-if="open" class="workbench-mask" @click.self="close">
    <section ref="dialog" class="workbench" role="dialog" aria-modal="true" aria-labelledby="workbench-title"
      tabindex="-1" :inert="!!library || !!picker" @keydown="dialogKeys">
      <header class="workbench-header">
        <div>
          <h2 id="workbench-title">按键映射与宏编辑器</h2>
          <p>选择按键、配置动作，或在宏库中编辑执行步骤。</p>
        </div>
        <button class="close" type="button" aria-label="关闭编辑器" :disabled="busy" @click="close">×</button>
      </header>

      <div class="workbench-tools">
        <nav aria-label="编辑器视图">
          <button type="button" :aria-pressed="tab === 'mapping'" @click="chooseTab('mapping')">按键映射</button>
          <button type="button" :aria-pressed="tab === 'macros'" @click="chooseTab('macros')">宏库</button>
        </nav>
        <div class="file-tools">
          <button :disabled="busy || !draft" @click="importFile">导入文件</button>
          <button :disabled="busy || !draft" @click="exportFile">{{ tab === 'macros' ? '导出宏库' : '导出映射与宏' }}</button>
        </div>
      </div>

      <div v-if="draft && tab === 'mapping'" class="workspace mapping-workspace" :inert="busy">
        <aside class="remote-panel">
          <h3>遥控器</h3>
          <HisenseRemotePreview :selected-id="button" @select="chooseButton" />
        </aside>
        <div class="row-panel">
          <div class="row-filters">
            <label>应用配置
              <select v-model="profile" @change="trigger = 1">
                <option value="default">桌面默认</option>
                <option v-for="(item, id) in draft.profiles" :key="id" :value="id">{{ item.name }}</option>
              </select>
            </label>
            <input v-model="search" type="search" aria-label="搜索遥控按键" placeholder="搜索按键或动作">
          </div>
          <div class="item-list">
            <button v-for="row in buttonRows" :key="row.id" class="list-row" :aria-pressed="button === row.id"
              @click="chooseButton(row.id)">
              <span><strong>{{ row.name }}</strong><small>{{ row.summary }}</small></span>
              <small v-if="row.extra" class="extra">{{ row.extra }} 项触发</small>
            </button>
            <p v-if="!buttonRows.length" class="empty">没有匹配的按键。</p>
          </div>
        </div>

        <div class="detail-panel">
          <div class="detail-heading">
            <div><small>{{ profileName }}</small><h3>{{ buttonName }}</h3></div>
            <button v-if="scope && !inherited" @click="restoreInheritance">恢复继承</button>
            <span v-else-if="inherited" class="tag">继承桌面</span>
          </div>
          <div class="trigger-list" aria-label="触发方式">
            <button v-for="[slot, label] in triggers" :key="slot" :aria-pressed="trigger === slot"
              :disabled="slot !== 1 && held(binding(button, 1))" @click="trigger = slot">{{ label }}</button>
          </div>
          <div class="action-preview">
            <small>{{ triggerName }}动作</small>
            <strong>{{ actionLabel(action) }}</strong>
          </div>
          <div class="action-buttons">
            <button @click="library = { tab: 'keys' }">键盘 / 组合键</button>
            <button @click="library = { tab: 'commands' }">常用命令</button>
            <button @click="library = { tab: 'macros' }">选择宏</button>
            <button @click="picker = {}">启动应用</button>
          </div>
          <label>其他动作
            <select :value="simpleActions.some(([type]) => type === action.type) ? action.type : ''" @change="event => {
              const type = (event.target as HTMLSelectElement).value;
              const option = simpleActions.find(([id]) => id === type);
              if (option) setAction({ type: option[0] });
            }">
              <option value="" disabled>选择鼠标、触摸或遥控器功能</option>
              <option v-for="[type, label] in simpleActions" :key="type" :value="type" :disabled="!canAssign({ type })">{{ label }}</option>
            </select>
          </label>
          <button v-if="action.type !== 'TextInput'" @click="setAction({ type: 'TextInput', value: '' })">输入文字</button>
          <label v-else>要输入的文字
            <textarea :value="action.value" rows="3" maxlength="4096"
              @input="setAction({ type: 'TextInput', value: ($event.target as HTMLTextAreaElement).value })" />
          </label>
          <button v-if="action.type !== 'MouseMove'" @click="setAction({ type: 'MouseMove', value: { dx: 0, dy: -1, step: 12, accelerate: false } })">移动鼠标</button>
          <div v-else class="motion-fields">
            <label>方向
              <select :value="`${action.value.dx},${action.value.dy}`" @change="event => {
                if (action.type !== 'MouseMove') return;
                const [dx, dy] = (event.target as HTMLSelectElement).value.split(',').map(Number) as [-1|0|1, -1|0|1];
                setAction({ type: 'MouseMove', value: { ...action.value, dx, dy } });
              }">
                <option value="0,-1">向上</option><option value="0,1">向下</option>
                <option value="-1,0">向左</option><option value="1,0">向右</option>
              </select>
            </label>
            <label>步长 {{ action.value.step }} px
              <input type="range" min="1" max="100" :value="action.value.step" @input="event => {
                if (action.type === 'MouseMove') setAction({ type: 'MouseMove', value: { ...action.value, step: Number((event.target as HTMLInputElement).value) } });
              }">
            </label>
            <label class="inline"><input type="checkbox" :checked="action.value.accelerate" @change="event => {
              if (action.type === 'MouseMove') setAction({ type: 'MouseMove', value: { ...action.value, accelerate: (event.target as HTMLInputElement).checked } });
            }"> 按住加速</label>
          </div>
          <button class="clear" :disabled="action.type === 'None'" @click="setAction(none)">清除当前动作</button>
        </div>
      </div>

      <div v-else-if="draft" class="workspace macro-workspace" :inert="busy">
        <div class="row-panel">
          <div class="row-filters">
            <button class="primary" @click="library = { tab: 'macros', macro: 'new', manage: true }">＋ 新建宏</button>
            <input v-model="search" type="search" aria-label="搜索宏" placeholder="搜索宏名称">
          </div>
          <div class="item-list">
            <button v-for="[id, item] in macroRows" :key="id" class="list-row" :aria-pressed="macroId === id"
              @click="macroId = id">
              <i class="macro-color" :style="{ background: item.color }" />
              <span><strong>{{ item.name }}</strong><small>{{ modeNames[item.mode] }}</small></span>
            </button>
            <p v-if="!macroRows.length" class="empty">{{ search ? '没有匹配的宏。' : '还没有宏，可点击“新建宏”。' }}</p>
          </div>
        </div>
        <div v-if="macro" class="detail-panel">
          <div class="detail-heading">
            <div><small>{{ modeNames[macro.mode] }}</small><h3>{{ macro.name }}</h3></div>
            <button class="primary" @click="library = { tab: 'macros', macro: macroId, manage: true }">编辑步骤</button>
          </div>
          <div v-for="stage in macroStages" :key="stage.name" class="macro-stage">
            <h4>{{ stage.name }} <small>{{ stage.steps.length }} 步</small></h4>
            <ol v-if="stage.steps.length">
              <li v-for="(step, index) in stage.steps" :key="index">{{ stepLabel(step) }}</li>
            </ol>
            <p v-else>此阶段没有步骤。</p>
          </div>
        </div>
        <p v-else class="empty">选择或创建一个宏，查看执行步骤。</p>
      </div>
      <p v-else-if="busy" class="empty" role="status">正在读取配置…</p>
      <button v-else class="retry" @click="load">重新读取配置</button>

      <details v-if="resolutions.length" class="associations" :inert="busy">
        <summary>本机应用关联 · {{ resolutions.length }} 项
          <small v-if="resolutions.some(row => !row.target)">有应用待确认</small>
        </summary>
        <div class="association-list">
          <div v-for="row in resolutions" :key="row.profile_id ?? row.original" class="association-row">
            <div><strong>{{ row.original }}</strong><small>{{ row.target || row.message }}</small></div>
            <button v-if="row.original !== '应用自动关联'" @click="picker = { row }">选择本机应用</button>
          </div>
        </div>
      </details>

      <footer>
        <div v-if="discard" class="discard">
          <span>当前更改尚未应用。</span>
          <button @click="discard = false">继续编辑</button>
          <button @click="emit('close')">放弃并关闭</button>
        </div>
        <span v-else class="status" :class="{ failed }" role="status">{{ message || (dirty ? '有未应用的更改' : '已同步当前配置') }}</span>
        <button class="primary" :disabled="busy || !dirty || !draft" @click="apply">{{ busy ? '处理中…' : '应用更改' }}</button>
      </footer>
    </section>

    <HisenseAssignmentLibrary :open="!!library" :initial-tab="library?.tab ?? 'commands'" :inside-workbench="true"
      :manage-macros="library?.manage" :open-macro="library?.macro" :initial-keys="keyboardKeys"
      :macros="draft?.macros ?? {}" :profiles="draft?.profiles ?? {}" :button-label="buttonName" :slot-label="triggerName"
      :base-slot="trigger === 1" :allow-held="trigger === 1 || trigger === 'long'" :disabled="busy"
      @close="library = undefined" @assign="setAction" @macros-change="updateMacros" />
    <ApplicationPicker :open="!!picker" @close="picker = undefined" @select="selectApp" />
  </div>
</template>

<style scoped>
.workbench-mask {
  position: fixed; inset: 0; z-index: 3600; background: var(--overlay);
  display: grid; place-items: center; padding: 20px;
}
.workbench {
  width: min(1190px, 96vw); height: min(810px, 93dvh); display: flex; flex-direction: column;
  background: var(--card-bg); color: var(--text); border: 1px solid var(--border);
  border-radius: 16px; box-shadow: var(--dialog-shadow); overflow: hidden;
}
.workbench-header, .workbench-tools, footer {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 16px 22px; border-bottom: 1px solid var(--border);
}
.workbench-header h2 { margin: 0; font-size: 20px; }
.workbench-header p { margin: 5px 0 0; color: var(--text-secondary); font-size: 12px; }
h3 { margin: 0; font-size: 16px; }
small { color: var(--text-secondary); font-size: 11px; }
nav, .file-tools, .discard { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
nav { padding: 3px; border-radius: 10px; background: var(--surface-soft); }
nav button { border-color: transparent; background: transparent; }
.close { padding: 5px 12px; font-size: 20px; }
.workspace { display: grid; flex: 1; min-height: 0; overflow: hidden; }
.mapping-workspace { grid-template-columns: minmax(110px, 170px) minmax(170px, 1fr) minmax(260px, 1.02fr); }
.macro-workspace { grid-template-columns: minmax(210px, .65fr) minmax(260px, 1fr); }
.remote-panel, .row-panel { min-height: 0; min-width: 0; border-right: 1px solid var(--border); }
.remote-panel { overflow: auto; padding: 16px; text-align: center; background: var(--surface-soft); }
.remote-panel h3 { margin-bottom: 14px; font-size: 13px; }
.row-panel { display: flex; flex-direction: column; }
.row-filters { display: grid; gap: 10px; padding: 16px; border-bottom: 1px solid var(--border); }
.row-filters input { width: 100%; }
.item-list { min-height: 0; overflow: auto; padding: 8px; }
.list-row {
  display: flex; align-items: center; gap: 10px; justify-content: space-between;
  width: 100%; padding: 10px; text-align: left; border-color: transparent;
}
.list-row span { min-width: 0; flex: 1; display: grid; gap: 3px; }
.list-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.list-row .extra { flex: none; }
.macro-color { flex: none; width: 10px; height: 10px; border-radius: 50%; }
.detail-panel { min-width: 0; min-height: 0; overflow: auto; padding: 19px; display: grid; align-content: start; gap: 14px; }
.detail-heading { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
.detail-heading h3 { margin-top: 3px; overflow-wrap: anywhere; }
.tag { color: var(--primary); font-size: 11px; }
.trigger-list { display: flex; flex-wrap: wrap; gap: 5px; }
.trigger-list button { flex: 1; padding-inline: 6px; white-space: nowrap; }
.action-preview { display: grid; gap: 6px; padding: 13px; background: var(--surface-soft); border-radius: 9px; }
.action-preview strong { overflow-wrap: anywhere; max-height: 4.5em; overflow: auto; }
.action-buttons { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 7px; }
.detail-panel > button { justify-self: start; }
.motion-fields { display: grid; gap: 8px; }
label { display: grid; gap: 5px; font-size: 12px; }
label.inline { display: flex; align-items: center; }
button, select, input, textarea {
  font: inherit; font-size: 12px; padding: 8px 10px; box-sizing: border-box;
  color: var(--text); background: var(--card-bg); border: 1px solid var(--border); border-radius: 8px;
}
select { width: 100%; min-width: 0; }
button { cursor: pointer; }
button[aria-pressed="true"], button:hover:not(:disabled) { color: var(--primary); background: var(--surface-soft); border-color: var(--primary); }
button:disabled { opacity: .48; cursor: default; }
button:focus-visible, input:focus-visible, select:focus-visible, textarea:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
button.primary { color: white; background: var(--primary); border-color: var(--primary); }
.clear, .failed { color: var(--danger-text); }
.empty { padding: 14px; color: var(--text-secondary); font-size: 12px; }
.retry { align-self: center; margin: 24px; }
.macro-stage { border: 1px solid var(--border); border-radius: 10px; padding: 12px; }
.macro-stage h4 { margin: 0 0 8px; }
.macro-stage ol { margin: 0; padding-left: 26px; }
.macro-stage li { padding: 5px 0; }
.macro-stage p { margin: 0; color: var(--text-secondary); }
.associations { flex: none; padding: 9px 22px; border-top: 1px solid var(--border); }
.associations summary { cursor: pointer; font-size: 12px; }
.associations summary small { margin-left: 8px; }
.association-list { max-height: min(140px, 20dvh); overflow: auto; }
.association-row { display: flex; align-items: start; justify-content: space-between; gap: 10px; padding: 7px 0; }
.association-row div { display: grid; gap: 4px; font-size: 12px; overflow-wrap: anywhere; min-width: 0; }
.association-row button { flex: none; }
footer { border-top: 1px solid var(--border); border-bottom: 0; }
.status, .discard { font-size: 12px; overflow-wrap: anywhere; }
footer > button { flex: none; }
@media (max-width: 740px) {
  .mapping-workspace { grid-template-columns: minmax(130px, .65fr) minmax(210px, 1fr); }
  .remote-panel { display: none; }
  .workbench-header, .workbench-tools, footer { padding: 10px 14px; }
  .detail-panel { padding: 12px; }
}
</style>
