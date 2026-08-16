<script lang="ts">
  import { Archive, CircleHelp, Folder, FolderOpen, FolderTree, LoaderCircle, MoreHorizontal, Pencil, Plus } from "lucide-svelte";
  import type { Conversation, Worktree } from "$lib/types";

  let { worktrees, conversations, selectedWorktreeId, selectedConversationId, runningConversationIds, onSelectWorktree, onCreateWorktree, onManageWorktree, onSelectConversation, onCreateConversation, onRenameConversation, onArchiveConversation }: {
    worktrees: Worktree[];
    conversations: Conversation[];
    selectedWorktreeId?: string;
    selectedConversationId?: string;
    runningConversationIds: string[];
    onSelectWorktree: (worktree: Worktree) => void | Promise<void>;
    onCreateWorktree: () => void;
    onManageWorktree: (worktree: Worktree) => void;
    onSelectConversation: (conversation: Conversation) => void | Promise<void>;
    onCreateConversation: (worktree: Worktree) => void | Promise<void>;
    onRenameConversation: (conversation: Conversation) => void;
    onArchiveConversation: (conversation: Conversation) => void | Promise<void>;
  } = $props();
  let showHelp = $state(false);
  let expandedIds = $state<string[]>([]);

  $effect(() => {
    if (selectedWorktreeId && !expandedIds.includes(selectedWorktreeId)) expandedIds = [...expandedIds, selectedWorktreeId];
  });

  function chatsFor(worktreeId: string) {
    return conversations.filter(conversation => conversation.worktreeId === worktreeId);
  }

  function chooseWorktree(worktree: Worktree) {
    if (!expandedIds.includes(worktree.id)) expandedIds = [...expandedIds, worktree.id];
    onSelectWorktree(worktree);
  }

  function newChat(event: MouseEvent, worktree: Worktree) {
    event.stopPropagation();
    if (!expandedIds.includes(worktree.id)) expandedIds = [...expandedIds, worktree.id];
    onCreateConversation(worktree);
  }

  function manage(event: MouseEvent, worktree: Worktree) {
    event.stopPropagation();
    onManageWorktree(worktree);
  }
</script>

<section class="worktrees">
  <div class="section-title">
    <span>Project</span>
    <div><button onclick={()=>showHelp=!showHelp} title="What is a worktree?" aria-label="Explain worktrees"><CircleHelp size={15}/></button><button onclick={onCreateWorktree} title="New worktree" aria-label="New worktree"><Plus size={16}/></button></div>
  </div>
  {#if showHelp}<div class="help"><FolderTree size={16}/><p><strong>Worktrees keep game assets organized.</strong> Each folder owns its chats, references, sprites, animations, and exports.</p></div>{/if}
  <nav aria-label="Project worktrees and chats">
    {#each worktrees as worktree}
      {@const expanded = expandedIds.includes(worktree.id)}
      {@const worktreeChats = chatsFor(worktree.id)}
      <div class="worktree-group">
        <div class="folder-row" class:selected={worktree.id===selectedWorktreeId}>
          <button class="folder-button" onclick={()=>chooseWorktree(worktree)} title={`${worktree.name} — ${worktree.kind} worktree`} aria-expanded={expanded}>
            {#if expanded}<FolderOpen size={18}/>{:else}<Folder size={18}/>{/if}<span>{worktree.name}</span>
          </button>
          <div class="worktree-actions">
            <button onclick={(event)=>newChat(event,worktree)} title={`New chat in ${worktree.name}`} aria-label={`New chat in ${worktree.name}`}><Plus size={15}/></button>
            {#if worktree.kind!=="general"}<button onclick={(event)=>manage(event,worktree)} title={`Manage ${worktree.name}`} aria-label={`Manage ${worktree.name}`}><MoreHorizontal size={15}/></button>{/if}
          </div>
        </div>
        {#if expanded}
          <div class="chat-list">
            {#each worktreeChats as conversation}
              <div class="chat-row" class:active={selectedConversationId===conversation.id}>
                <button class="chat-button" onclick={()=>onSelectConversation(conversation)} title={conversation.title} aria-current={selectedConversationId===conversation.id?"page":undefined}><span>{conversation.title}</span>{#if runningConversationIds.includes(conversation.id)}<i title="Generating in this chat"><LoaderCircle size={13}/></i>{/if}</button>
                <div class="chat-actions">
                  <button onclick={()=>onRenameConversation(conversation)} title={`Rename ${conversation.title}`} aria-label={`Rename ${conversation.title}`}><Pencil size={13}/></button>
                  <button onclick={()=>onArchiveConversation(conversation)} title={`Archive ${conversation.title}`} aria-label={`Archive ${conversation.title}`}><Archive size={13}/></button>
                </div>
              </div>
            {/each}
            {#if !worktreeChats.length}<p class="empty">No chats yet</p>{/if}
          </div>
        {/if}
      </div>
    {/each}
  </nav>
</section>

<style>
  .worktrees{padding-bottom:8px}.section-title{height:33px;display:flex;align-items:center;justify-content:space-between;padding:0 8px 4px;color:var(--muted);font-size:12px;font-weight:650}.section-title>div{display:flex;gap:1px}.section-title button{border:0;background:transparent;color:var(--faint);width:28px;height:28px;display:grid;place-items:center;cursor:pointer;border-radius:6px}.section-title button:hover{background:var(--surface-hover);color:var(--text)}.help{margin:0 2px 10px;padding:10px;border:1px solid var(--border);border-radius:7px;background:var(--surface);display:grid;grid-template-columns:18px 1fr;gap:8px;color:var(--accent)}.help p{font-size:10px;line-height:1.45;color:var(--faint);margin:0}.help strong{display:block;color:var(--text);font-size:11px;margin-bottom:3px}nav{display:flex;flex-direction:column;gap:3px}.worktree-group{min-width:0}.folder-row{height:39px;display:flex;align-items:center;border:1px solid transparent;border-radius:8px;transition:background-color .12s ease}.folder-row:hover,.folder-row.selected{background:var(--surface-hover);color:var(--text)}.folder-row.selected{border-color:#ffffff08}.folder-button{height:100%;min-width:0;flex:1;border:0;background:transparent;color:var(--muted);display:grid;grid-template-columns:20px minmax(0,1fr);align-items:center;gap:9px;padding:0 9px;font:inherit;text-align:left;cursor:pointer}.folder-row.selected .folder-button{color:var(--text)}.folder-button span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:14px;font-weight:560}.worktree-actions{display:flex;align-items:center;margin-right:4px;opacity:0}.folder-row:hover .worktree-actions,.folder-row:focus-within .worktree-actions{opacity:1}.worktree-actions button{width:27px;height:28px;border:0;background:transparent;color:var(--faint);border-radius:6px;display:grid;place-items:center;cursor:pointer}.worktree-actions button:hover{background:var(--selected);color:var(--text)}.chat-list{display:flex;flex-direction:column;gap:2px;margin:2px 0 5px 29px}.chat-row{height:35px;display:flex;align-items:center;border-radius:7px;min-width:0}.chat-row:hover,.chat-row.active{background:var(--surface-hover)}.chat-row.active{background:var(--selected);box-shadow:inset 0 1px 0 #ffffff08}.chat-button{height:100%;min-width:0;flex:1;border:0;background:transparent;color:var(--muted);font:inherit;font-size:13px;text-align:left;padding:0 8px;cursor:pointer;display:grid;grid-template-columns:minmax(0,1fr) 16px;align-items:center;gap:5px}.chat-row.active .chat-button{color:var(--text);font-weight:560}.chat-button span{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.chat-button i{display:grid;place-items:center;color:var(--accent);animation:spin .8s linear infinite}.chat-actions{display:flex;flex:0 0 58px;align-items:center;padding-right:4px;opacity:0}.chat-row:hover .chat-actions,.chat-row:focus-within .chat-actions{opacity:1}.chat-actions button{width:27px;min-width:27px;height:27px;border:0;background:transparent;color:var(--faint);border-radius:5px;display:grid;place-items:center;cursor:pointer}.chat-actions button:hover{background:var(--surface);color:var(--text)}.empty{height:31px;display:flex;align-items:center;margin:0;padding:0 8px;color:var(--faint);font-size:12px}@keyframes spin{to{transform:rotate(360deg)}}
</style>
