import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WindowTitleBar from "./WindowTitleBar.vue";

const { win, stop, resized, focus } = vi.hoisted(() => {
  const stop = vi.fn();
  const resized = { callback: () => {} }, focus = { callback: (_event: {payload:boolean}) => {} };
  return { stop, resized, focus, win: {
    close: vi.fn(), minimize: vi.fn(), toggleMaximize: vi.fn(), startDragging: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false), isFocused: vi.fn().mockResolvedValue(true),
    onResized: vi.fn(async callback => { resized.callback = callback; return stop; }),
    onFocusChanged: vi.fn(async callback => { focus.callback = callback; return stop; }),
  } };
});
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));
describe("macOS style window titlebar", () => {
  beforeEach(() => { vi.clearAllMocks(); win.isMaximized.mockResolvedValue(false); });
  it("uses native window controls and keeps the close-request lifecycle", async () => {
    const view = mount(WindowTitleBar); await flushPromises();
    await view.get('[aria-label="关闭窗口"]').trigger("click");
    await view.get('[aria-label="最小化窗口"]').trigger("click");
    win.isMaximized.mockResolvedValue(true);
    await view.get('[aria-label="最大化窗口"]').trigger("click"); await flushPromises();
    expect(win.close).toHaveBeenCalledOnce(); expect(win.minimize).toHaveBeenCalledOnce(); expect(win.toggleMaximize).toHaveBeenCalledOnce();
    expect(view.find('[aria-label="还原窗口"]').exists()).toBe(true);
    focus.callback({payload:false}); await flushPromises(); expect(view.classes()).toContain("inactive");
    view.unmount(); expect(stop).toHaveBeenCalledTimes(2);
  });
  it("drags only the empty titlebar and uses double clicks to maximize", async () => {
    const view = mount(WindowTitleBar); await flushPromises();
    view.get('.window-titlebar').element.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:0,detail:1}));
    view.get('.window-titlebar').element.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:0,detail:2}));
    view.get('.window-titlebar').element.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:2,detail:1}));
    view.get('.traffic.close').element.dispatchEvent(new MouseEvent("mousedown", {bubbles:true,button:0,detail:1}));
    expect(win.startDragging).toHaveBeenCalledOnce(); expect(win.toggleMaximize).toHaveBeenCalledOnce();
    view.unmount();
  });
});
