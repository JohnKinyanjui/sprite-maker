<script lang="ts">
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Download, ExternalLink, Minus, Plus, Play, RotateCcw, X } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { Asset } from "$lib/types";

  let { asset, onAnimate, onDownload, onClose }: {
    asset: Asset;
    onAnimate: (asset: Asset) => void | Promise<void>;
    onDownload: (asset: Asset) => void | Promise<void>;
    onClose: () => void;
  } = $props();

  let zoom = $state(1);
  const minimum = 0.25;
  const maximum = 8;

  $effect(() => {
    asset.id;
    zoom = asset.category === "terrain" ? 2 : asset.width <= 64 && asset.height <= 64 ? 6 : asset.width <= 160 && asset.height <= 160 ? 3 : 1.5;
  });

  function changeZoom(amount: number) {
    zoom = Math.min(maximum, Math.max(minimum, Number((zoom + amount).toFixed(2))));
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") onClose();
    if (event.key === "+" || event.key === "=") changeZoom(0.25);
    if (event.key === "-") changeZoom(-0.25);
    if (event.key === "0") zoom = 1;
  }

  function wheel(event: WheelEvent) {
    event.preventDefault();
    changeZoom(event.deltaY > 0 ? -0.25 : 0.25);
  }

  const formatBytes = (bytes: number) => bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes/1024).toFixed(1)} KB` : `${(bytes/1048576).toFixed(1)} MB`;
</script>

<svelte:window onkeydown={keydown}/>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <div class="viewer" role="dialog" aria-modal="true" aria-label={`Sprite viewer for ${asset.name}`} tabindex="-1">
    <header>
      <div class="identity"><strong>{asset.name}</strong><span>{asset.category} · {asset.width}×{asset.height} · {asset.format.toUpperCase()}</span></div>
      <div class="tools">
        <button class="zoom-action" onclick={() => changeZoom(-0.25)} title="Zoom out"><Minus size={14}/><span>Zoom out</span></button>
        <button class="zoom" onclick={() => zoom = 1} title="Show actual pixels">{Math.round(zoom * 100)}%</button>
        <button class="zoom-action" onclick={() => changeZoom(0.25)} title="Zoom in"><Plus size={14}/><span>Zoom in</span></button>
        <button class="actual" onclick={() => zoom = 1} title="Reset to actual pixels"><RotateCcw size={13}/><span>Actual size</span></button>
        <i></i>
        <button class="close" onclick={onClose} title="Close viewer"><X size={16}/></button>
      </div>
    </header>

    <div class="body">
      <div class="stage" onwheel={wheel}>
        <div class="canvas" style={`width:${Math.max(1, asset.width * zoom)}px;height:${Math.max(1, asset.height * zoom)}px`}>
          <img src={assetUrl(asset.path)} alt={asset.name} draggable="false"/>
        </div>
      </div>
      <aside>
        <div><h2>Sprite</h2><p>Inspect the complete source image before editing or separating any regions.</p></div>
        <dl>
          <div><dt>Canvas</dt><dd>{asset.width} × {asset.height}px</dd></div>
          <div><dt>Format</dt><dd>{asset.format.toUpperCase()}</dd></div>
          <div><dt>File size</dt><dd>{formatBytes(asset.fileSize)}</dd></div>
          <div><dt>Background</dt><dd>{asset.hasAlpha ? "Transparent" : "Opaque"}</dd></div>
        </dl>
        <p class="path">{asset.relativePath}</p>
        <div class="actions">
          <button class="primary" onclick={() => onAnimate(asset)}><Play size={13}/> Animate in chat</button>
          <button onclick={() => onDownload(asset)}><Download size={13}/> Download sprite</button>
          <button onclick={() => revealItemInDir(asset.path)}><ExternalLink size={13}/> Reveal on disk</button>
        </div>
        {#if asset.category === "terrain"}<div class="terrain-note"><strong>Complete terrain atlas</strong><span>The atlas stays as one image. Tile separation is left to the user or a future slicing workflow.</span></div>{/if}
      </aside>
    </div>
  </div>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:85;background:#000b;display:grid;place-items:center;padding:18px}.viewer{width:calc(100vw - 36px);height:calc(100vh - 64px);display:grid;grid-template-rows:58px minmax(0,1fr);overflow:hidden;border:1px solid var(--border-strong);border-radius:12px;background:var(--bg);box-shadow:0 28px 90px #000d}.viewer>header{display:flex;align-items:center;justify-content:space-between;padding:0 10px 0 18px;border-bottom:1px solid var(--border);background:var(--sidebar)}.identity strong,.identity span{display:block}.identity strong{font-size:14px}.identity span{font-size:10px;color:var(--faint);margin-top:3px;text-transform:capitalize}.tools{display:flex;align-items:center;gap:5px}.tools button{height:34px;min-width:34px;border:1px solid var(--border);border-radius:7px;background:var(--surface);color:var(--muted);display:flex;align-items:center;justify-content:center;gap:6px;padding:0 9px;font:inherit;font-size:10px;cursor:pointer}.tools button:hover{border-color:var(--border-strong);background:var(--surface-hover);color:var(--text)}.tools .zoom-action{min-width:88px}.tools .actual{min-width:90px}.tools .zoom{min-width:58px;background:var(--bg);font-weight:650}.tools .close{min-width:34px;padding:0;border-color:transparent;background:transparent}.tools i{width:1px;height:22px;background:var(--border);margin:0 4px}.body{min-height:0;display:grid;grid-template-columns:minmax(0,1fr) 250px}.stage{min-width:0;min-height:0;overflow:auto;display:grid;place-items:center;padding:58px;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:20px 20px;background-position:0 0,0 10px,10px -10px,-10px 0}.canvas{flex:none;line-height:0;box-shadow:0 0 0 1px #ffffff12,0 12px 40px #0008}.canvas img{width:100%;height:100%;object-fit:fill;image-rendering:pixelated;user-select:none}.body>aside{border-left:1px solid var(--border);background:var(--sidebar);padding:20px 16px;overflow:auto}.body>aside h2{font-size:13px;margin:0}.body>aside p{font-size:10px;line-height:1.5;color:var(--faint);margin:5px 0 0}dl{display:flex;flex-direction:column;gap:10px;margin:22px 0}dl div{display:flex;justify-content:space-between;gap:10px;font-size:11px}dt{color:var(--faint)}dd{margin:0;color:var(--muted);text-align:right}.path{overflow-wrap:anywhere!important;padding-top:14px;border-top:1px solid var(--border)}.actions{display:flex;flex-direction:column;gap:7px;margin-top:18px}.actions button{height:32px;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--muted);display:flex;align-items:center;justify-content:center;gap:7px;font:inherit;font-size:11px;cursor:pointer}.actions button:hover{border-color:var(--border-strong);color:var(--text)}.actions .primary{background:var(--text);border-color:var(--text);color:var(--bg)}.terrain-note{margin-top:18px;padding:11px;border:1px solid var(--border);border-radius:7px;background:var(--surface)}.terrain-note strong,.terrain-note span{display:block}.terrain-note strong{font-size:10px;color:var(--text)}.terrain-note span{font-size:9px;line-height:1.5;color:var(--faint);margin-top:5px}@media(max-width:900px){.tools .zoom-action span,.tools .actual span{display:none}.tools .zoom-action,.tools .actual{min-width:34px}.backdrop{padding:8px}.viewer{width:100%;height:100%;border-radius:8px}.body{grid-template-columns:1fr}.body>aside{display:none}.stage{padding:24px}}
</style>
