<script lang="ts">
  import { X, ExternalLink, Trash2, Pencil, Check, Image as ImageIcon, Layers3, Anchor, Download, Upload, RefreshCw } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, assetUrl } from "$lib/api";
  import FacingPanel from "$lib/components/FacingPanel.svelte";
  import PromoteAnchorDialog from "$lib/components/PromoteAnchorDialog.svelte";
  import { confirmAndDeleteAsset } from "$lib/asset-removal";
  import { errorMessage, type Animation, type Asset, type AssetVersion, type CharacterAnchorSummary, type CharacterProfile } from "$lib/types";

  let { asset, workspaceId, animations, onClose, onChanged, onDeleted, onError, onNotice }: {
    asset: Asset; workspaceId: string; animations: Animation[]; onClose: () => void; onChanged: (asset: Asset) => void; onDeleted: () => void; onError: (message: string) => void; onNotice?: (message: string) => void;
  } = $props();
  let editing = $state(false);
  let name = $state("");
  let busy = $state(false);
  let versions = $state<AssetVersion[]>([]);
  let anchorSummary = $state<CharacterAnchorSummary>();
  let characterProfile = $state<CharacterProfile>();
  let promoteDialogOpen = $state(false);
  async function loadCharacterProfile() {
    if (!anchorSummary?.slug) { characterProfile = undefined; return; }
    try { characterProfile = await api.getCharacterProfile(workspaceId, anchorSummary.slug); }
    catch { characterProfile = undefined; }
  }
  async function rebuildCharacterProfile() {
    if (!anchorSummary?.slug) { characterProfile = undefined; return; }
    try { characterProfile = await api.refreshCharacterProfile(workspaceId, anchorSummary.slug); }
    catch { characterProfile = undefined; }
  }
  $effect(() => {
    if (!editing) name = asset.name;
    api.listAssetVersions(asset.id).then(value => versions=value).catch(() => versions=[]);
    api.listAnchors(workspaceId).then(anchors => {
      anchorSummary = anchors.find(anchor => anchor.assetId === asset.id);
      void loadCharacterProfile();
    }).catch(() => { anchorSummary = undefined; characterProfile = undefined; });
  });
  let usedBy = $derived(animations.filter(animation => animation.frames.some(frame => frame.assetId === asset.id)));

  async function rename() {
    if (!name.trim() || name === asset.name) { editing = false; return; }
    busy = true;
    try { const updated = await api.renameAsset(asset.id, name); onChanged(updated); editing = false; }
    catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
  async function promoteAnchor(view: string, autoOrient: boolean) {
    busy = true;
    try {
      const anchor = await api.promoteAnchor(workspaceId, asset.id, undefined, autoOrient, view);
      anchorSummary = anchor;
      characterProfile = await api.refreshCharacterProfile(workspaceId, anchor.slug);
      promoteDialogOpen = false;
      onNotice?.(`Promoted character anchor "${anchor.slug}" (${anchor.frameWidth}×${anchor.frameHeight}) · ${anchor.view ?? view}`);
    } catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
  async function flipToCanonical() {
    if (!anchorSummary) return;
    busy = true;
    try {
      const anchor = await api.orientAnchor(workspaceId, anchorSummary.slug);
      anchorSummary = anchor;
      await rebuildCharacterProfile();
      onNotice?.(`Anchor oriented to canonical facing ${anchor.facing ?? "w"}`);
    } catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
  async function remove() {
    busy = true;
    try {
      if (await confirmAndDeleteAsset(asset)) onDeleted();
      else busy = false;
    } catch (error) { onError(errorMessage(error)); busy = false; }
  }
  const formatBytes = (bytes: number) => bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes/1024).toFixed(1)} KB` : `${(bytes/1048576).toFixed(1)} MB`;
  async function exportProfile() {
    if (!anchorSummary?.slug) return;
    const destination = await open({ filters: [{ name: "JSON", extensions: ["json"] }], defaultPath: `${anchorSummary.slug}-profile.json` });
    if (typeof destination !== "string") return;
    busy = true;
    try {
      characterProfile = await api.exportCharacterProfile({ workspaceId, slug: anchorSummary.slug, destination });
      onNotice?.("Character profile exported");
    } catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
  async function importProfile() {
    const sourcePath = await open({ filters: [{ name: "JSON", extensions: ["json"] }], multiple: false });
    if (typeof sourcePath !== "string") return;
    busy = true;
    try {
      characterProfile = await api.importCharacterProfile({ workspaceId, sourcePath });
      onNotice?.(`Character profile imported for ${characterProfile.anchorSlug}`);
    } catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }
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
    <section><h3>PIPELINE</h3><button class="wide anchor" onclick={() => promoteDialogOpen = true} disabled={busy}><Anchor size={12} /> Set as character anchor</button>
      {#if anchorSummary?.facing}<p class="facing-badge">Facing <strong>{anchorSummary.facing?.toUpperCase()}</strong>{#if anchorSummary.facingStatus} · {anchorSummary.facingStatus}{/if}{#if anchorSummary.facingConfidence} · {Math.round(anchorSummary.facingConfidence * 100)}%{/if}</p>{/if}
      {#if anchorSummary && anchorSummary.facing !== "w" && anchorSummary.view !== "top_down"}<button class="wide" onclick={flipToCanonical} disabled={busy}>Flip to canonical (W)</button>{/if}
      <p class="muted">Promoted anchors define canvas size and foot baseline for follow-up animations.</p></section>
    {#if anchorSummary}
      <section><h3>CHARACTER PROFILE</h3>
        {#if characterProfile}
          <dl class="profile">
            <div><dt>dHash</dt><dd>{characterProfile.dHash}</dd></div>
            <div><dt>Rig</dt><dd>{characterProfile.rigId ?? "—"}</dd></div>
            <div><dt>Motions</dt><dd>{characterProfile.directionFamilies.length ? characterProfile.directionFamilies.join(", ") : "—"}</dd></div>
            <div><dt>Palette</dt><dd class="palette-row">{#each characterProfile.palette.slice(0, 8) as color}<span style={`background:${color}`} title={color}></span>{/each}</dd></div>
          </dl>
        {:else}<p class="muted">Profile not built yet.</p>{/if}
        <button class="wide" onclick={rebuildCharacterProfile} disabled={busy}><RefreshCw size={12} /> Refresh profile</button>
        <button class="wide" onclick={exportProfile} disabled={busy || !characterProfile}><Download size={12} /> Export profile</button>
        <button class="wide" onclick={importProfile} disabled={busy}><Upload size={12} /> Import profile</button>
        <FacingPanel {workspaceId} slug={anchorSummary.slug} {onError} {onNotice} />
      </section>
    {/if}
    <section><h3>ANIMATIONS</h3>{#if usedBy.length}<div class="used-list">{#each usedBy as animation}<div><Layers3 size={12} /><span>{animation.name}</span><small>{animation.frames.filter(frame => frame.assetId === asset.id).length} frame(s)</small></div>{/each}</div>{:else}<p class="muted">This image is not used by an animation.</p>{/if}</section>
    <section><h3>VERSIONS</h3>{#if versions.length}<div class="version-list">{#each versions as version}<div class:selected={version.selected}><span>v{version.versionNumber}</span><strong>{version.changeKind}</strong><small>{version.available ? new Date(version.createdAt).toLocaleString() : "Metadata only"}</small></div>{/each}</div>{:else}<p class="muted">No indexed versions yet.</p>{/if}</section>
    <section class="danger"><button onclick={remove} disabled={busy}><Trash2 size={12} /> Remove from project</button><p class="muted">Deletes the file on disk and removes it from animations, anchors, and generation manifests when needed.</p></section>
  </div>
</aside>
{#if promoteDialogOpen}<PromoteAnchorDialog busy={busy} onClose={() => promoteDialogOpen = false} onConfirm={promoteAnchor} />{/if}

<style>
  .inspector{width:264px;min-width:264px;height:100%;border-left:1px solid var(--border);background:var(--sidebar);display:flex;flex-direction:column}.inspector>header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 10px 0 13px;font-size:12px;color:var(--muted)}header>div{display:flex;gap:7px;align-items:center}header button,.name-row button{border:0;background:transparent;color:var(--faint);width:26px;height:26px;display:grid;place-items:center;border-radius:4px;cursor:pointer}header button:hover,.name-row button:hover{background:var(--surface-hover);color:var(--text)}.scroll{overflow:auto;flex:1}.preview{height:210px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border-bottom:1px solid var(--border)}.preview img{max-width:86%;max-height:86%;object-fit:contain;image-rendering:pixelated}
  section{padding:14px;border-bottom:1px solid var(--border)}.name-row{display:flex;justify-content:space-between;align-items:center;gap:6px}.name-row h2{font-size:12px;margin:0;font-weight:600;overflow-wrap:anywhere}.name-row p{font-size:11px;color:var(--faint);margin:3px 0 0;text-transform:capitalize}.name-row input{min-width:0;flex:1;height:27px;border:1px solid var(--accent);background:var(--bg);border-radius:4px;color:var(--text);font:inherit;font-size:12px;padding:0 7px;outline:0}h3{font-size:10px;letter-spacing:.14em;color:var(--faint);margin:0 0 11px}dl{margin:0;display:flex;flex-direction:column;gap:9px}dl div{display:flex;justify-content:space-between;gap:10px;font-size:11px}dt{color:var(--faint)}dd{margin:0;color:var(--muted);text-align:right}.path{font-size:11px;line-height:1.45;color:var(--muted);overflow-wrap:anywhere;margin:0 0 10px}.wide{height:28px;width:100%;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;gap:6px;align-items:center;justify-content:center;font:inherit;font-size:11px;cursor:pointer}.wide:hover{color:var(--text);border-color:var(--border-strong)}.wide.anchor{border-color:var(--accent);color:var(--text)}.facing-badge{font-size:11px;color:var(--muted);margin:8px 0 0}.facing-badge strong{color:var(--text)}.used-list{display:flex;flex-direction:column;gap:8px}.used-list div{display:grid;grid-template-columns:13px minmax(0,1fr) auto;align-items:center;gap:5px;font-size:11px;color:var(--muted)}.used-list small{font-size:10px;color:var(--faint)}.version-list{display:flex;flex-direction:column;gap:5px}.version-list>div{display:grid;grid-template-columns:24px minmax(0,1fr) auto;gap:6px;align-items:center;padding:6px;border:1px solid var(--border);border-radius:4px;color:var(--muted)}.version-list>div.selected{border-color:var(--accent);background:var(--accent-dim)}.version-list span{font-size:10px;color:var(--faint)}.version-list strong{font-size:10px;font-weight:600;text-transform:capitalize}.version-list small{font-size:9px;color:var(--faint)}.muted{font-size:11px;line-height:1.5;color:var(--faint);margin:0}.profile{margin:0 0 10px;display:flex;flex-direction:column;gap:8px}.profile div{display:flex;justify-content:space-between;gap:8px;font-size:11px}.profile dt{color:var(--faint)}.profile dd{margin:0;color:var(--muted);text-align:right;overflow-wrap:anywhere}.palette-row{display:flex;gap:4px;justify-content:flex-end}.palette-row span{width:12px;height:12px;border:1px solid var(--border);border-radius:2px;display:inline-block}.danger{border-bottom:0}.danger button{border:0;background:transparent;color:#c56f6b;font:inherit;font-size:11px;display:flex;align-items:center;gap:7px;padding:4px;cursor:pointer}
</style>
