<script lang="ts">
  import { Pencil, Trash2, X } from "lucide-svelte";
  import { CREATABLE_WORKTREE_KINDS, type CreatableWorktreeKind } from "$lib/worktree-kinds";
  import type { Worktree } from "$lib/types";

  let { worktree, busy = false, onSave, onDelete, onClose }: {
    worktree: Worktree;
    busy?: boolean;
    onSave: (name: string, kind: CreatableWorktreeKind, description?: string) => void | Promise<void>;
    onDelete: () => void | Promise<void>;
    onClose: () => void;
  } = $props();

  let initializedId = $state("");
  let name = $state("");
  let kind = $state<CreatableWorktreeKind>("object");
  let description = $state("");
  let confirmingDelete = $state(false);

  $effect(() => {
    if (initializedId === worktree.id) return;
    initializedId = worktree.id;
    name = worktree.name;
    kind = worktree.kind as CreatableWorktreeKind;
    description = worktree.description ?? "";
    confirmingDelete = false;
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!name.trim() || busy) return;
    onSave(name.trim(), kind, description.trim() || undefined);
  }
</script>

<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&onClose()}>
  <form class="dialog" onsubmit={submit} aria-labelledby="manage-worktree-title">
    <header>
      <div><p>WORKTREE</p><h2 id="manage-worktree-title">Manage {worktree.name}</h2><span>Change how new work in this worktree is routed.</span></div>
      <button type="button" class="icon" onclick={onClose} aria-label="Close"><X size={15}/></button>
    </header>

    <label>Name<input bind:value={name} disabled={busy}/></label>
    <label>Type
      <select bind:value={kind} disabled={busy}>
        {#each CREATABLE_WORKTREE_KINDS as option}<option value={option.value}>{option.label}</option>{/each}
      </select>
    </label>
    <p class="description">{CREATABLE_WORKTREE_KINDS.find(option=>option.value===kind)?.description}</p>
    <label>Description<textarea bind:value={description} rows="3" disabled={busy} placeholder="Optional notes for this worktree"></textarea></label>

    <div class="danger">
      {#if confirmingDelete}
        <p>Chats, references, animations, and asset links will move to General. Files stay on disk.</p>
        <div><button type="button" onclick={()=>confirmingDelete=false} disabled={busy}>Keep worktree</button><button type="button" class="confirm-delete" onclick={onDelete} disabled={busy}><Trash2 size={13}/>{busy ? "Deleting…" : "Move contents and delete"}</button></div>
      {:else}
        <button type="button" class="delete" onclick={()=>confirmingDelete=true} disabled={busy}><Trash2 size={13}/>Delete worktree…</button>
      {/if}
    </div>

    <footer><button type="button" onclick={onClose} disabled={busy}>Cancel</button><button class="primary" disabled={!name.trim()||busy}><Pencil size={13}/>{busy ? "Saving…" : "Save changes"}</button></footer>
  </form>
</div>

<style>
  .backdrop{position:fixed;inset:0;background:#0009;display:grid;place-items:center;z-index:60}.dialog{width:min(430px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:9px;box-shadow:0 24px 70px #000a;padding:20px;color:var(--text)}header{display:flex;align-items:flex-start;justify-content:space-between;margin-bottom:19px}header p{font-size:10px;letter-spacing:.14em;color:var(--accent);font-weight:750;margin:0 0 7px}h2{font-size:17px;line-height:1.25;margin:0}header span{display:block;color:var(--faint);font-size:11px;margin-top:5px}.icon{width:27px;height:27px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:5px;cursor:pointer}.icon:hover{background:var(--surface-hover);color:var(--text)}label{display:block;font-size:11px;color:var(--muted);margin-top:13px}input,select,textarea{display:block;width:100%;margin-top:7px;background:var(--bg);border:1px solid var(--border-strong);border-radius:5px;color:var(--text);font:inherit;font-size:12px;outline:0}input,select{height:37px;padding:0 10px}textarea{padding:9px 10px;resize:vertical;min-height:66px}input:focus,select:focus,textarea:focus{border-color:var(--accent)}.description{font-size:10px;color:var(--faint);margin:6px 1px 0}.danger{border-top:1px solid var(--border);margin-top:19px;padding-top:14px}.danger>p{font-size:11px;line-height:1.45;color:var(--muted);margin:0 0 10px}.danger>div{display:flex;justify-content:flex-end;gap:7px}.danger button{height:32px;border:1px solid var(--border);background:var(--bg);color:var(--muted);border-radius:5px;padding:0 10px;font:inherit;font-size:11px;cursor:pointer}.danger .delete,.danger .confirm-delete{color:#d97a75;display:flex;align-items:center;gap:6px}.danger .confirm-delete{border-color:#7f3d3d;background:#351d1d}footer{display:flex;justify-content:flex-end;gap:7px;margin-top:18px}footer button{height:34px;border:1px solid var(--border);background:transparent;color:var(--muted);border-radius:5px;padding:0 12px;font:inherit;font-size:11px;cursor:pointer}footer .primary{background:var(--text);color:var(--bg);border-color:var(--text);display:flex;align-items:center;gap:6px}button:disabled,input:disabled,select:disabled,textarea:disabled{opacity:.55;cursor:not-allowed}
</style>
