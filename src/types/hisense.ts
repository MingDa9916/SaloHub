export type MacroMode = "Once" | "Hold" | "Toggle" | "Sequence";
export type MacroStep = { type: "KeyDown" | "KeyUp"; value: number } | { type: "MouseDown" | "MouseUp"; value: boolean } | { type: "Delay"; value: number };
export interface HisenseMacro { name: string; mode: MacroMode; color: string; use_default_delay: boolean; default_delay_ms: number; show_key_transitions: boolean; press: MacroStep[]; hold: MacroStep[]; release: MacroStep[]; }
export interface HisenseProfile { name: string; process_name: string; enabled: boolean; button_bindings: Record<string, HisenseAction>; long_press_bindings: Record<string, HisenseAction>; multi_click_bindings: Record<string, Partial<Record<2|3|4,HisenseAction>>>; }
export type HisenseAction =
  | { type: "SingleKey"; value: number }
  | { type: "ComboKey"; value: number[] }
  | { type: "TextInput" | "LaunchApp" | "Macro" | "SelectProfile"; value: string }
  | { type: "MouseMove"; value: { dx: -1 | 0 | 1; dy: -1 | 0 | 1; step: number; accelerate: boolean } }
  | { type: "MouseClick" | "MouseRightClick" | "TouchClick" | "PointerConfirm" | "TouchDrag" | "PointerAi" | "ToggleHighlighter" | "CycleProfile" | "StopMacros" | "VoiceInput" | "FocusChatGpt" | "None" };
export interface HisenseConfig {
  macros?: Record<string,HisenseMacro>; profiles?: Record<string,HisenseProfile>; profile_switching_enabled?: boolean; fixed_profile?: string | null;
  auto_connect: boolean; pointer_enabled: boolean; sensitivity: number;
  adaptive_sensitivity: boolean; precision_ratio: number;
  touch_indicator_enabled: boolean;
  indicator_color: string; indicator_pressed_color: string; highlight_trail_enabled: boolean; highlight_trail_color: string;
  notice_enabled?: boolean; notice_transparency_percent?: number;
  notice_background_color?: string; notice_text_color?: string;
  highlight_trail_width_px: number; highlight_trail_duration_ms: number;
  highlight_trail_opacity_percent: number; highlight_trail_glow_percent: number;
  pointer_hold_ms: number; pointer_gesture_revision?: number;
  smoothing_ms: number; deadzone_pixels: number;
  long_press_bindings?: Record<string, HisenseAction>;
  multi_click_bindings?: Record<string, Partial<Record<2 | 3 | 4, HisenseAction>>>;
  multi_click_interval_ms?: number;
  button_bindings: Record<string, HisenseAction>; button_sources: Record<string, string>;
  voice_keys: number[]; voice_profile: string; voice_mode: "Hold" | "Toggle";
  microphone_name: string | null; microphone_gain_db: number;
  confirm_touch_revision?: number;
}
export interface HisenseStatus {
  running: boolean; connected: boolean; battery_level: number | null; charging_power_connected: boolean | null; packets: number;
  active_profile?: string; active_profile_name?: string; active_application?: string; active_macros?: number;
  last_button: string | null; last_source: string | null; observed_buttons: string[];
  learning_button: string | null; error: string | null;
  voice_active: boolean; pointer_mode: "touch" | "move"; microphone: string | null; voice_error: string | null;
  voice_level: number; audio_frames: number;
}
export const HISENSE_BUTTONS = [
  ["power", "电源"], ["source", "信号源"], ["pointer", "指向"], ["ai", "AI"],
  ["tv", "电视"], ["shortcut", "快捷"], ["up", "上"], ["left", "左"],
  ["ok", "确认"], ["right", "右"], ["down", "下"], ["back", "返回"],
  ["home", "主页"], ["mic", "语音"], ["settings", "设置"], ["menu", "菜单"],
  ["volume_down", "音量−"], ["volume_up", "音量＋"],
  ["pointer_ok", "确认（指向开启）"], ["pointer_right", "副键（指向开启）"],
] as const;
