<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { BookImage, Check, ImagePlus, Trash2 } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage, type ReferenceCategory, type ReferenceImage } from "$lib/types";

  let { worktreeId, conversationId, references, activeIds, maximumActive = 0, onReferences, onActiveIds, onError, onNotice }: {
    worktreeId: string; conversationId?: string; references: ReferenceImage[]; activeIds: string[]; maximumActive?: number;
    onReferences: (references: ReferenceImage[]) => void; onActiveIds: (ids: string[]) => void;
    onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();

  let category = $state<ReferenceCategory>("character_appearance");
  let importing = $state(false);

  const categories: {id:ReferenceCategory;label:string}[] = [
    {id:"character_appearance",label:"Character appearance"},{id:"clothing",label:"Clothing"},{id:"face",label:"Face"},
    {id:"weapon",label:"Weapon"},{id:"pose",label:"Pose"},{id:"art_style",label:"Art style"},{id:"environment",label:"Environment"},
    {id:"palette",label:"Palette"},{id:"animation",label:"Animation"},{id:"vfx",label:"VFX"},{id:"anatomy",label:"Anatomy"},
    {id:"lighting",label:"Lighting"},{id:"other",label:"Other"},
  ];
  const labelFor = (value:ReferenceCategory) => categories.find(item=>item.id===value)?.label ?? value;

  async function importImages() {
    const selected=await open({multiple:true,directory:false,title:"Import reference images",filters:[{name:"Images",extensions:["png","jpg","jpeg","webp","gif"]}]});
    const paths=typeof selected==="string"?[selected]:selected??[];
    if(!paths.length)return;
    importing=true;
    try {
      const created=[];
      for(const path of paths)created.push(await api.importReferenceImage(worktreeId,path,category));
      onReferences([...created,...references]);onNotice(`Imported ${created.length} reference${created.length===1?"":"s"}`);
    } catch(error){onError(errorMessage(error));} finally{importing=false;}
  }

  async function toggle(reference:ReferenceImage) {
    if(!conversationId){onError("Create or select a chat before activating references");return;}
    const active=activeIds.includes(reference.id);
    if(!active && maximumActive>0 && activeIds.length>=maximumActive){onError(`This provider supports up to ${maximumActive} active references`);return;}
    try{await api.setConversationReference(conversationId,reference.id,!active);onActiveIds(active?activeIds.filter(id=>id!==reference.id):[...activeIds,reference.id]);}
    catch(error){onError(errorMessage(error));}
  }

  async function remove(reference:ReferenceImage) {
    if(!window.confirm(`Delete the copied reference “${reference.name}”?`))return;
    try{await api.deleteReferenceImage(reference.id);onReferences(references.filter(item=>item.id!==reference.id));onActiveIds(activeIds.filter(id=>id!==reference.id));onNotice("Reference deleted");}
    catch(error){onError(errorMessage(error));}
  }
</script>

<section class="reference-library">
  <header><div><h1>Reference library</h1><p>{references.length} image{references.length===1?"":"s"} · {activeIds.length} active for this chat</p></div><div class="actions"><select bind:value={category} aria-label="Import category">{#each categories as item}<option value={item.id}>{item.label}</option>{/each}</select><button class="primary" onclick={importImages} disabled={importing}><ImagePlus size={14}/>{importing?"Importing…":"Import images"}</button></div></header>
  {#if references.length}
    <div class="grid">
      {#each references as reference}
        {@const active=activeIds.includes(reference.id)}
        <article class:active>
          <button class="preview" onclick={()=>toggle(reference)} title={active?"Remove from this chat":"Use in this chat"}><img src={assetUrl(reference.path)} alt={reference.name}/><span class="check">{#if active}<Check size={13}/>{/if}</span></button>
          <div class="meta"><strong>{reference.name}</strong><span>{labelFor(reference.category)} · {reference.width}×{reference.height}</span></div>
          <button class="delete" onclick={()=>remove(reference)} title="Delete reference"><Trash2 size={12}/></button>
        </article>
      {/each}
    </div>
  {:else}
    <div class="empty"><BookImage size={31} strokeWidth={1.3}/><h2>No references in this worktree</h2><p>Add appearance, pose, palette, weapon, environment, animation, or VFX images. Click a thumbnail to activate it for the current chat.</p><button onclick={importImages}><ImagePlus size={14}/> Import first reference</button></div>
  {/if}
</section>

<style>
  .reference-library{height:100%;display:flex;flex-direction:column;background:var(--bg)}header{height:58px;min-height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 18px}h1{font-size:14px;margin:0}header p{font-size:11px;color:var(--faint);margin:4px 0 0}.actions{display:flex;gap:7px}.actions select,.actions button,.empty button{height:31px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:6px;padding:0 9px;font:inherit;font-size:11px}.actions button,.empty button{display:flex;align-items:center;gap:6px;cursor:pointer}.actions button.primary,.empty button{background:var(--text);color:var(--bg);border-color:var(--text)}button:disabled{opacity:.5}.grid{padding:18px;overflow:auto;display:grid;grid-template-columns:repeat(auto-fill,minmax(190px,1fr));align-content:start;gap:12px}article{position:relative;border:1px solid var(--border);background:var(--surface);border-radius:8px;overflow:hidden}article.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.preview{position:relative;width:100%;height:145px;border:0;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:14px 14px;background-position:0 0,0 7px,7px -7px,-7px 0;display:grid;place-items:center;cursor:pointer}.preview img{max-width:90%;max-height:90%;object-fit:contain;image-rendering:pixelated}.check{position:absolute;right:8px;top:8px;width:22px;height:22px;border-radius:6px;border:1px solid var(--border-strong);background:var(--surface);display:grid;place-items:center;color:var(--accent)}article.active .check{background:var(--accent);color:white;border-color:var(--accent)}.meta{padding:10px 34px 11px 11px}.meta strong,.meta span{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.meta strong{font-size:12px}.meta span{font-size:10px;color:var(--faint);margin-top:4px}.delete{position:absolute;right:8px;bottom:10px;width:24px;height:24px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:4px;cursor:pointer}.delete:hover{background:var(--surface-hover);color:#cf7772}.empty{height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;color:var(--faint)}.empty h2{font-size:18px;color:var(--text);margin:17px 0 7px}.empty p{max-width:470px;font-size:12px;line-height:1.55;margin:0 0 20px}
</style>
