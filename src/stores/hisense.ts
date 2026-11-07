import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { HisenseStatus } from "../types/hisense";
export const useHisenseStore = defineStore("hisense", () => {
  const status = ref<HisenseStatus>({ running: false, connected: false, battery_level: null, charging_power_connected: null, packets: 0, last_button: null, last_source: null, observed_buttons: [], learning_button: null, error: null, voice_active: false, pointer_mode: "touch", microphone: null, voice_error: null, voice_level: 0, audio_frames: 0 });
  async function refresh() { status.value = await invoke<HisenseStatus>("hisense_status"); }
  return { status, refresh };
});
