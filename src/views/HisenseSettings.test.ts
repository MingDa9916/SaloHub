import { enableAutoUnmount, flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import HisenseSettings from "./HisenseSettings.vue";
import { HISENSE_BUTTONS, type HisenseConfig } from "../types/hisense";

const { invoke, openFile, saveFile } = vi.hoisted(() => ({ invoke: vi.fn(), openFile: vi.fn(), saveFile: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openFile, save: saveFile }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
enableAutoUnmount(afterEach);
let stored: HisenseConfig;
beforeEach(() => {
  vi.useFakeTimers();
  stored = {
    auto_connect: true, pointer_enabled: true, touch_indicator_enabled: true, pointer_hold_ms: 350, pointer_gesture_revision: 2, sensitivity: 2, adaptive_sensitivity: true, precision_ratio: 0.25, smoothing_ms: 18, deadzone_pixels: 1.5,
    long_press_bindings: {}, multi_click_bindings: {}, multi_click_interval_ms: 300,
    button_bindings: Object.fromEntries(HISENSE_BUTTONS.map(([id]) => [id, { type: "None" }])),
    button_sources: { "key:28": "ok" }, voice_keys: [0xa2, 0xa0, 0x44], voice_profile: "codex",
    voice_mode: "Hold", microphone_name: null, microphone_gain_db: 0,
  };
  openFile.mockReset(); saveFile.mockReset();
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
  it("opens each quick action in a matching modal and restores focus on Escape", async () => {
    const view = await open();
    expect(view.find('[role="dialog"]').exists()).toBe(false);
    const cards = view.findAll(".quick-grid button");
    const titles = ["指向与防抖", "语音输入设置", "按键桥接", "接收器管理"];
    for (const [index, title] of titles.entries()) {
      (cards[index].element as HTMLButtonElement).focus();
      await cards[index].trigger("click");
      const dialog = view.get('[role="dialog"]');
      expect(view.get("#hisense-settings-title").text()).toBe(title);
      expect(dialog.attributes("aria-modal")).toBe("true");
      expect(document.activeElement).toBe(dialog.element);
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
    await view.get(".settings-dialog .shortcut button").trigger("click"); await flushPromises();
    await view.get('[aria-label="关闭语音输入设置"]').trigger("click"); await flushPromises();
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
    await view.get('[aria-label="海信输入法预设"]').setValue("doubao-hold");
    await view.get('[aria-label="关闭语音输入设置"]').trigger("click"); await flushPromises();
    await view.get(".quick-grid button:first-child").trigger("click");
    await view.get('[aria-label="防抖强度"]').setValue("35");
    await view.get('[aria-label="微动过滤"]').setValue("3");
    await vi.advanceTimersByTimeAsync(351); await flushPromises();
    expect(stored.voice_keys).toEqual([0xa5]); expect(stored.voice_mode).toBe("Hold");
    expect(stored.smoothing_ms).toBe(35); expect(stored.deadzone_pixels).toBe(3); view.unmount();
  });
});
