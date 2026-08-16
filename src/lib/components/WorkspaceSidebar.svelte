<script lang="ts">
  import { ArchiveRestore, ChevronsUpDown, PanelLeftClose, Settings } from "lucide-svelte";
  import ArchivedChatsDialog from "$lib/components/ArchivedChatsDialog.svelte";
  import ConversationRenameDialog from "$lib/components/ConversationRenameDialog.svelte";
  import WorktreeManageDialog from "$lib/components/WorktreeManageDialog.svelte";
  import WorktreeExplorer from "$lib/components/WorktreeExplorer.svelte";
  import type { CreatableWorktreeKind } from "$lib/worktree-kinds";
  import type { Conversation, Workspace, Worktree } from "$lib/types";

  let { workspace, worktrees, selectedWorktreeId, conversations, selectedConversationId, runningConversationIds, onWorktree, onNewWorktree, onUpdateWorktree, onDeleteWorktree, onConversation, onNewConversation, onRenameConversation, onArchiveConversation, onListArchivedConversations, onRestoreConversation, onSettings, onHome, onCollapse }: {
    workspace: Workspace; worktrees: Worktree[]; selectedWorktreeId?: string; conversations: Conversation[]; selectedConversationId?: string; runningConversationIds: string[];
    onWorktree: (worktree: Worktree) => void | Promise<void>; onNewWorktree: () => void;
    onUpdateWorktree: (worktree: Worktree, name: string, kind: CreatableWorktreeKind, description?: string) => void | Promise<void>;
    onDeleteWorktree: (worktree: Worktree) => void | Promise<void>;
    onConversation: (conversation: Conversation) => void | Promise<void>; onNewConversation: (worktree: Worktree) => void | Promise<void>;
    onRenameConversation: (conversation: Conversation, title: string) => void | Promise<void>;
    onArchiveConversation: (conversation: Conversation) => void | Promise<void>;
    onListArchivedConversations: () => Promise<Conversation[]>;
    onRestoreConversation: (conversation: Conversation) => void | Promise<void>;
    onSettings: () => void; onHome: () => void; onCollapse: () => void;
  } = $props();
  let renameTarget = $state<Conversation>();
  let renaming = $state(false);
  let manageTarget = $state<Worktree>();
  let managing = $state(false);
  let archiveOpen = $state(false);
  let archivedConversations = $state<Conversation[]>([]);
  let loadingArchive = $state(false);
  let restoringId = $state<string>();

  async function rename(title: string) {
    if (!renameTarget) return;
    renaming = true;
    try { await onRenameConversation(renameTarget, title); renameTarget = undefined; }
    finally { renaming = false; }
  }

  async function saveWorktree(name: string, kind: CreatableWorktreeKind, description?: string) {
    if (!manageTarget) return;
    managing = true;
    try { await onUpdateWorktree(manageTarget, name, kind, description); manageTarget = undefined; }
    catch { /* The parent surfaces a detailed toast and keeps the dialog open. */ }
    finally { managing = false; }
  }

  async function deleteWorktree() {
    if (!manageTarget) return;
    managing = true;
    try { await onDeleteWorktree(manageTarget); manageTarget = undefined; }
    catch { /* The parent surfaces a detailed toast and keeps the dialog open. */ }
    finally { managing = false; }
  }

  async function openArchive() {
    archiveOpen = true;
    loadingArchive = true;
    try { archivedConversations = await onListArchivedConversations(); }
    catch { archivedConversations = []; }
    finally { loadingArchive = false; }
  }

  async function restoreConversation(conversation: Conversation) {
    restoringId = conversation.id;
    try {
      await onRestoreConversation(conversation);
      archivedConversations = archivedConversations.filter(item => item.id !== conversation.id);
      archiveOpen = false;
    } catch { /* The parent surfaces a detailed toast and keeps the dialog open. */ }
    finally { restoringId = undefined; }
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
    <WorktreeExplorer {worktrees} {conversations} {selectedWorktreeId} {selectedConversationId} {runningConversationIds} onSelectWorktree={onWorktree} onCreateWorktree={onNewWorktree} onManageWorktree={(worktree)=>manageTarget=worktree} onSelectConversation={onConversation} onCreateConversation={onNewConversation} onRenameConversation={(conversation)=>renameTarget=conversation} {onArchiveConversation}/>
  </div>

  <footer><button onclick={openArchive}><ArchiveRestore size={15}/><span>Archived chats</span></button><button onclick={onSettings}><Settings size={15}/><span>Settings</span></button></footer>
</aside>

{#if renameTarget}<ConversationRenameDialog conversation={renameTarget} busy={renaming} onRename={rename} onClose={()=>renameTarget=undefined}/>{/if}
{#if manageTarget}<WorktreeManageDialog worktree={manageTarget} busy={managing} onSave={saveWorktree} onDelete={deleteWorktree} onClose={()=>manageTarget=undefined}/>{/if}
{#if archiveOpen}<ArchivedChatsDialog conversations={archivedConversations} {worktrees} loading={loadingArchive} {restoringId} onRestore={restoreConversation} onClose={()=>archiveOpen=false}/>{/if}

<style>
  .sidebar{width:270px;min-width:270px;height:100%;background:var(--sidebar);border-right:1px solid var(--border);display:flex;flex-direction:column;color:var(--text);overflow:hidden}.workspace-switcher{height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;padding:0 10px;gap:4px}.workspace-button{min-width:0;flex:1;display:grid;grid-template-columns:32px minmax(0,1fr) 15px;align-items:center;gap:9px;border:0;background:transparent;color:var(--text);text-align:left;padding:6px;border-radius:8px;cursor:pointer}.workspace-button:hover,.icon-button:hover{background:var(--surface-hover)}.workspace-mark{width:30px;height:30px;border-radius:7px;background:var(--accent-dim);color:var(--accent);display:grid;place-items:center;font-size:14px;font-weight:700}.workspace-button strong,.workspace-button small{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.workspace-button strong{font-size:14px;font-weight:640}.workspace-button small{font-size:11px;color:var(--faint);margin-top:2px}.icon-button{width:32px;height:32px;border:0;background:transparent;color:var(--faint);border-radius:6px;display:grid;place-items:center;cursor:pointer}.scroll{flex:1;overflow:auto;padding:16px 9px}footer{border-top:1px solid var(--border);padding:7px 10px;box-sizing:border-box}footer button{height:34px;width:100%;border:1px solid transparent;background:transparent;color:var(--muted);display:flex;align-items:center;gap:9px;border-radius:7px;padding:0 10px;font:inherit;font-size:13px;cursor:pointer;text-align:left}footer button:hover{background:var(--surface-hover);color:var(--text)}
</style>
