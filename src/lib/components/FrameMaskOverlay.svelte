<script lang="ts">
  import type { BrushStamp, RegionMaskRect } from "$lib/types";

  let {
    imageSrc,
    frameWidth,
    frameHeight,
    scale = 1,
    mode = "regen",
    regions = $bindable([]),
    brushStrokes = $bindable([]),
    regionX = $bindable(0),
    regionY = $bindable(0),
    regionW = $bindable(16),
    regionH = $bindable(16),
    brushRadius = 8,
    tool = $bindable<"rect" | "brush" | "erase">("rect"),
  }: {
    imageSrc: string;
    frameWidth: number;
    frameHeight: number;
    scale?: number;
    mode?: "regen" | "erase";
    regions?: RegionMaskRect[];
    brushStrokes?: BrushStamp[];
    regionX?: number;
    regionY?: number;
    regionW?: number;
    regionH?: number;
    brushRadius?: number;
    tool?: "rect" | "brush" | "erase";
  } = $props();

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let dragging = $state(false);
  let dragStart = $state<{ x: number; y: number } | null>(null);

  const displayScale = $derived(Math.min(1, 220 / Math.max(frameWidth, frameHeight)) * scale);

  function mapPointer(event: PointerEvent) {
    const canvas = canvasEl;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const x = Math.floor((event.clientX - rect.left) / displayScale);
    const y = Math.floor((event.clientY - rect.top) / displayScale);
    return {
      x: Math.max(0, Math.min(frameWidth - 1, x)),
      y: Math.max(0, Math.min(frameHeight - 1, y)),
    };
  }

  function redraw() {
    const canvas = canvasEl;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = mode === "erase" ? "rgba(255, 80, 80, 0.35)" : "rgba(100, 181, 223, 0.35)";
    ctx.strokeStyle = mode === "erase" ? "#cc6863" : "#64b5df";
    for (const region of regions) {
      ctx.fillRect(region.x * displayScale, region.y * displayScale, region.width * displayScale, region.height * displayScale);
      ctx.strokeRect(region.x * displayScale, region.y * displayScale, region.width * displayScale, region.height * displayScale);
    }
    for (const stroke of brushStrokes) {
      ctx.beginPath();
      ctx.arc(stroke.x * displayScale, stroke.y * displayScale, stroke.radius * displayScale, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
  }

  $effect(() => {
    redraw();
  });

  function onPointerDown(event: PointerEvent) {
    const point = mapPointer(event);
    if (!point) return;
    if (tool === "brush" || tool === "erase") {
      brushStrokes = [...brushStrokes, { x: point.x, y: point.y, radius: brushRadius }];
      redraw();
      return;
    }
    dragging = true;
    dragStart = point;
    regionX = point.x;
    regionY = point.y;
    regionW = 1;
    regionH = 1;
    regions = [{ x: regionX, y: regionY, width: regionW, height: regionH }];
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging || !dragStart || tool !== "rect") return;
    const point = mapPointer(event);
    if (!point) return;
    regionX = Math.min(dragStart.x, point.x);
    regionY = Math.min(dragStart.y, point.y);
    regionW = Math.max(1, Math.abs(point.x - dragStart.x) + 1);
    regionH = Math.max(1, Math.abs(point.y - dragStart.y) + 1);
    regions = [{ x: regionX, y: regionY, width: regionW, height: regionH }];
    redraw();
  }

  function onPointerUp() {
    dragging = false;
    dragStart = null;
  }

  export function clearMask() {
    regions = [];
    brushStrokes = [];
    redraw();
  }
</script>

<div class="mask-overlay">
  <div class="tools">
    <button class:active={tool === "rect"} onclick={() => tool = "rect"}>Rect</button>
    <button class:active={tool === "brush" || tool === "erase"} onclick={() => tool = mode === "erase" ? "erase" : "brush"}>Brush</button>
    <label>Radius<input type="number" min="1" max="64" bind:value={brushRadius} /></label>
    <button onclick={clearMask}>Clear</button>
  </div>
  <div class="stage" style={`width:${frameWidth * displayScale}px;height:${frameHeight * displayScale}px`}>
    <img src={imageSrc} alt="" style={`width:${frameWidth * displayScale}px;height:${frameHeight * displayScale}px`} />
    <canvas
      bind:this={canvasEl}
      width={frameWidth * displayScale}
      height={frameHeight * displayScale}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointerleave={onPointerUp}
    ></canvas>
  </div>
</div>

<style>
  .mask-overlay{display:grid;gap:8px}
  .tools{display:flex;gap:6px;align-items:center;flex-wrap:wrap}
  .tools button,.tools label{font-size:10px;color:var(--muted)}
  .tools button{height:24px;border:1px solid var(--border);background:var(--surface);border-radius:4px;padding:0 8px;cursor:pointer}
  .tools button.active{border-color:var(--accent);color:var(--text)}
  .tools label{display:flex;align-items:center;gap:4px}
  .tools input{width:42px;height:22px;border:1px solid var(--border);background:var(--bg);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:0 4px}
  .stage{position:relative;image-rendering:pixelated;border:1px solid var(--border);background:var(--preview)}
  .stage img{display:block;pointer-events:none}
  .stage canvas{position:absolute;inset:0;touch-action:none;cursor:crosshair}
</style>
