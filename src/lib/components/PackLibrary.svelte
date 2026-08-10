<script lang="ts">
  import { ArrowLeft, Boxes, Images, PackageOpen, Search } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { Asset, AssetPack } from "$lib/types";

  let { packs, assets, selectedPackId = "", onView, onBack, onOpen }: {
    packs: AssetPack[];
    assets: Asset[];
    selectedPackId?: string;
    onView: (pack: AssetPack) => void;
    onBack: () => void;
    onOpen: (asset: Asset) => void;
  } = $props();

  let search = $state("");

  function packAssets(pack: AssetPack) {
    const paths = new Set(pack.files);
    return assets.filter(asset => paths.has(asset.relativePath));
  }

  let selectedPack = $derived(packs.find(pack => pack.id === selectedPackId));
  let selectedItems = $derived(selectedPack ? packAssets(selectedPack).filter(asset => asset.name.toLowerCase().includes(search.toLowerCase())) : []);
</script>

<section class="pack-library">
  {#if selectedPack}
    <header class="detail-header">
      <div class="detail-identity"><button class="back" onclick={onBack} title="Back to all packs"><ArrowLeft size={14}/></button><div><h1>{selectedPack.name}</h1><p>{packAssets(selectedPack).length} reusable sprites · {selectedPack.style}</p></div></div>
      <div class="search"><Search size={13}/><input bind:value={search} placeholder="Filter this pack" aria-label="Filter this pack"/></div>
    </header>
    {#if selectedItems.length}
      <div class="sprite-grid">
        {#each selectedItems as asset}
          <button class="sprite-card" onclick={()=>onOpen(asset)} title={`Inspect ${asset.name}`}>
            <span class="sprite-preview"><img src={assetUrl(asset.path)} alt={asset.name}/></span>
            <span class="sprite-copy"><strong>{asset.name}</strong><small>{asset.width}×{asset.height} · {asset.format.toUpperCase()}</small></span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="empty"><Search size={30} strokeWidth={1.3}/><h2>No matching sprites</h2><p>Try another name.</p></div>
    {/if}
  {:else}
  <header><div><h1>Asset packs</h1><p>Coordinated assets that share one production style</p></div><span><Boxes size={13}/>{packs.length} pack{packs.length===1?"":"s"}</span></header>
  {#if packs.length}
    <div class="grid">
      {#each packs as pack}
        {@const items=packAssets(pack)}
        <article>
          <div class="mosaic">
            {#each items.slice(0,4) as asset}<button onclick={()=>onOpen(asset)} title={`Open ${asset.name}`}><img src={assetUrl(asset.path)} alt={asset.name}/></button>{/each}
            {#if !items.length}<PackageOpen size={30}/>{/if}
          </div>
          <div class="copy"><div><small>{pack.kind} · {pack.style}</small><h2>{pack.name}</h2><p>{pack.description}</p></div><button class="view" onclick={()=>onView(pack)}><Images size={13}/> View {items.length} sprites</button></div>
        </article>
      {/each}
    </div>
  {:else}
    <div class="empty"><PackageOpen size={34} strokeWidth={1.3}/><h2>No asset packs yet</h2><p>Type <code>/pack</code> in chat to create a coordinated set of animals, objects, characters, or effects in one art style.</p></div>
  {/if}
  {/if}
</section>

<style>
  .pack-library{height:100%;display:flex;flex-direction:column;background:var(--bg)}header{height:58px;min-height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 18px}h1{font-size:14px;margin:0}header p{font-size:11px;color:var(--faint);margin:4px 0 0}header>span{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:11px}.grid{padding:20px;display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,440px),620px));gap:16px;overflow:auto;align-content:start}article{min-width:0;border:1px solid var(--border);border-radius:9px;background:var(--surface);overflow:hidden}.mosaic{height:228px;box-sizing:border-box;padding:14px;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));grid-template-rows:repeat(2,minmax(0,1fr));gap:7px;color:var(--faint);background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0}.mosaic button{position:relative;width:100%;height:100%;min-width:0;min-height:0;overflow:hidden;border:1px solid var(--border);border-radius:6px;background:#0b0c0ccf;cursor:pointer}.mosaic button:hover{border-color:var(--border-strong)}.mosaic img{position:absolute;inset:8px;display:block;width:calc(100% - 16px);height:calc(100% - 16px);object-fit:contain;image-rendering:pixelated}.copy{min-height:108px;box-sizing:border-box;padding:14px;display:flex;gap:18px;align-items:flex-end;justify-content:space-between}.copy>div{min-width:0}.copy small{display:block;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:9px;color:var(--faint);text-transform:uppercase;letter-spacing:.1em}.copy h2{font-size:14px;margin:6px 0}.copy p{font-size:11px;line-height:1.45;color:var(--muted);margin:0;display:-webkit-box;line-clamp:2;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}.view{height:31px;white-space:nowrap;flex:0 0 auto;border:1px solid var(--border-strong);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;display:flex;align-items:center;gap:6px;padding:0 9px;cursor:pointer}.detail-identity{display:flex;align-items:center;gap:10px}.back{width:30px;height:30px;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--muted);display:grid;place-items:center;cursor:pointer}.back:hover{color:var(--text);border-color:var(--border-strong)}.search{width:220px;height:30px;box-sizing:border-box;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--faint);display:flex;align-items:center;gap:7px;padding:0 9px}.search input{min-width:0;flex:1;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:11px}.sprite-grid{padding:18px;display:grid;grid-template-columns:repeat(auto-fill,minmax(190px,1fr));gap:12px;overflow:auto;align-content:start}.sprite-card{min-width:0;padding:0;overflow:hidden;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text);text-align:left;cursor:pointer}.sprite-card:hover{border-color:var(--border-strong);background:var(--surface-hover)}.sprite-preview{height:170px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0}.sprite-preview img{display:block;max-width:90%;max-height:90%;object-fit:contain;image-rendering:pixelated}.sprite-copy{height:55px;box-sizing:border-box;padding:10px 11px;display:block}.sprite-copy strong,.sprite-copy small{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.sprite-copy strong{font-size:12px}.sprite-copy small{font-size:10px;color:var(--faint);margin-top:4px}.empty{flex:1;display:flex;align-items:center;justify-content:center;flex-direction:column;color:var(--faint);text-align:center}.empty h2{font-size:16px;color:var(--text);margin:16px 0 7px}.empty p{font-size:12px;max-width:470px;line-height:1.55;margin:0}.empty code{color:var(--text);background:var(--surface);padding:2px 5px;border-radius:4px}@media(max-width:760px){.grid,.sprite-grid{grid-template-columns:1fr;padding:14px}.copy{align-items:flex-start;flex-direction:column}.mosaic{height:210px}.search{width:160px}}
</style>
