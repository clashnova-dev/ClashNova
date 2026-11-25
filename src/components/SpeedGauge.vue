<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(defineProps<{ value: number; active?: boolean }>(), { value: 0, active: false });

// 仪表盘几何（clock-angle：0=正上方，顺时针为正）。弧线自左下 225° 顺时针扫 270° 至右下。
const CX = 130;
const CY = 135;
const R = 104;
const LABEL_R = R - 25;
const NEEDLE_LEN = 90;
const START = 225;
const SWEEP = 270;
// 刻度与参考一致：低速段展开、高速段压缩（分段线性，等角分布）。
const TICKS = [0, 5, 10, 50, 100, 250, 500, 750, 1000];

function polar(r: number, deg: number) {
  const rad = (deg * Math.PI) / 180;
  return { x: CX + r * Math.sin(rad), y: CY - r * Math.cos(rad) };
}
function arc(r: number, aStart: number, aEnd: number) {
  const s = polar(r, aStart);
  const e = polar(r, aEnd);
  const large = aEnd - aStart > 180 ? 1 : 0;
  return `M ${s.x.toFixed(2)} ${s.y.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${e.x.toFixed(2)} ${e.y.toFixed(2)}`;
}
// 速率 → 归一化 t（0~1），按刻度分段线性映射。
function toT(v: number): number {
  const x = Math.min(Math.max(v, 0), TICKS[TICKS.length - 1]);
  const segs = TICKS.length - 1;
  for (let i = 0; i < segs; i++) {
    if (x <= TICKS[i + 1]) return (i + (x - TICKS[i]) / (TICKS[i + 1] - TICKS[i])) / segs;
  }
  return 1;
}

const t = computed(() => toT(props.value));
const trackPath = arc(R, START, START + SWEEP);
const progressPath = computed(() => (t.value <= 0 ? "" : arc(R, START, START + SWEEP * t.value)));
const needleAngle = computed(() => START + SWEEP * t.value);
const ticks = computed(() =>
  TICKS.map((v, i) => {
    const p = polar(LABEL_R, START + (SWEEP * i) / (TICKS.length - 1));
    return { v, x: p.x, y: p.y };
  }),
);
const reading = computed(() => {
  const v = props.value;
  if (!Number.isFinite(v) || v <= 0) return "0";
  return v >= 100 ? Math.round(v).toString() : v.toFixed(2);
});
</script>

<template>
  <svg class="gauge" viewBox="0 0 260 245" role="img" :aria-label="`${reading} Mbps`">
    <defs>
      <linearGradient id="speed-arc" x1="0" y1="1" x2="1" y2="0">
        <stop offset="0" stop-color="#2b7fff" />
        <stop offset="0.5" stop-color="#1fd1d6" />
        <stop offset="1" stop-color="#34d399" />
      </linearGradient>
      <linearGradient id="speed-needle" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="var(--text)" />
        <stop offset="1" stop-color="var(--muted)" stop-opacity="0.15" />
      </linearGradient>
    </defs>
    <path :d="trackPath" class="gauge-track" fill="none" :stroke-width="16" stroke-linecap="round" />
    <path v-if="progressPath" :d="progressPath" class="gauge-fill" :class="{ active }" fill="none" stroke="url(#speed-arc)" :stroke-width="16" stroke-linecap="round" />
    <text v-for="tick in ticks" :key="tick.v" :x="tick.x" :y="tick.y" class="gauge-tick" text-anchor="middle" dominant-baseline="central">{{ tick.v }}</text>
    <g class="gauge-needle" :style="{ transform: `rotate(${needleAngle}deg)` }">
      <polygon :points="`${CX - 5},${CY} ${CX + 5},${CY} ${CX},${CY - NEEDLE_LEN}`" fill="url(#speed-needle)" />
    </g>
    <circle :cx="CX" :cy="CY" r="9" class="gauge-hub" />
    <text :x="CX" :y="CY + 42" class="gauge-value" text-anchor="middle">{{ reading }}</text>
    <text :x="CX" :y="CY + 64" class="gauge-unit" text-anchor="middle">Mbps</text>
  </svg>
</template>

<style scoped>
.gauge { width: 100%; height: auto; display: block; }
.gauge-track { stroke: var(--line); }
.gauge-fill { transition: d 0.25s linear; filter: drop-shadow(0 0 6px rgba(31, 209, 214, 0.25)); }
.gauge-needle { transform-origin: 130px 135px; transition: transform 0.25s cubic-bezier(0.22, 1, 0.36, 1); }
.gauge-hub { fill: var(--text); }
.gauge-tick { fill: var(--muted); font-size: 12px; font-weight: 500; font-variant-numeric: tabular-nums; }
.gauge-value { fill: var(--text); font-size: 40px; font-weight: 650; font-variant-numeric: tabular-nums; letter-spacing: -0.01em; }
.gauge-unit { fill: var(--muted); font-size: 14px; font-weight: 500; }
@media (prefers-reduced-motion: reduce) {
  .gauge-needle, .gauge-fill { transition: none; }
}
</style>
