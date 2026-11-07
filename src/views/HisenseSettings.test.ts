import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import HisenseSettings from "./HisenseSettings.vue";
import { HISENSE_BUTTONS, type HisenseConfig } from "../types/hisense";

const { invoke, openFile, saveFile, listen, unlisten } = vi.hoisted(() => ({ invoke: vi.fn(), openFile: vi.fn(), saveFile: vi.fn(), listen: vi.fn(), unlisten: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openFile, save: saveFile }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
enableAutoUnmount(afterEach);
let stored: HisenseConfig;
beforeEach(() => {
  vi.useFakeTimers(); window.localStorage.clear();
  stored = {
    macros: {}, profiles: {}, profile_switching_enabled: true, fixed_profile: null,
    auto_connect: true, pointer_enabled: true, touch_indicator_enabled: true, indicator_color: "#ffffff", indicator_pressed_color: "#3b82f6", highlight_trail_enabled: true, highlight_trail_color: "#22d3ee", highlight_trail_width_px: 6, highlight_trail_duration_ms: 650, highlight_trail_opacity_percent: 66, highlight_trail_glow_percent: 50, pointer_hold_ms: 350, pointer_gesture_revision: 2, sensitivity: 2, adaptive_sensitivity: true, precision_ratio: 0.25, smoothing_ms: 18, deadzone_pixels: 1.5,
    long_press_bindings: {}, multi_click_bindings: {}, multi_click_interval_ms: 300,
    button_bindings: Object.fromEntries(HISENSE_BUTTONS.map(([id]) => [id, { type: "None" }])),
    button_sources: { "key:28": "ok" }, voice_keys: [0xa2, 0xa0, 0x44], voice_profile: "codex",
    voice_mode: "Hold", microphone_name: null, microphone_gain_db: 0,
  };
  openFile.mockReset(); saveFile.mockReset(); unlisten.mockReset(); listen.mockReset().mockResolvedValue(unlisten);
  invoke.mockReset().mockImplementation(async (command: string, args?: { config: HisenseConfig }) => {
    if (command === "hisense_config") return structuredClone(stored);
    if (command === "hisense_save_config") { stored = structuredClone(args!.config); return; }
    if (command === "get_audio_devices") return [];
    if (command === "hisense_status") return { running: true, connected: true, observed_buttons: [], learning_button: null, voice_active: false, voice_level: 0 };
  });
});
afterEach(() => { vi.useRealTimers(); });
async function open() {
  const wrapper = mount(HisenseSettings, { attachTo: document.body, global: { plugins: [createPinia()], stubs: { Teleport: true } } });
  await flushPromises(); return wrapper;
}
describe("Hisense independent mapping", () => {
  it("keeps the editor, preview and list in sync when an inherited app key gets its first override", async () => {
    stored.button_bindings.up = { type: "SingleKey", value: 0x26 };
    stored.profiles = { browser: { name: "Chrome", process_name: "chrome.exe", enabled: true, button_bindings: {}, long_press_bindings: {}, multi_click_bindings: {} } };
    const view = await open();
    await view.get('[aria-label="编辑应用配置"]').setValue("browser");
    await view.get('[data-button-id="up"]').trigger("click");
    expect(view.get('.mapping-editor .binding-value').text()).toBe("上方向键");
    expect(view.get('[data-button-id="up"] .key-binding').text()).toBe("上方向键");
    await view.get('.assignment-entries button:nth-child(2)').trigger("click");
    await view.get('[aria-label="搜索分配功能"]').setValue("Page Up");
    await view.get('.key-catalog button').trigger("click");
    expect(view.get('.mapping-editor .binding-value').text()).toBe("Page Up");
    expect(view.get('[data-button-id="up"] .key-binding').text()).toBe("Page Up");
    expect(view.get('.preview-caption small').text()).toBe("Page Up");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.profiles.browser.button_bindings.up).toEqual({ type: "SingleKey", value: 0x21 });
    expect(stored.button_bindings.up).toEqual({ type: "SingleKey", value: 0x26 });
    await view.get('.editing-scope button').trigger("click");
    expect(view.get('.mapping-editor .binding-value').text()).toBe("上方向键");
    expect(view.get('[data-button-id="up"] .key-binding').text()).toBe("上方向键");
    expect(view.get('.preview-caption small').text()).toBe("上方向键");
  });
  it("assigns commands and keypad Enter from the new libraries",async()=>{
    const view=await open();await view.get('[data-button-id="menu"]').trigger('click');
    await view.get('.assignment-entries button:nth-child(1)').trigger('click');
    await view.get('[aria-label="搜索分配功能"]').setValue('复制');
    await view.get('.catalog-item').trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.button_bindings.menu).toEqual({type:'ComboKey',value:[0xa2,0x43]});
    await view.get('.assignment-entries button:nth-child(2)').trigger('click');await view.get('[aria-label="搜索分配功能"]').setValue('Num Enter');
    await view.get('.key-catalog button').trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.button_bindings.menu).toEqual({type:'SingleKey',value:0x10d});expect(stored.button_bindings.ok.type).toBe('None');
  });
  it("stops keyboard macro recording when the library closes without saving a draft",async()=>{
    const original=invoke.getMockImplementation()!;
    invoke.mockImplementation((command,args)=>command==='hisense_macro_record_start'?Promise.resolve(11):command==='hisense_macro_record_poll'?Promise.resolve({active:!(args as unknown as {stop:boolean}).stop,steps:[{type:'KeyDown',value:65},{type:'KeyUp',value:65}]}):original(command,args));
    const view=await open();await view.get('.assignment-entries button:nth-child(3)').trigger('click');await view.get('.library-body aside button.primary').trigger('click');
    await view.get('.step-tools button').trigger('click');await flushPromises();await vi.advanceTimersByTimeAsync(201);await flushPromises();
    expect(view.findAll('.step-list li')).toHaveLength(2);
    await view.get('[aria-label="关闭功能库"]').trigger('click');await flushPromises();
    expect(invoke).toHaveBeenCalledWith('hisense_macro_record_poll',{session:11,stop:true});expect(Object.keys(stored.macros!)).toHaveLength(0);
  });
  it("saves the three independent stages of a sequence macro",async()=>{
    const view=await open();await view.get('.assignment-entries button:nth-child(3)').trigger('click');await view.get('.library-body aside button.primary').trigger('click');
    await view.get('.mode-options button:nth-child(4)').trigger('click');
    await view.findAll('.step-tools button').find(b=>b.text()==='添加按键')!.trigger('click');
    await view.get('.segment-tabs button:nth-child(2)').trigger('click');await view.findAll('.step-tools button').find(b=>b.text()==='添加鼠标点按')!.trigger('click');
    await view.get('.segment-tabs button:nth-child(3)').trigger('click');await view.get('[aria-label="宏步骤按键"]').setValue(66);await view.findAll('.step-tools button').find(b=>b.text()==='添加按键')!.trigger('click');
    await view.get('.macro-editor footer .primary').trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    const m=Object.values(stored.macros!)[0];expect(m.mode).toBe('Sequence');expect(m.press).toHaveLength(2);expect(m.hold).toEqual([{type:'MouseDown',value:false},{type:'MouseUp',value:false}]);expect(m.release[0]).toEqual({type:'KeyDown',value:66});
  });
  it("creates an app and clears a fixed selection when that app is disabled",async()=>{
    const view=await open();await view.get('.profile-bar>button').trigger('click');await view.get('.profile-tools button').trigger('click');
    await view.get('[aria-label="应用配置名称"]').setValue('Firefox');await view.get('[aria-label="应用进程名"]').setValue('firefox.exe');await view.get('.profile-form').trigger('submit');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    const id=Object.keys(stored.profiles!)[0];expect(stored.profiles![id].process_name).toBe('firefox.exe');expect(stored.profiles![id].button_bindings).toEqual({});
    await view.get('[aria-label="固定使用配置文件"]').setValue(id);await vi.advanceTimersByTimeAsync(351);await flushPromises();expect(stored.fixed_profile).toBe(id);
    await view.findAll('.profile-actions button').find(b=>b.text()==='停用')!.trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();expect(stored.fixed_profile).toBeNull();expect(stored.profiles![id].enabled).toBe(false);
  });
  it("creates, edits and assigns a reusable macro",async()=>{
    const view=await open();await view.get('[data-button-id="shortcut"]').trigger('click');await view.get('.assignment-entries button:nth-child(3)').trigger('click');
    await view.get('.library-body aside button.primary').trigger('click');await view.get('[aria-label="宏名称"]').setValue('My macro');
    await view.findAll('.step-tools button').find(b=>b.text()==='添加按键')!.trigger('click');
    await view.get('.macro-editor footer .primary').trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    const id=Object.keys(stored.macros!)[0];expect(stored.macros![id].press).toEqual([{type:'KeyDown',value:65},{type:'KeyUp',value:65}]);expect(stored.button_bindings.shortcut).toEqual({type:'Macro',value:id});
    expect(view.get('.mapping-editor').text()).toContain('My macro');
  });
  it("edits an app override independently and restores inheritance",async()=>{
    stored.profiles={browser:{name:'Browser',process_name:'chrome.exe',enabled:true,button_bindings:{},long_press_bindings:{},multi_click_bindings:{}}};
    stored.button_bindings.menu={type:'SingleKey',value:65};
    const view=await open();await view.get('[aria-label="编辑应用配置"]').setValue('browser');await view.get('[data-button-id="menu"]').trigger('click');
    await view.get('.mapping-editor .action-select').setValue('ToggleHighlighter');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.button_bindings.menu).toEqual({type:'SingleKey',value:65});expect(stored.profiles!.browser.button_bindings.menu).toEqual({type:'ToggleHighlighter'});
    await view.get('.editing-scope button').trigger('click');await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.profiles!.browser.button_bindings.menu).toBeUndefined();expect(view.get('.mapping-editor').text()).toContain('A');
  });
  it("manages profile switching and applies native selections without saving them back",async()=>{
    const view=await open();await view.get('.profile-bar>button').trigger('click');await view.get('[aria-label="启用配置文件切换"]').setValue(false);await vi.advanceTimersByTimeAsync(351);await flushPromises();expect(stored.profile_switching_enabled).toBe(false);
    const event=listen.mock.calls.find(([name])=>name==='hisense-profile-settings')![1];invoke.mockClear();event({payload:{fixed_profile:'default',profile_switching_enabled:true}});await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(invoke.mock.calls.some(([c])=>c==='hisense_save_config')).toBe(false);expect((view.get('[aria-label="启用配置文件切换"]').element as HTMLInputElement).checked).toBe(true);
  });

  it("places the editor underneath the preview and shows the matching console heading", async () => {
    const view = await open();
    expect(view.get(".page-heading h1").text()).toBe("设备控制台");
    expect(view.find(".mapping-list-panel .mapping-editor").exists()).toBe(false);
    await view.get('.hisense-remote [aria-label="选择AI键"]').trigger("click");
    expect(view.get(".remote-panel .mapping-editor").attributes("data-editing-button")).toBe("ai");
    const preview = view.get(".remote-panel .hisense-remote").element;
    const editor = view.get(".remote-panel .mapping-editor").element;
    expect(preview.compareDocumentPosition(editor) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
  it.each([[true,true,"充电电源已连接"],[false,true,"充电电源未连接"],[null,true,"待更新"],[true,false,"—"]])("shows charging status only %s connected %s",async(charging_power_connected,connected,text)=>{
    const original=invoke.getMockImplementation()!;
    invoke.mockImplementation((command,args)=>command==="hisense_status"?Promise.resolve({running:true,connected,charging_power_connected,battery_level:88,observed_buttons:[],learning_button:null,voice_active:false,voice_level:0}):original(command,args));
    const view=await open();expect(view.get(".device-power dd").text()).toContain(text);expect(view.get(".device-power").text()).not.toContain("88%");expect(view.text()).not.toContain("电量未上报");
  });
  it.each([
    ["左 Ctrl", "qianwen-left-ctrl", [0xa2]],
    ["左 Ctrl + 左 Win", "qianwen-left-ctrl-win", [0xa2,0x5b]],
    ["左 Win + 左 Alt", "qianwen-left-win-alt", [0x5b,0xa4]],
    ["右 Alt", "qianwen", [0xa5]],
  ])("uses Xiaomi's input-method cards to apply Qianwen %s only to Hisense", async (label, profile, keys) => {
    const view = await open();
    await view.get(".quick-grid button:nth-child(2)").trigger("click");
    await view.get("#ime-tab-qianwen").trigger("click");
    expect(view.findAll('.ime-provider-tabs [role="tab"]').map(tab => tab.text())).toEqual(["ChatGPT", "微信", "千问", "豆包"]);
    const card = view.findAll(".ime-qianwen-option").find(card => card.get("strong").text() === label)!;
    await card.get("button").trigger("click"); await flushPromises();
    expect(stored.voice_keys).toEqual(keys); expect(stored.voice_profile).toBe(profile); expect(stored.voice_mode).toBe("Hold");
    expect(stored.button_bindings.mic).toEqual({ type: "VoiceInput" });
    expect(stored.button_bindings.home).toEqual({ type: "None" });
    expect(card.text()).toContain("当前已应用");
    expect(invoke.mock.calls.some(([c]) => c === "save_config" || c === "update_key_mapping")).toBe(false);
    await view.get('[aria-label="关闭输入法设置"]').trigger("click"); await flushPromises();
    await view.get(".quick-grid button:nth-child(2)").trigger("click");
    expect(view.get('#ime-tab-qianwen').attributes('aria-selected')).toBe('true');
    expect(view.findAll('.ime-qianwen-option').find(card => card.get('strong').text() === label)!.text()).toContain('当前已应用');
  });
  it("offers two modes and binds a highlighter toggle without replacing other mappings", async () => {
    const view = await open();
    await view.get('[data-button-id="shortcut"]').trigger("click");
    const options = view.get(".mapping-editor .action-select").text();
    expect(options).toContain("循环切换两种模式");
    expect(options).not.toContain("三种模式");
    await view.get(".mapping-editor .action-select").setValue("ToggleHighlighter");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.button_bindings.shortcut).toEqual({ type: "ToggleHighlighter" });
    expect(stored.button_bindings.ok).toEqual({ type: "None" });
  });
  it("applies a trail preset, saves custom controls, and resets only trail preferences", async () => {
    const view = await open();
    await view.get(".quick-grid button:first-child").trigger("click");
    expect((view.get('[aria-label="开启荧光轨迹"]').element as HTMLInputElement).checked).toBe(true);
    await view.get('[aria-label="触摸圆点颜色"]').setValue("#ffaa00");
    await view.get(".trail-presets button:nth-child(3)").trigger("click");
    await view.get('[aria-label="荧光轨迹时长"]').setValue("1800");
    await view.get('[aria-label="荧光轨迹粗细"]').setValue("18");
    await view.get('[aria-label="荧光轨迹不透明度"]').setValue("90");
    await view.get('[aria-label="荧光轨迹发光强度"]').setValue("100");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.highlight_trail_color).toBe("#facc15");
    expect([stored.highlight_trail_width_px, stored.highlight_trail_duration_ms, stored.highlight_trail_opacity_percent, stored.highlight_trail_glow_percent]).toEqual([18,1800,90,100]);
    await view.get(".trail-footer button").trigger("click");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.indicator_color).toBe("#ffaa00");
    expect(stored.highlight_trail_enabled).toBe(true);
    expect([stored.highlight_trail_width_px, stored.highlight_trail_duration_ms, stored.highlight_trail_opacity_percent, stored.highlight_trail_glow_percent]).toEqual([6,650,66,50]);
  });
  it("reflects remote trail toggles without saving back or discarding pending edits", async () => {
    const view = await open();
    await view.get(".quick-grid button:first-child").trigger("click");
    const onEvent = listen.mock.calls.find(([name]) => name === "hisense-visual-settings")![1];
    invoke.mockClear();
    onEvent({ payload: { highlight_trail_enabled: false, touch_indicator_enabled: true } });
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect((view.get('[aria-label="开启荧光轨迹"]').element as HTMLInputElement).checked).toBe(false);
    expect(invoke.mock.calls.some(([c]) => c === "hisense_save_config")).toBe(false);
    await view.get('[aria-label="防抖强度"]').setValue("24");
    onEvent({ payload: { highlight_trail_enabled: true, touch_indicator_enabled: true } });
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.smoothing_ms).toBe(24);
    expect(stored.highlight_trail_enabled).toBe(true);
    view.unmount(); expect(unlisten).toHaveBeenCalledTimes(2);
  });
  it("opens each quick action in a matching modal and restores focus on Escape", async () => {
    const view = await open();
    expect(view.find('[role="dialog"]').exists()).toBe(false);
    const cards = view.findAll(".quick-grid button");
    const titles = ["指向与防抖", "输入法设置", "按键桥接", "接收器管理"];
    for (const [index, title] of titles.entries()) {
      (cards[index].element as HTMLButtonElement).focus();
      await cards[index].trigger("click"); await flushPromises();
      const dialog = view.get('[role="dialog"]');
      expect(view.get(index === 1 ? "#ime-settings-title" : "#hisense-settings-title").text()).toBe(title);
      expect(dialog.attributes("aria-modal")).toBe("true");
      if (index === 1) expect(document.activeElement?.getAttribute("role")).toBe("tab"); else expect(document.activeElement).toBe(dialog.element);
      expect(invoke.mock.calls.some(([command]) => command === "hisense_start" || command === "hisense_stop" || command === "hisense_setup_receiver")).toBe(false);
      await dialog.trigger("keydown", { key: "Escape" }); await flushPromises();
      expect(view.find('[role="dialog"]').exists()).toBe(false);
      expect(document.activeElement).toBe(cards[index].element);
    }
  });
  it("keeps edits when a settings popup closes and reopens, and stops voice shortcut capture on close", async () => {
    const view = await open();
    await view.get(".quick-grid button:first-child").trigger("click");
    await view.get('[aria-label="防抖强度"]').setValue("29");
    await view.get('[aria-label="关闭指向与防抖"]').trigger("click"); await flushPromises();
    expect(stored.smoothing_ms).toBe(29);
    await view.get(".quick-grid button:first-child").trigger("click");
    expect((view.get('[aria-label="防抖强度"]').element as HTMLInputElement).value).toBe("29");
    await view.get('[aria-label="关闭指向与防抖"]').trigger("click"); await flushPromises();
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) => command === "capture_shortcut_start" ? Promise.resolve(7) : original(command, args));
    await view.get(".quick-grid button:nth-child(2)").trigger("click");
    await view.get(".voice-extra .shortcut button").trigger("click"); await flushPromises();
    await view.get('[aria-label="关闭输入法设置"]').trigger("click"); await flushPromises();
    expect(invoke).toHaveBeenCalledWith("capture_shortcut_stop", { sessionId: 7 });
    expect(view.find('[role="dialog"]').exists()).toBe(false);
  });
  it("saves each trigger slot separately, including text, mouse movement and long-hold voice", async () => {
    const view = await open();
    await view.get('[data-button-id="menu"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("Keyboard");
    await view.get('[data-trigger="2"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("TextInput");
    await view.get('[aria-label="菜单动作内容"]').setValue("double only");
    await view.get('[data-trigger="3"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("FocusChatGpt");
    await view.get('[data-trigger="4"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("MouseMove");
    await view.get('[aria-label="鼠标移动方向"]').setValue("-1,0");
    await view.get('[aria-label="鼠标移动步幅"]').setValue("17");
    await view.get('[data-trigger="long"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("VoiceInput");
    await view.get('[aria-label="海信连击间隔"]').setValue("450");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.button_bindings.menu).toEqual({ type: "SingleKey", value: 32 });
    expect(stored.multi_click_bindings?.menu?.[2]).toEqual({ type: "TextInput", value: "double only" });
    expect(stored.multi_click_bindings?.menu?.[3]).toEqual({ type: "FocusChatGpt" });
    expect(stored.multi_click_bindings?.menu?.[4]).toEqual({ type: "MouseMove", value: { dx: -1, dy: 0, step: 17, accelerate: true } });
    expect(stored.long_press_bindings?.menu).toEqual({ type: "VoiceInput" });
    expect(stored.multi_click_interval_ms).toBe(450);
    expect(stored.button_bindings.ok.type).toBe("None");
  });
  it("keeps realtime UWB controls immediate and clears incompatible advanced slots on purpose", async () => {
    const view = await open();
    await view.get('[data-trigger="2"]').trigger("click");
    expect(view.get(".mapping-editor .action-select").findAll("option").some(o => o.attributes("value") === "VoiceInput")).toBe(false);
    await view.get(".mapping-editor .action-select").setValue("Keyboard");
    await view.get('[data-trigger="1"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("PointerConfirm");
    expect(view.get('[data-trigger="2"]').attributes("disabled")).toBeDefined();
    expect(view.get('[data-trigger="long"]').attributes("disabled")).toBeDefined();
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.button_bindings.ok.type).toBe("PointerConfirm");
    expect(stored.multi_click_bindings?.ok).toBeUndefined();
  });
  it("records a shortcut into the captured long-press slot without changing its base action", async () => {
    const view = await open(); const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) => command === "capture_shortcut_start" ? Promise.resolve(9) : command === "capture_shortcut_poll" ? Promise.resolve({ sessionId: 9, keys: [0xa2, 0x44] }) : original(command, args));
    await view.get('[data-button-id="menu"]').trigger("click");
    await view.get('[data-trigger="long"]').trigger("click");
    await view.get(".mapping-editor .action-select").setValue("Keyboard");
    const record = view.findAll(".mapping-editor .row-actions button").find(b => b.text() === "录入")!;
    await record.trigger("click"); await flushPromises();
    await vi.advanceTimersByTimeAsync(71); await flushPromises();
    expect(stored.long_press_bindings?.menu).toEqual({ type: "ComboKey", value: [0xa2, 0x44] });
    expect(stored.button_bindings.menu.type).toBe("None");
    expect(invoke).toHaveBeenCalledWith("capture_shortcut_stop", { sessionId: 9 });
  });
  it("uses Hisense-only export/import commands and preserves edits when import fails", async () => {
    const view = await open();
    saveFile.mockResolvedValue("mapping-test.json");
    await view.get(".mapping-tools button:first-child").trigger("click"); await flushPromises();
    expect(invoke).toHaveBeenCalledWith("hisense_export_mappings", { path: "mapping-test.json" });
    openFile.mockResolvedValue("mapping-test.json"); const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) => command === "hisense_import_mappings" ? Promise.reject("不支持的映射格式") : original(command, args));
    await view.get(".mapping-tools button:nth-child(2)").trigger("click"); await flushPromises();
    expect(view.text()).toContain("导入失败，当前设置已保留");
    expect(stored.button_sources).toEqual({ "key:28": "ok" });
    expect(invoke.mock.calls.some(([c]) => c === "import_xiaomi_mappings" || c === "export_xiaomi_mappings")).toBe(false);
  });
  it("saves point colors and optional fluorescent trail without changing key actions", async () => {
    const view = await open();
    await view.get(".quick-grid button:first-child").trigger("click");
    await view.get('[aria-label="触摸圆点颜色"]').setValue("#ff9900");
    await view.get('[aria-label="触摸按下颜色"]').setValue("#ff33aa");
    await view.get('[aria-label="开启荧光轨迹"]').setValue(true);
    await view.get('[aria-label="荧光轨迹颜色"]').setValue("#44ff88");
    await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.indicator_color).toBe("#ff9900");expect(stored.indicator_pressed_color).toBe("#ff33aa");
    expect(stored.highlight_trail_enabled).toBe(true);expect(stored.highlight_trail_color).toBe("#44ff88");
    expect(stored.button_bindings.ok.type).toBe("None");
    await view.get('[aria-label="开启荧光轨迹"]').setValue(false);
    await vi.advanceTimersByTimeAsync(351);await flushPromises();
    expect(stored.highlight_trail_enabled).toBe(false);
  });
  it("selects and highlights a physical button from the photo and edits the same mapping", async () => {
    const view = await open();
    const ai = view.get('.hisense-remote [aria-label="选择AI键"]');
    await ai.trigger("click");
    expect(ai.attributes("aria-pressed")).toBe("true");
    expect(view.get('[data-button-id="ai"]').classes()).toContain("selected");
    expect(view.get(".mapping-editor").attributes("data-editing-button")).toBe("ai");
    await view.get(".mapping-editor select").setValue("VoiceInput");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.button_bindings.ai.type).toBe("VoiceInput");
    expect(stored.button_bindings.ok.type).toBe("None");
    await view.get('[data-button-id="right"]').trigger("click");
    expect(view.get('.hisense-remote [aria-label="选择右键"]').attributes("aria-pressed")).toBe("true");
    await view.get('.channel-switch button').trigger("click");
    expect(view.get(".mapping-editor").attributes("data-editing-button")).toBe("pointer_ok");
    expect(view.get('.hisense-remote [aria-label="选择确认键"]').attributes("aria-pressed")).toBe("true");
  });
  it("persists voice actions for every physical button and both pointing actions without writing Xiaomi settings", async () => {
    const view = await open();
    expect(view.findAll(".mapping-row")).toHaveLength(HISENSE_BUTTONS.length);
    for (const row of view.findAll(".mapping-row")) { await row.trigger("click"); await view.get(".mapping-editor select").setValue("VoiceInput"); }
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(Object.values(stored.button_bindings).every(a => a.type === "VoiceInput")).toBe(true);
    expect(invoke.mock.calls.some(([c]) => c === "save_config" || c === "update_key_mapping")).toBe(false);
    view.unmount();
  });
  it("keeps failed changes visible and never starts learning after a failed save", async () => {
    const view = await open();
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) => command === "hisense_save_config" ? Promise.reject("disk full") : original(command, args));
    await view.get('.mapping-editor[data-editing-button="ok"] select').setValue("VoiceInput");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(view.text()).toContain("保存失败");
    await view.get('.mapping-editor[data-editing-button="ok"] .row-actions .learn-button').trigger("click"); await flushPromises();
    expect(invoke.mock.calls.some(([c]) => c === "hisense_learn")).toBe(false);
    expect(stored.button_bindings.ok.type).toBe("None"); view.unmount(); await flushPromises();
  });
  it("waits for pending edits to finish saving before starting button learning", async () => {
    const view = await open();
    const original = invoke.getMockImplementation()!;
    let finish: (() => void) | undefined;
    invoke.mockImplementation((command, args) => command === "hisense_save_config" ? new Promise<void>(resolve => { finish = resolve; }) : original(command, args));
    await view.get('.mapping-editor[data-editing-button="ok"] select').setValue("VoiceInput");
    await vi.advanceTimersByTimeAsync(351);
    await view.get('.mapping-editor[data-editing-button="ok"] .row-actions .learn-button').trigger("click"); await flushPromises();
    expect(invoke.mock.calls.some(([c]) => c === "hisense_learn")).toBe(false);
    finish!(); await flushPromises();
    expect(invoke).toHaveBeenCalledWith("hisense_learn", { button: "ok" }); view.unmount();
  });
  it("reuses the input-method preset and saves anti-jitter controls independently", async () => {
    const view = await open();
    await view.get(".quick-grid button:nth-child(2)").trigger("click");
    await view.get("#ime-tab-doubao").trigger("click");
    await view.get(".ime-doubao-option .ime-button--primary").trigger("click"); await flushPromises();
    await view.get('[aria-label="关闭输入法设置"]').trigger("click"); await flushPromises();
    await view.get(".quick-grid button:first-child").trigger("click");
    await view.get('[aria-label="防抖强度"]').setValue("35");
    await view.get('[aria-label="微动过滤"]').setValue("3");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.voice_keys).toEqual([0xa5]); expect(stored.voice_mode).toBe("Hold");
    expect(stored.smoothing_ms).toBe(35); expect(stored.deadzone_pixels).toBe(3); view.unmount();
  });
});
