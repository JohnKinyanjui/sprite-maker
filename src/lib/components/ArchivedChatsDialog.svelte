<script lang="ts">
  import { ArchiveRestore, LoaderCircle, X } from "lucide-svelte";
  import type { Conversation, Worktree } from "$lib/types";

  let { conversations, worktrees, loading = false, restoringId, onRestore, onClose }: {
    conversations: Conversation[];
    worktrees: Worktree[];
    loading?: boolean;
    restoringId?: string;
    onRestore: (conversation: Conversation) => void | Promise<void>;
    onClose: () => void;
  } = $props();

  function worktreeName(conversation: Conversation) {
    return worktrees.find(worktree => worktree.id === conversation.worktreeId)?.name ?? "General";
  }

  function archivedDate(conversation: Conversation) {
    if (!conversation.archivedAt) return "Archived";
    const date = new Date(conversation.archivedAt);
    return Number.isNaN(date.getTime()) ? "Archived" : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
  }
</script>

<svelte:window onkeydown={(event)=>event.key==="Escape"&&onClose()}/>

<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="archived-chats-title">
    <header>
      <div><p>PROJECT HISTORY</p><h2 id="archived-chats-title">Archived chats</h2><span>Restore a chat to its original worktree and continue where you left off.</span></div>
      <button class="icon" onclick={onClose} aria-label="Close archived chats"><X size={16}/></button>
    </header>

    <div class="list">
      {#if loading}
        <div class="status"><LoaderCircle size={16}/>Loading archived chats…</div>
      {:else if !conversations.length}
        <div class="empty"><ArchiveRestore size={22}/><strong>No archived chats</strong><span>Chats you archive will appear here.</span></div>
      {:else}
        {#each conversations as conversation}
          <article>
            <div><strong>{conversation.title}</strong><span>{worktreeName(conversation)} · {archivedDate(conversation)}</span></div>
            <button onclick={()=>onRestore(conversation)} disabled={Boolean(restoringId)} aria-label={`Restore ${conversation.title}`}><ArchiveRestore size={14}/>{restoringId===conversation.id ? "Restoring…" : "Restore"}</button>
          </article>
        {/each}
      {/if}
    </div>

    <footer><button onclick={onClose}>Close</button></footer>
  </div>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:62;background:#000a;display:grid;place-items:center}.dialog{width:min(560px,calc(100vw - 32px));max-height:min(660px,calc(100vh - 40px));display:flex;flex-direction:column;background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 28px 80px #000a;color:var(--text);padding:21px}header{display:flex;align-items:flex-start;justify-content:space-between;padding-bottom:17px;border-bottom:1px solid var(--border)}header p{font-size:10px;letter-spacing:.13em;color:var(--accent);font-weight:700;margin:0 0 7px}h2{font-size:19px;line-height:1.2;margin:0}header span{display:block;font-size:12px;color:var(--faint);margin-top:7px}.icon{width:29px;height:29px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:6px;cursor:pointer}.icon:hover{background:var(--surface-hover);color:var(--text)}.list{min-height:170px;overflow:auto;padding:10px 0}.status,.empty{min-height:170px;display:flex;align-items:center;justify-content:center;color:var(--faint);font-size:12px}.status{gap:8px}.status :global(svg){animation:spin .8s linear infinite}.empty{flex-direction:column;gap:6px}.empty strong{color:var(--muted);font-size:13px}.empty span{font-size:11px}article{min-height:58px;display:flex;align-items:center;gap:14px;padding:8px 9px;border-radius:7px}article:hover{background:var(--surface-hover)}article>div{min-width:0;flex:1}article strong,article span{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}article strong{font-size:13px;font-weight:600}article span{font-size:10px;color:var(--faint);margin-top:4px}article button,footer button{height:32px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--muted);font:inherit;font-size:11px;padding:0 10px;display:flex;align-items:center;gap:6px;cursor:pointer}article button:hover{color:var(--text);border-color:var(--accent)}button:disabled{opacity:.5;cursor:not-allowed}footer{display:flex;justify-content:flex-end;border-top:1px solid var(--border);padding-top:14px}@keyframes spin{to{transform:rotate(360deg)}}
</style>
