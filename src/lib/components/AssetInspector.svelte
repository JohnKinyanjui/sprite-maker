<script lang="ts">
  import { X, ExternalLink, Trash2, Pencil, Check, Image as ImageIcon, Layers3 } from "lucide-svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage, type Animation, type Asset, type AssetVersion } from "$lib/types";

  let { asset, animations, onClose, onChanged, onDeleted, onError }: {
    asset: Asset; animations: Animation[]; onClose: () => void; onChanged: (asset: Asset) => void; onDeleted: () => void; onError: (message: string) => void;
  } = $props();
  let editing = $state(false);
  let name = $state("");
  let busy = $state(false);
  let versions = $state<AssetVersion[]>([]);
  $effect(() => { if (!editing) name = asset.name; api.listAssetVersions(asset.id).then(value => versions=value).catch(() => versions=[]); });
  let usedBy = $derived(animations.filter(animation => animation.frames.some(frame => frame.assetId === asset.id)));

  async function rename() {
    if (!name.trim() || name === asset.name) { editing = false; return; }
    busy = true;
    try { const updated = await api.renameAsset(asset.id, name); onChanged(updated); editing = false; }
    catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
  async function remove() {
    if (!window.confirm(`Delete ${asset.name} from disk? This cannot be undone.`)) return;
    busy = true;
    try { await api.deleteAsset(asset.id); onDeleted(); }
    catch (error) { onError(errorMessage(error)); busy = false; }
  }
  const formatBytes = (bytes: number) => bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes/1024).toFixed(1)} KB` : `${(bytes/1048576).toFixed(1)} MB`;
</script>

<aside class="inspector">
  <header><div><ImageIcon size={13} /><span>Asset inspector</span></div><button onclick={onClose} title="Close inspector"><X size={14} /></button></header>
  <div class="scroll">
    <div class="preview"><img src={assetUrl(asset.path)} alt={asset.name} /></div>
    <section>
      <div class="name-row">
        {#if editing}<input bind:value={name} onkeydown={(event) => event.key === "Enter" && rename()} /><button onclick={rename} disabled={busy}><Check size={13} /></button>
        {:else}<div><h2>{asset.name}</h2><p>{asset.category}</p></div><button onclick={() => {name=asset.name;editing=true;}} title="Rename asset"><Pencil size={12} /></button>{/if}
      </div>
    </section>
    <section><h3>FILE</h3><dl><div><dt>Dimensions</dt><dd>{asset.width} × {asset.height} px</dd></div><div><dt>Format</dt><dd>{asset.format.toUpperCase()}</dd></div><div><dt>Size</dt><dd>{formatBytes(asset.fileSize)}</dd></div><div><dt>Transparency</dt><dd>{asset.hasAlpha ? "Alpha channel" : "Opaque"}</dd></div></dl></section>
    <section><h3>LOCATION</h3><p class="path">{asset.relativePath}</p><button class="wide" onclick={() => revealItemInDir(asset.path)}><ExternalLink size={12} /> Reveal on disk</button></section>
    <section><h3>ANIMATIONS</h3>{#if usedBy.length}<div class="used-list">{#each usedBy as animation}<div><Layers3 size={12} /><span>{animation.name}</span><small>{animation.frames.filter(frame => frame.assetId === asset.id).length} frame(s)</small></div>{/each}</div>{:else}<p class="muted">This image is not used by an animation.</p>{/if}</section>
    <section><h3>VERSIONS</h3>{#if versions.length}<div class="version-list">{#each versions as version}<div class:selected={version.selected}><span>v{version.versionNumber}</span><strong>{version.changeKind}</strong><small>{version.available ? new Date(version.createdAt).toLocaleString() : "Metadata only"}</small></div>{/each}</div>{:else}<p class="muted">No indexed versions yet.</p>{/if}</section>
    <section class="danger"><button onclick={remove} disabled={busy}><Trash2 size={12} /> Delete asset</button></section>
  </div>
</aside>

<style>
  .inspector{width:264px;min-width:264px;height:100%;border-left:1px solid var(--border);background:var(--sidebar);display:flex;flex-direction:column}.inspector>header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 10px 0 13px;font-size:12px;color:var(--muted)}header>div{display:flex;gap:7px;align-items:center}header button,.name-row button{border:0;background:transparent;color:var(--faint);width:26px;height:26px;display:grid;place-items:center;border-radius:4px;cursor:pointer}header button:hover,.name-row button:hover{background:var(--surface-hover);color:var(--text)}.scroll{overflow:auto;flex:1}.preview{height:210px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border-bottom:1px solid var(--border)}.preview img{max-width:86%;max-height:86%;object-fit:contain;image-rendering:pixelated}
  section{padding:14px;border-bottom:1px solid var(--border)}.name-row{display:flex;justify-content:space-between;align-items:center;gap:6px}.name-row h2{font-size:12px;margin:0;font-weight:600;overflow-wrap:anywhere}.name-row p{font-size:11px;color:var(--faint);margin:3px 0 0;text-transform:capitalize}.name-row input{min-width:0;flex:1;height:27px;border:1px solid var(--accent);background:var(--bg);border-radius:4px;color:var(--text);font:inherit;font-size:12px;padding:0 7px;outline:0}h3{font-size:10px;letter-spacing:.14em;color:var(--faint);margin:0 0 11px}dl{margin:0;display:flex;flex-direction:column;gap:9px}dl div{display:flex;justify-content:space-between;gap:10px;font-size:11px}dt{color:var(--faint)}dd{margin:0;color:var(--muted);text-align:right}.path{font-size:11px;line-height:1.45;color:var(--muted);overflow-wrap:anywhere;margin:0 0 10px}.wide{height:28px;width:100%;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;gap:6px;align-items:center;justify-content:center;font:inherit;font-size:11px;cursor:pointer}.wide:hover{color:var(--text);border-color:var(--border-strong)}.used-list{display:flex;flex-direction:column;gap:8px}.used-list div{display:grid;grid-template-columns:13px minmax(0,1fr) auto;align-items:center;gap:5px;font-size:11px;color:var(--muted)}.used-list small{font-size:10px;color:var(--faint)}.version-list{display:flex;flex-direction:column;gap:5px}.version-list>div{display:grid;grid-template-columns:24px minmax(0,1fr) auto;gap:6px;align-items:center;padding:6px;border:1px solid var(--border);border-radius:4px;color:var(--muted)}.version-list>div.selected{border-color:var(--accent);background:var(--accent-dim)}.version-list span{font-size:10px;color:var(--faint)}.version-list strong{font-size:10px;font-weight:600;text-transform:capitalize}.version-list small{font-size:9px;color:var(--faint)}.muted{font-size:11px;line-height:1.5;color:var(--faint);margin:0}.danger{border-bottom:0}.danger button{border:0;background:transparent;color:#c56f6b;font:inherit;font-size:11px;display:flex;align-items:center;gap:7px;padding:4px;cursor:pointer}
</style>
