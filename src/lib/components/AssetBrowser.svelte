<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Search, Upload, RefreshCw, Image, UserRound, Bug, Mountain, Package, Sparkles, FolderOpen, MoreHorizontal } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { buildSpriteGroups, type SpriteGroup } from "$lib/sprite-groups";
  import { errorMessage, type Animation, type Asset, type AssetPack } from "$lib/types";

  let { workspaceId, worktreeId, assets, animations, packs = [], packId = "", category, selectedAssetId, onAssets, onSelect, onOpen, onPack, onLinked, onError }: {
    workspaceId: string; worktreeId?: string; assets: Asset[]; animations: Animation[]; packs?: AssetPack[]; packId?: string; category?: string; selectedAssetId?: string;
    onAssets: (assets: Asset[]) => void; onSelect: (asset: Asset) => void; onOpen: (group: SpriteGroup) => void | Promise<void>; onLinked?: () => void | Promise<void>; onError: (message: string) => void;
    onPack?: (packId: string) => void;
  } = $props();
  let search = $state("");
  let importing = $state(false);
  let importCategory = $state("characters");
  let refreshing = $state(false);
  let activeCategory = $state("assets");

  $effect(() => {
    if (category) activeCategory = category;
    if (activeCategory !== "assets") importCategory = activeCategory;
  });

  let groups = $derived(buildSpriteGroups(assets, animations));
  let packFiles = $derived(new Set(packs.find(pack => pack.id === packId)?.files ?? []));
  let activePack = $derived(packs.find(pack => pack.id === packId));
  let filtered = $derived(groups.filter(group => (activeCategory === "assets" || group.category === activeCategory) && (!packId || group.frames.some(asset => packFiles.has(asset.relativePath))) && (group.name.toLowerCase().includes(search.toLowerCase()) || group.frames.some(asset => asset.name.toLowerCase().includes(search.toLowerCase())))));
  let counts = $derived(Object.fromEntries(["characters","creatures","terrain","props","effects"].map(value => [value, groups.filter(group => group.category === value).length])));
  let heading = $derived(activeCategory === "assets" ? "All sprites" : activeCategory[0].toUpperCase() + activeCategory.slice(1));

  async function refresh() {
    refreshing = true;
    try { onAssets(await api.scanAssets(workspaceId)); }
    catch (error) { onError(errorMessage(error)); }
    finally { refreshing = false; }
  }

  async function importFile() {
    const selected = await open({ multiple: false, directory: false, title: "Import sprite asset", filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }] });
    if (typeof selected !== "string") return;
    importing = true;
    try {
      const asset = await api.importAsset(workspaceId, selected, importCategory);
      if (worktreeId) { await api.linkAssetToWorktree(worktreeId, asset.id); await onLinked?.(); }
      await refresh(); onSelect(asset);
    } catch (error) { onError(errorMessage(error)); }
    finally { importing = false; }
  }

  function drag(event: DragEvent, asset: Asset) {
    event.dataTransfer?.setData("application/x-sprite-studio-asset", asset.id);
    event.dataTransfer?.setData("text/plain", asset.id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
  }

  const categoryIcon = (value: string) => value === "characters" ? UserRound : value === "creatures" ? Bug : value === "terrain" ? Mountain : value === "props" ? Package : Sparkles;
</script>

<section class="asset-browser">
  <header><div><h1>{activePack ? activePack.name : heading}</h1><p>{filtered.length} sprite{filtered.length === 1 ? "" : "s"}{activePack ? " in this pack" : " in project"}</p></div><div class="header-actions"><button onclick={refresh} title="Scan project files" class:spinning={refreshing}><RefreshCw size={14} /></button><button class="import" onclick={importFile} disabled={importing}><Upload size={13} /> {importing ? "Importing…" : "Import"}</button></div></header>
  <div class="asset-layout">
    <aside>
      <div class="aside-label">LIBRARY</div>
      <button class="folder-row" class:active={activeCategory === "assets"} onclick={() => activeCategory = "assets"}><Image size={13}/><span>All sprites</span><small>{groups.length}</small></button>
      {#each ["characters","creatures","terrain","props","effects"] as item}
        {@const Icon = categoryIcon(item)}
        <button class="folder-row" class:active={activeCategory === item} onclick={() => activeCategory = item}><Icon size={13} /><span>{item[0].toUpperCase() + item.slice(1)}</span><small>{counts[item] ?? 0}</small></button>
      {/each}
      <div class="aside-label import-label">IMPORT TO</div>
      <select bind:value={importCategory}><option value="characters">Characters</option><option value="creatures">Creatures</option><option value="terrain">Terrain</option><option value="props">Props</option><option value="effects">Effects</option></select>
      <p class="hint">Imported files are copied into your project. Existing files are never overwritten.</p>
    </aside>
    <div class="content">
      <div class="toolbar"><div class="toolbar-filters"><div class="search"><Search size={13} /><input bind:value={search} placeholder="Filter sprites" /></div><select class="pack-filter" value={packId} onchange={(event)=>onPack?.(event.currentTarget.value)} aria-label="Filter by pack" title={activePack?.name ?? "All packs"}><option value="">All packs</option>{#each packs as pack}<option value={pack.id}>{pack.name}</option>{/each}</select></div><span>Animation frames are grouped into sprite sets</span></div>
      {#if filtered.length}
        <div class="grid">
          {#each filtered as group}
            <button class="asset" class:selected={group.frames.some(asset => selectedAssetId === asset.id)} class:grouped={group.frames.length > 1} onclick={() => onOpen(group)} draggable="true" ondragstart={(event) => drag(event, group.preview)}>
              <div class="thumb"><img src={assetUrl(group.preview.path)} alt={group.name} />{#if group.frames.length > 1}<span class="frame-badge">{group.frames.length} frames</span>{/if}</div>
              <div class="asset-info"><div><strong>{group.name}</strong><small>{group.preview.width}×{group.preview.height} · {group.frames.length > 1 ? `Sprite set${group.fps ? ` · ${group.fps} FPS` : ""}` : group.preview.format.toUpperCase()}</small></div><span title="Reveal on disk" role="button" tabindex="0" onclick={(event) => { event.stopPropagation(); revealItemInDir(group.preview.path); }} onkeydown={(event) => event.key === "Enter" && revealItemInDir(group.preview.path)}><MoreHorizontal size={14} /></span></div>
            </button>
          {/each}
        </div>
      {:else}
        <div class="empty"><div class="empty-icon">{#if search}<Search size={23} />{:else}<Image size={23} />{/if}</div><h2>{search ? "No matching assets" : "No assets here yet"}</h2><p>{search ? "Try a different search term." : "Import an image or add files to the project assets folder."}</p>{#if !search}<button onclick={importFile}><FolderOpen size={13} /> Import first asset</button>{/if}</div>
      {/if}
    </div>
  </div>
</section>

<style>
  .asset-browser{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);padding:0 14px 0 17px;display:flex;align-items:center;justify-content:space-between}header h1{font-size:12px;margin:0;font-weight:600}header p{font-size:11px;color:var(--faint);margin:3px 0 0}.header-actions{display:flex;gap:6px}.header-actions button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;padding:0 8px;display:flex;align-items:center;gap:6px;font:inherit;font-size:12px;cursor:pointer}.header-actions button:first-child{width:28px;padding:0;justify-content:center}.header-actions .import{background:var(--text);color:var(--bg);border-color:var(--text)}.spinning :global(svg){animation:spin .8s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
  .asset-layout{flex:1;min-height:0;display:grid;grid-template-columns:166px minmax(0,1fr)}aside{border-right:1px solid var(--border);padding:14px 10px;background:var(--sidebar)}.aside-label{font-size:10px;font-weight:700;letter-spacing:.13em;color:var(--faint);padding:4px 7px 7px}.folder-row{width:100%;height:28px;border:0;background:transparent;display:grid;grid-template-columns:14px minmax(0,1fr) 18px;gap:7px;align-items:center;padding:0 7px;color:var(--muted);font:inherit;font-size:12px;text-align:left;border-radius:4px;cursor:pointer}.folder-row:hover,.folder-row.active{background:var(--selected);color:var(--text)}.folder-row small{text-align:right;color:var(--faint);font-size:11px}.import-label{margin-top:23px}select{width:100%;height:28px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:12px;padding:0 6px;outline:none}.hint{font-size:10px;line-height:1.5;color:var(--faint);margin:9px 3px}
  .content{display:flex;flex-direction:column;min-width:0;min-height:0}.toolbar{height:43px;border-bottom:1px solid var(--border);padding:0 14px;display:flex;align-items:center;justify-content:space-between;gap:12px}.toolbar>span{font-size:10px;color:var(--faint);white-space:nowrap}.toolbar-filters{display:flex;align-items:center;gap:7px;min-width:0}.search{width:210px;height:26px;border:1px solid var(--border);border-radius:5px;display:flex;align-items:center;gap:6px;padding:0 8px;color:var(--faint);background:var(--surface)}.search input{min-width:0;flex:1;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:12px}.pack-filter{width:clamp(190px,20vw,280px);height:26px;text-overflow:ellipsis}.grid{padding:15px;display:grid;grid-template-columns:repeat(auto-fill,minmax(185px,1fr));gap:12px;overflow:auto;align-content:start}.asset{min-width:0;padding:0;border:1px solid transparent;background:transparent;color:var(--text);border-radius:7px;text-align:left;cursor:pointer;overflow:hidden}.asset:hover,.asset.selected{border-color:var(--border-strong);background:var(--surface)}.asset.selected{box-shadow:0 0 0 1px var(--accent-dim)}.asset.grouped{box-shadow:3px 3px 0 -1px var(--bg),4px 4px 0 0 var(--border)}.asset.grouped.selected{box-shadow:0 0 0 1px var(--accent-dim),3px 3px 0 -1px var(--bg),4px 4px 0 0 var(--border)}.thumb{height:132px;position:relative;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:14px 14px;background-position:0 0,0 7px,7px -7px,-7px 0}.thumb img{max-width:92%;max-height:92%;object-fit:contain;image-rendering:pixelated}.frame-badge{position:absolute;top:8px;right:8px;height:23px;display:flex;align-items:center;padding:0 8px;border:1px solid #ffffff20;border-radius:6px;background:#111212e8;color:#f4f4f5;font-size:10px;font-weight:650;box-shadow:0 3px 10px #0007}.asset-info{height:54px;padding:0 9px;display:flex;align-items:center;justify-content:space-between}.asset-info>div{min-width:0}.asset-info strong,.asset-info small{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.asset-info strong{font-size:12px;font-weight:560}.asset-info small{font-size:10px;color:var(--faint);margin-top:3px}.asset-info>span{color:var(--faint);padding:4px;display:grid;place-items:center}.empty{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;color:var(--faint)}.empty-icon{width:48px;height:48px;border:1px solid var(--border);border-radius:8px;display:grid;place-items:center;background:var(--surface)}.empty h2{font-size:13px;color:var(--text);margin:15px 0 6px}.empty p{font-size:12px;margin:0}.empty button{margin-top:17px;border:1px solid var(--border-strong);background:var(--surface);color:var(--text);height:30px;border-radius:5px;padding:0 10px;display:flex;gap:6px;align-items:center;font:inherit;font-size:12px;cursor:pointer}
  @media(max-width:1050px){.toolbar>span{display:none}.search{width:180px}.pack-filter{width:210px}.grid{grid-template-columns:repeat(auto-fill,minmax(170px,1fr))}}
</style>
