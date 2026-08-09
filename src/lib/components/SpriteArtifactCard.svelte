<script lang="ts">
  import { Clapperboard, Download, LoaderCircle, Pause, Pencil, Play, Image as ImageIcon } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { Animation, Asset, SpriteGenerationMetadata } from "$lib/types";

  let { generation, assets, animations, onEditAsset, onEditAnimation, onExportAsset, onExportAnimation }: {
    generation: SpriteGenerationMetadata; assets: Asset[]; animations: Animation[];
    onEditAsset: (asset: Asset) => void; onEditAnimation: (animation: Animation) => void; onExportAsset: (asset: Asset) => Promise<void>; onExportAnimation: (animation: Animation) => Promise<void>;
  } = $props();
  let frame = $state(0);
  let playing = $state(true);
  let exporting = $state(false);
  let frames = $derived(generation.assetIds.map(id => assets.find(asset => asset.id === id)).filter((asset): asset is Asset => Boolean(asset)));
  let animation = $derived(animations.find(item => item.id === generation.animationId));
  let current = $derived(frames[frame] ?? frames[0]);

  $effect(() => {
    if (!playing || frames.length < 2) return;
    const timer = window.setTimeout(() => frame = (frame + 1) % frames.length, 1000 / Math.max(1, generation.fps));
    return () => window.clearTimeout(timer);
  });

  function edit() {
    if (animation) onEditAnimation(animation);
    else if (current) onEditAsset(current);
  }

  async function exportSprite() {
    if ((!animation && !current) || exporting) return;
    exporting = true;
    try {
      if (animation) await onExportAnimation(animation);
      else if (current) await onExportAsset(current);
    }
    finally { exporting = false; }
  }
</script>

{#if current}
  <section class="artifact" aria-label={`Generated sprite ${generation.name}`}>
    <div class="preview">
      <img src={assetUrl(current.path)} alt={`${generation.name}, frame ${frame + 1}`}/>
      {#if frames.length > 1}<div class="frame-count">{frame + 1}/{frames.length}</div>{/if}
    </div>
    <div class="details">
      <div class="eyebrow">{#if animation}<Clapperboard size={12}/><span>SPRITE ANIMATION</span>{:else}<ImageIcon size={12}/><span>SPRITE ASSET</span>{/if}</div>
      <h3>{generation.name}</h3>
      <p>{current.width}×{current.height} px · {frames.length} frame{frames.length===1?"":"s"}{#if frames.length > 1} · {generation.fps} FPS{/if}</p>
      <div class="frame-strip">
        {#each frames as asset, index}<button class:active={index===frame} onclick={() => {frame=index;playing=false;}} title={`Preview frame ${index+1}`}><img src={assetUrl(asset.path)} alt=""/></button>{/each}
      </div>
      <div class="actions">
        {#if frames.length > 1}<button class="playback" class:active={playing} onclick={() => playing=!playing} aria-pressed={playing} title={playing?"Pause preview":"Play preview"}>{#if playing}<Pause size={13}/>{:else}<Play size={13} fill="currentColor"/>{/if}<span>{playing?"Pause":"Play"}</span></button>{/if}
        <button onclick={edit}><Pencil size={13}/><span>Edit {animation ? "animation" : "sprite"}</span></button>
        <button class="primary" onclick={exportSprite} disabled={exporting}>{#if exporting}<LoaderCircle class="spin" size={13}/>{:else}<Download size={13}/>{/if}<span>{exporting?"Exporting…":"Export"}</span></button>
      </div>
    </div>
  </section>
{/if}

<style>
  .artifact{width:min(640px,100%);min-height:216px;margin-top:16px;border:1px solid var(--border-strong);border-radius:12px;background:var(--surface);display:grid;grid-template-columns:210px minmax(0,1fr);overflow:hidden;box-shadow:0 10px 28px #0004}.preview{position:relative;display:grid;place-items:center;overflow:hidden;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border-right:1px solid var(--border)}.preview>img{width:128px;height:128px;object-fit:contain;image-rendering:pixelated}.frame-count{position:absolute;left:10px;bottom:10px;height:25px;padding:0 8px;border:1px solid #ffffff1a;border-radius:6px;background:#101111e8;color:#d9d9db;display:flex;align-items:center;font-size:12px}.details{min-width:0;padding:20px;display:flex;flex-direction:column}.eyebrow{display:flex;align-items:center;gap:6px;color:var(--faint);font-size:11px;font-weight:700;letter-spacing:.11em}.details h3{font-size:17px;line-height:1.25;margin:11px 0 5px;font-weight:650}.details p{font-size:13px;color:var(--muted);margin:0}.frame-strip{display:flex;gap:6px;margin-top:14px;overflow:hidden}.frame-strip button{width:38px;height:38px;min-width:38px;border:1px solid var(--border);border-radius:6px;background:var(--preview);padding:3px;cursor:pointer}.frame-strip button:hover{border-color:var(--border-strong)}.frame-strip button.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.frame-strip img{width:100%;height:100%;object-fit:contain;image-rendering:pixelated}.actions{display:flex;align-items:center;gap:7px;margin-top:auto;padding-top:14px}.actions button{height:32px;border:1px solid var(--border-strong);border-radius:7px;background:var(--surface-hover);color:var(--text);padding:0 10px;display:flex;align-items:center;justify-content:center;gap:6px;font:inherit;font-size:12px;font-weight:570;cursor:pointer;white-space:nowrap}.actions button:hover{border-color:#535456;background:#282929}.actions button.playback.active{color:#c4b5fd;border-color:#6d4ad0}.actions button.primary{margin-left:auto;background:var(--accent);border-color:var(--accent);color:white}.actions button.primary:hover{background:#7c4fe6;border-color:#7c4fe6}.actions button:disabled{opacity:.55;cursor:wait}.actions :global(.spin){animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:760px){.artifact{grid-template-columns:170px}.preview>img{width:104px;height:104px}.actions button span{display:none}.actions button.primary{margin-left:0}}
</style>
