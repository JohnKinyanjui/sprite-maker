<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Plus, ArrowRight, HardDrive, Bot, Box } from "lucide-svelte";
  import LogoMark from "$lib/components/LogoMark.svelte";
  import { api } from "$lib/api";
  import { errorMessage, type Workspace } from "$lib/types";

  let { workspaces, onOpen, onCreated, onError }: {
    workspaces: Workspace[];
    onOpen: (workspace: Workspace) => void;
    onCreated: (workspace: Workspace) => void;
    onError: (message: string) => void;
  } = $props();

  let creating = $state(false);
  let name = $state("");
  let path = $state("");
  let busy = $state(false);

  async function chooseCreateDirectory() {
    const selected = await open({ directory: true, multiple: false, title: "Choose a workspace directory" });
    if (typeof selected === "string") {
      path = selected;
      if (!name) name = selected.split(/[\\/]/).filter(Boolean).at(-1) ?? "New workspace";
    }
  }

  async function create() {
    if (!name.trim() || !path) return;
    busy = true;
    try {
      onCreated(await api.createWorkspace(name, path));
      creating = false;
    } catch (error) { onError(errorMessage(error)); }
    finally { busy = false; }
  }

  async function openExisting() {
    const selected = await open({ directory: true, multiple: false, title: "Open a Sprite Studio workspace" });
    if (typeof selected !== "string") return;
    try { onCreated(await api.openWorkspace(selected)); }
    catch (error) { onError(errorMessage(error)); }
  }
</script>

<main class="welcome">
  <section class="intro">
    <div class="mark"><LogoMark size={27} /></div>
    <p class="eyebrow">SPRITE STUDIO</p>
    <h1>Build game assets with<br />the tools you already own.</h1>
    <p class="lede">A local-first workspace for creating, animating, testing, and exporting 2D assets alongside your AI agent.</p>
    <div class="actions">
      <button class="primary" onclick={() => creating = true}><Plus size={16} /> New workspace</button>
      <button onclick={openExisting}><FolderOpen size={16} /> Open folder</button>
    </div>
    <div class="principles">
      <span><HardDrive size={14} /> Local files</span><span><Bot size={14} /> Bring your AI</span><span><Box size={14} /> Engine-ready exports</span>
    </div>
  </section>

  <aside class="recent">
    <div class="recent-heading"><h2>Recent workspaces</h2><span>{workspaces.length}</span></div>
    {#if workspaces.length}
      <div class="workspace-list">
        {#each workspaces as workspace}
          <button class="workspace" onclick={() => onOpen(workspace)}>
            <div class="folder"><FolderOpen size={17} /></div>
            <div><strong>{workspace.name}</strong><small>{workspace.path}</small></div>
            <ArrowRight class="arrow" size={16} />
          </button>
        {/each}
      </div>
    {:else}
      <div class="empty">
        <FolderOpen size={25} strokeWidth={1.4} />
        <strong>No recent workspaces</strong>
        <p>Create a workspace or open an existing asset folder to get started.</p>
      </div>
    {/if}
  </aside>
</main>

{#if creating}
  <div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && (creating = false)}>
    <form class="dialog" onsubmit={(event) => { event.preventDefault(); create(); }}>
      <div><p class="eyebrow">NEW WORKSPACE</p><h2>Create an asset workspace</h2><p>Sprite files stay in this folder. Project metadata is stored locally by Sprite Studio.</p></div>
      <label>Name<input bind:value={name} placeholder="First Exile" /></label>
      <label>Directory<div class="path-field"><input value={path} readonly placeholder="Choose a local directory" /><button type="button" onclick={chooseCreateDirectory}>Browse</button></div></label>
      <div class="dialog-actions"><button type="button" onclick={() => creating = false}>Cancel</button><button class="primary" disabled={!name.trim() || !path || busy}>{busy ? "Creating…" : "Create workspace"}</button></div>
    </form>
  </div>
{/if}

<style>
  .welcome{min-height:100vh;display:grid;grid-template-columns:minmax(520px,1.25fr) minmax(340px,.75fr);background:var(--bg);color:var(--text)}
  .intro{padding:clamp(70px,12vh,130px) clamp(56px,8vw,128px);display:flex;flex-direction:column;align-items:flex-start;justify-content:center;border-right:1px solid var(--border)}
  .mark{--logo-pixel:var(--text);width:46px;height:46px;border:1px solid var(--border-strong);display:grid;place-items:center;border-radius:10px;background:var(--surface);margin-bottom:25px;color:#f5a524}
  .eyebrow{font-size:12px;letter-spacing:.18em;font-weight:700;color:var(--muted);margin:0 0 14px}
  h1{font-size:clamp(40px,4.2vw,64px);line-height:1.02;letter-spacing:-.045em;margin:0;max-width:760px;font-weight:630}
  .lede{font-size:16px;line-height:1.65;color:var(--muted);max-width:570px;margin:28px 0 30px}
  .actions{display:flex;gap:9px}.actions button,.dialog-actions button,.path-field button{height:38px;padding:0 14px;border:1px solid var(--border-strong);border-radius:7px;background:var(--surface);color:var(--text);font:inherit;font-size:13px;display:flex;align-items:center;gap:8px;cursor:pointer}
  button.primary{background:var(--text);color:var(--bg);border-color:var(--text)} button:disabled{opacity:.45;cursor:not-allowed}
  .principles{display:flex;gap:24px;color:var(--muted);font-size:12px;margin-top:46px}.principles span{display:flex;gap:7px;align-items:center}
  .recent{padding:clamp(60px,10vh,100px) clamp(36px,5vw,70px);display:flex;flex-direction:column;justify-content:center;min-width:0}
  .recent-heading{display:flex;align-items:center;justify-content:space-between;margin-bottom:16px}.recent-heading h2{font-size:13px;font-weight:600;margin:0}.recent-heading span{font-size:12px;color:var(--faint)}
  .workspace-list{display:flex;flex-direction:column}.workspace{width:100%;display:grid;grid-template-columns:36px minmax(0,1fr) 16px;gap:11px;text-align:left;align-items:center;padding:12px 8px;border:0;border-top:1px solid var(--border);background:transparent;color:var(--text);cursor:pointer}.workspace:last-child{border-bottom:1px solid var(--border)}.workspace:hover{background:var(--surface-hover)}
  .workspace .folder{width:34px;height:34px;border:1px solid var(--border);border-radius:6px;display:grid;place-items:center;color:var(--muted)}.workspace strong,.workspace small{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.workspace strong{font-size:12px;font-weight:570}.workspace small{font-size:12px;color:var(--faint);margin-top:3px}.workspace :global(.arrow){color:var(--faint)}
  .empty{border:1px dashed var(--border-strong);border-radius:8px;min-height:190px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;color:var(--faint);padding:24px}.empty strong{font-size:12px;color:var(--muted);margin-top:12px}.empty p{font-size:12px;line-height:1.5;max-width:230px}
  .backdrop{position:fixed;inset:0;background:#0008;display:grid;place-items:center;z-index:30}.dialog{width:min(460px,calc(100vw - 32px));background:var(--surface);border:1px solid var(--border-strong);box-shadow:0 24px 70px #0007;border-radius:10px;padding:24px;display:flex;flex-direction:column;gap:20px}.dialog h2{font-size:19px;margin:0 0 8px}.dialog p:not(.eyebrow){font-size:12px;line-height:1.5;color:var(--muted);margin:0}.dialog label{font-size:12px;color:var(--muted);display:flex;flex-direction:column;gap:7px}.dialog input{height:38px;box-sizing:border-box;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--text);padding:0 11px;font:inherit;font-size:12px;outline:none}.dialog input:focus{border-color:var(--accent)}.path-field{display:flex;gap:7px}.path-field input{flex:1;min-width:0}.dialog-actions{display:flex;justify-content:flex-end;gap:8px;margin-top:4px}
  @media(max-width:850px){.welcome{grid-template-columns:1fr}.intro{border-right:0;border-bottom:1px solid var(--border);padding:60px 36px}.recent{padding:36px}.principles{flex-wrap:wrap}.recent{justify-content:flex-start}}
</style>
