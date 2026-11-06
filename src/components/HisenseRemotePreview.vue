<script setup lang="ts">
import photo from "../assets/hisense-remote.png";
import { HISENSE_BUTTONS } from "../types/hisense";

defineProps<{ selectedId: string; hoverId?: string | null }>();
defineEmits<{ select: [id: string]; hover: [id: string | null] }>();
// Positions are measured on the supplied 361 × 994 CRF3C65 photograph.
const keys: Array<[string, number, number, number, number]> = [
  ["power",176,118,46,46], ["source",112,150,48,48], ["pointer",239,151,50,50],
  ["ai",175,186,52,52], ["tv",112,221,48,48], ["shortcut",239,220,50,50],
  ["up",176,270,62,40], ["right",245,338,42,62], ["down",176,405,62,40],
  ["left",107,338,42,62], ["ok",176,338,106,106], ["back",112,455,44,44],
  ["home",239,455,44,44], ["mic",176,490,50,50], ["settings",112,524,44,44],
  ["menu",239,524,44,44], ["volume_down",120,589,60,40], ["volume_up",231,589,60,40],
];
const name = (id: string) => HISENSE_BUTTONS.find(([key]) => key === id)?.[1] || id;
const physical = (id?: string | null) => id === "pointer_ok" ? "ok" : id;
</script>

<template>
  <div class="hisense-remote" aria-label="海信遥控器按键预览">
    <img :src="photo" alt="海信 CRF3C65 遥控器" draggable="false" />
    <button v-for="[id,x,y,w,h] in keys" :key="id" type="button"
      :class="['remote-hotspot', { active: physical(selectedId) === id, hovered: physical(hoverId) === id }]"
      :style="{ left: `${x/361*100}%`, top: `${y/994*100}%`, width: `${w/361*100}%`, height: `${h/994*100}%` }"
      :aria-label="`选择${name(id)}键`" :title="`${name(id)} · 查看映射`" :aria-pressed="physical(selectedId) === id"
      @click="$emit('select', id)" @mouseenter="$emit('hover', id)" @mouseleave="$emit('hover', null)"
      @focus="$emit('hover', id)" @blur="$emit('hover', null)" />
  </div>
</template>

<style scoped>
.hisense-remote { position: relative; width: min(100%, 210px); aspect-ratio: 361/994; margin: 0 auto; }
img { display: block; width: 100%; border-radius: 18px; user-select: none; }
.remote-hotspot { position: absolute; transform: translate(-50%,-50%); border: 2px solid transparent; border-radius: 50%; background: transparent; padding: 0; cursor: pointer; transition: background .15s, box-shadow .15s; }
.remote-hotspot:hover, .remote-hotspot.hovered { border-color: var(--primary); background: #3478f622; box-shadow: 0 0 0 4px #3478f61a; }
.remote-hotspot.active { border-color: var(--primary); background: #3478f63b; box-shadow: 0 0 0 5px #3478f622, 0 0 18px #3478f633; }
.remote-hotspot:focus-visible { outline: 3px solid var(--primary); outline-offset: 4px; }
</style>
