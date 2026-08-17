<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Plus, X } from "lucide-svelte";
  import { api } from "$lib/api";
  import { errorMessage, type Workspace } from "$lib/types";

  let { onCreated, onClose, onError }: { onCreated:(workspace:Workspace)=>void|Promise<void>; onClose:()=>void; onError:(message:string)=>void } = $props();
  let mode = $state<"create"|"open">("create");
  let name = $state("");
  let path = $state("");
  let busy = $state(false);

  async function chooseDirectory(){
    const selected=await open({directory:true,multiple:false,title:mode==="create"?"Choose a project directory":"Open a Sprite Studio project"});
    if(typeof selected!=="string")return;
    if(mode==="open"){
      busy=true;
      try{await onCreated(await api.openWorkspace(selected));onClose();}catch(error){onError(errorMessage(error));}finally{busy=false;}
      return;
    }
    path=selected;if(!name)name=selected.split(/[\\/]/).filter(Boolean).at(-1)??"New project";
  }
  async function create(){if(!name.trim()||!path)return;busy=true;try{await onCreated(await api.createWorkspace(name.trim(),path));onClose();}catch(error){onError(errorMessage(error));}finally{busy=false;}}
</script>

<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&!busy&&onClose()}>
  <form class="dialog" onsubmit={(event)=>{event.preventDefault();create();}}>
    <header><div><span>PROJECTS</span><h2>Add a project</h2></div><button type="button" onclick={onClose} aria-label="Close"><X size={16}/></button></header>
    <div class="mode"><button type="button" class:active={mode==="create"} onclick={()=>mode="create"}><Plus size={14}/>New project</button><button type="button" class:active={mode==="open"} onclick={()=>mode="open"}><FolderOpen size={14}/>Open folder</button></div>
    {#if mode==="create"}
      <label>Project name<input bind:value={name} placeholder="My game"/></label>
      <label>Project folder<div class="path"><input value={path} readonly placeholder="Choose a local folder"/><button type="button" onclick={chooseDirectory}>Browse</button></div></label>
      <footer><button type="button" onclick={onClose}>Cancel</button><button class="primary" disabled={busy||!name.trim()||!path}>{busy?"Creating…":"Create project"}</button></footer>
    {:else}
      <div class="open"><FolderOpen size={30}/><strong>Open an existing project folder</strong><p>Sprite Studio will register it under Projects and keep all existing files in place.</p><button type="button" class="primary" disabled={busy} onclick={chooseDirectory}>{busy?"Opening…":"Choose folder"}</button></div>
    {/if}
  </form>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:60;background:#000a;display:grid;place-items:center}.dialog{width:min(480px,calc(100vw - 32px));background:var(--surface);border:1px solid var(--border-strong);border-radius:12px;box-shadow:0 28px 80px #000b;padding:22px}.dialog header{display:flex;align-items:flex-start;justify-content:space-between}.dialog header span{font-size:9px;letter-spacing:.16em;color:var(--faint);font-weight:700}.dialog h2{font-size:19px;margin:6px 0 0}.dialog header button{width:29px;height:29px;border:0;border-radius:6px;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}.dialog header button:hover{background:var(--surface-hover);color:var(--text)}.mode{display:grid;grid-template-columns:1fr 1fr;gap:6px;margin:22px 0}.mode button{height:38px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--muted);display:flex;align-items:center;justify-content:center;gap:7px;font:inherit;font-size:12px;cursor:pointer}.mode button.active{border-color:var(--accent);background:var(--accent-dim);color:var(--text)}label{display:block;color:var(--muted);font-size:11px;margin-top:15px}input{width:100%;height:38px;margin-top:7px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--text);padding:0 10px;font:inherit;font-size:12px}.path{display:flex;gap:7px}.path input{min-width:0;flex:1}.path button,footer button,.open button{height:38px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--muted);padding:0 12px;font:inherit;font-size:12px;cursor:pointer}.path button{margin-top:7px}footer{display:flex;justify-content:flex-end;gap:7px;margin-top:24px}.primary{background:var(--accent)!important;border-color:var(--accent)!important;color:#111!important;font-weight:650}.primary:disabled{opacity:.45}.open{min-height:220px;border:1px dashed var(--border-strong);border-radius:8px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;color:var(--faint);padding:24px}.open strong{font-size:13px;color:var(--text);margin-top:12px}.open p{max-width:300px;font-size:11px;line-height:1.55;margin:6px 0 18px}
</style>
