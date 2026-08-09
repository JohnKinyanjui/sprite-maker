<script lang="ts">
  import { ChevronsUpDown, PanelLeftClose, Settings } from "lucide-svelte";
  import ConversationRenameDialog from "$lib/components/ConversationRenameDialog.svelte";
  import WorktreeExplorer from "$lib/components/WorktreeExplorer.svelte";
  import type { Conversation, Workspace, Worktree } from "$lib/types";

  let { workspace, worktrees, selectedWorktreeId, conversations, selectedConversationId, onWorktree, onNewWorktree, onConversation, onNewConversation, onRenameConversation, onArchiveConversation, onSettings, onHome, onCollapse }: {
    workspace: Workspace; worktrees: Worktree[]; selectedWorktreeId?: string; conversations: Conversation[]; selectedConversationId?: string;
    onWorktree: (worktree: Worktree) => void | Promise<void>; onNewWorktree: () => void;
    onConversation: (conversation: Conversation) => void | Promise<void>; onNewConversation: (worktree: Worktree) => void | Promise<void>;
    onRenameConversation: (conversation: Conversation, title: string) => void | Promise<void>;
    onArchiveConversation: (conversation: Conversation) => void | Promise<void>;
    onSettings: () => void; onHome: () => void; onCollapse: () => void;
  } = $props();
  let renameTarget = $state<Conversation>();
  let renaming = $state(false);

  async function rename(title: string) {
    if (!renameTarget) return;
    renaming = true;
    try { await onRenameConversation(renameTarget, title); renameTarget = undefined; }
    finally { renaming = false; }
  }
</script>

<aside class="sidebar">
  <div class="workspace-switcher">
    <button class="workspace-button" onclick={onHome} title="Change workspace">
      <span class="workspace-mark">{workspace.name.slice(0, 1).toUpperCase()}</span>
      <span><strong>{workspace.name}</strong><small>{workspace.path}</small></span><ChevronsUpDown size={14}/>
    </button>
    <button class="icon-button" onclick={onCollapse} title="Collapse sidebar" aria-label="Collapse sidebar"><PanelLeftClose size={16}/></button>
  </div>

  <div class="scroll">
    <WorktreeExplorer {worktrees} {conversations} {selectedWorktreeId} {selectedConversationId} onSelectWorktree={onWorktree} onCreateWorktree={onNewWorktree} onSelectConversation={onConversation} onCreateConversation={onNewConversation} onRenameConversation={(conversation)=>renameTarget=conversation} {onArchiveConversation}/>
  </div>

  <footer><button onclick={onSettings}><Settings size={15}/><span>Settings</span></button></footer>
</aside>

{#if renameTarget}<ConversationRenameDialog conversation={renameTarget} busy={renaming} onRename={rename} onClose={()=>renameTarget=undefined}/>{/if}

<style>
  .sidebar{width:270px;min-width:270px;height:100%;background:var(--sidebar);border-right:1px solid var(--border);display:flex;flex-direction:column;color:var(--text);overflow:hidden}.workspace-switcher{height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;padding:0 10px;gap:4px}.workspace-button{min-width:0;flex:1;display:grid;grid-template-columns:32px minmax(0,1fr) 15px;align-items:center;gap:9px;border:0;background:transparent;color:var(--text);text-align:left;padding:6px;border-radius:8px;cursor:pointer}.workspace-button:hover,.icon-button:hover{background:var(--surface-hover)}.workspace-mark{width:30px;height:30px;border-radius:7px;background:var(--accent-dim);color:var(--accent);display:grid;place-items:center;font-size:14px;font-weight:700}.workspace-button strong,.workspace-button small{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.workspace-button strong{font-size:14px;font-weight:640}.workspace-button small{font-size:11px;color:var(--faint);margin-top:2px}.icon-button{width:32px;height:32px;border:0;background:transparent;color:var(--faint);border-radius:6px;display:grid;place-items:center;cursor:pointer}.scroll{flex:1;overflow:auto;padding:16px 9px}footer{height:54px;border-top:1px solid var(--border);padding:8px 10px;box-sizing:border-box}footer button{height:38px;width:100%;border:1px solid transparent;background:transparent;color:var(--muted);display:flex;align-items:center;gap:9px;border-radius:7px;padding:0 10px;font:inherit;font-size:14px;cursor:pointer;text-align:left}footer button:hover{background:var(--surface-hover);color:var(--text)}
</style>
