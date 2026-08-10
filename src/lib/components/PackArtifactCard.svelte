<script lang="ts">
  import { Boxes, Images } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { Asset, AssetPack } from "$lib/types";

  let { pack, assets, onView }: { pack: AssetPack; assets: Asset[]; onView: (pack: AssetPack) => void } = $props();
  let items = $derived(pack.files.map(file => assets.find(asset => asset.relativePath === file)).filter((asset): asset is Asset => Boolean(asset)));
</script>

<section class="pack-card" aria-label={`Generated asset pack ${pack.name}`}>
  <div class="mosaic">
    {#each items.slice(0,6) as asset}<img src={assetUrl(asset.path)} alt={asset.name}/>{/each}
    {#if items.length > 6}<span>+{items.length - 6}</span>{/if}
  </div>
  <div class="details">
    <div class="eyebrow"><Boxes size={12}/><span>ASSET PACK</span></div>
    <h3>{pack.name}</h3>
    <p>{items.length} reusable asset{items.length===1?"":"s"} · {pack.kind} · {pack.style}</p>
    {#if pack.description}<small>{pack.description}</small>{/if}
    <button onclick={()=>onView(pack)}><Images size={13}/> View sprites</button>
  </div>
</section>

<style>
  .pack-card{width:min(680px,100%);min-height:230px;margin-top:16px;border:1px solid var(--border-strong);border-radius:12px;background:var(--surface);display:grid;grid-template-columns:280px minmax(0,1fr);overflow:hidden;box-shadow:0 10px 28px #0004}.mosaic{position:relative;padding:12px;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));grid-template-rows:repeat(2,minmax(0,1fr));gap:6px;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border-right:1px solid var(--border)}.mosaic img{width:100%;height:100%;min-width:0;min-height:0;object-fit:contain;image-rendering:pixelated;background:#0b0c0cbf;border:1px solid var(--border);border-radius:6px}.mosaic>span{position:absolute;right:10px;bottom:10px;height:25px;padding:0 8px;border:1px solid #ffffff1a;border-radius:6px;background:#101111e8;color:#d9d9db;display:flex;align-items:center;font-size:11px}.details{min-width:0;padding:20px;display:flex;flex-direction:column}.eyebrow{display:flex;align-items:center;gap:6px;color:var(--faint);font-size:11px;font-weight:700;letter-spacing:.11em}.details h3{font-size:18px;line-height:1.25;margin:12px 0 6px}.details p{font-size:12px;color:var(--muted);margin:0}.details small{font-size:11px;line-height:1.5;color:var(--faint);margin-top:12px}.details button{align-self:flex-start;height:32px;margin-top:auto;border:1px solid var(--border-strong);border-radius:7px;background:var(--text);color:var(--bg);padding:0 11px;display:flex;align-items:center;gap:6px;font:inherit;font-size:12px;font-weight:620;cursor:pointer}@media(max-width:760px){.pack-card{grid-template-columns:210px}.mosaic{grid-template-columns:repeat(2,minmax(0,1fr));grid-template-rows:repeat(3,minmax(0,1fr))}}
</style>
