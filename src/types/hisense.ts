export type HisenseAction =
  | { type: "SingleKey"; value: number }
  | { type: "ComboKey"; value: number[] }
  | { type: "TextInput" | "LaunchApp"; value: string }
  | { type: "MouseMove"; value: { dx: -1 | 0 | 1; dy: -1 | 0 | 1; step: number; accelerate: boolean } }
  | { type: "MouseClick" | "MouseRightClick" | "TouchClick" | "PointerConfirm" | "TouchDrag" | "PointerAi" | "VoiceInput" | "FocusChatGpt" | "None" };
export interface HisenseConfig {
  auto_connect: boolean; pointer_enabled: boolean; sensitivity: number;
  adaptive_sensitivity: boolean; precision_ratio: number;
  touch_indicator_enabled: boolean;
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
  running: boolean; connected: boolean; packets: number;
  last_button: string | null; last_source: string | null; observed_buttons: string[];
  learning_button: string | null; error: string | null;
  voice_active: boolean; pointer_mode: "touch" | "move" | "slide"; microphone: string | null; voice_error: string | null;
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
