<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { GlobalSettings, VoiceFeedbackSettings } from '../types';
import VoiceFeedbackEditor from './VoiceFeedbackSettings.vue';
const settings=ref<VoiceFeedbackSettings>(),saving=ref(false),error=ref('');
let pending:VoiceFeedbackSettings|undefined;
async function save(value:VoiceFeedbackSettings){settings.value=value;pending=value;if(saving.value)return;saving.value=true;error.value='';try{while(pending){const next=pending;pending=undefined;try{await invoke('save_voice_feedback',{settings:next});}catch(e){pending??=next;throw e;}}}catch(e){error.value=String(e);}finally{saving.value=false;}}
onMounted(async()=>{try{settings.value=(await invoke<GlobalSettings>('get_global_settings')).voice_feedback;}catch(e){error.value=String(e);}});
</script>
<template><div><p>海信与小米共用语音提示音设置。支持预设、试听、自定义歌曲及片段裁剪。</p><VoiceFeedbackEditor :model-value="settings" :saving="saving" @update:model-value="save" /><p v-if="error" role="status">{{ error }} <button :disabled="saving" @click="settings&&save(settings)">重试保存</button></p></div></template>
