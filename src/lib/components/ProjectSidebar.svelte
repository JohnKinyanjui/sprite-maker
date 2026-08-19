<script lang="ts">
  import {
    Archive,
    ArchiveRestore,
    Blocks,
    ChevronDown,
    ChevronRight,
    FolderClosed,
    FolderOpen,
    GalleryThumbnails,
    LoaderCircle,
    MessageCirclePlus,
    Paintbrush,
    PenLine,
    Plus,
    Settings2,
    SlidersHorizontal,
    Sprout,
  } from "lucide-svelte";
  import ConversationRenameDialog from "$lib/components/ConversationRenameDialog.svelte";
  import ArchivedChatsDialog from "$lib/components/ArchivedChatsDialog.svelte";
  import ProviderLogo from "$lib/components/ProviderLogo.svelte";
  import type { Conversation, Workspace, Worktree } from "$lib/types";

  let { workspaces, workspace, worktrees, conversations, selectedWorktreeId, selectedConversationId, runningConversationIds, activeView, onView, onProject, onAddProject, onConversation, onNewConversation, onRenameConversation, onArchiveConversation, onListArchivedConversations, onRestoreConversation, onSettings, onManageProject }: {
    workspaces:Workspace[];workspace?:Workspace;worktrees:Worktree[];conversations:Conversation[];selectedWorktreeId?:string;selectedConversationId?:string;runningConversationIds:string[];activeView:string;
    onView:(view:"chat"|"media"|"skills"|"arts")=>void;onProject:(workspace:Workspace)=>void|Promise<void>;onAddProject:()=>void;onConversation:(conversation:Conversation)=>void|Promise<void>;onNewConversation:(worktree?:Worktree)=>void|Promise<void>;onRenameConversation:(conversation:Conversation,title:string)=>void|Promise<void>;onArchiveConversation:(conversation:Conversation)=>void|Promise<void>;onListArchivedConversations:()=>Promise<Conversation[]>;onRestoreConversation:(conversation:Conversation)=>void|Promise<void>;onSettings:()=>void;onManageProject:()=>void;
  }=$props();
  let expandedProjects=$state<string[]>([]);
  let renameTarget=$state<Conversation>();
  let renaming=$state(false);
  let archiveOpen=$state(false);
  let archivedConversations=$state<Conversation[]>([]);
  let loadingArchive=$state(false);
  let restoringId=$state<string>();
  const primary=[{id:"chat",label:"New Chat",icon:MessageCirclePlus},{id:"media",label:"Media Gallery",icon:GalleryThumbnails},{id:"skills",label:"Skills",icon:Blocks},{id:"arts",label:"Arts",icon:Paintbrush}] as const;

  $effect(()=>{if(workspace&&!expandedProjects.includes(workspace.id))expandedProjects=[...expandedProjects,workspace.id];});
  function chatsFor(worktree:Worktree){return conversations.filter(item=>item.worktreeId===worktree.id);}
  async function rename(title:string){if(!renameTarget)return;renaming=true;try{await onRenameConversation(renameTarget,title);renameTarget=undefined;}finally{renaming=false;}}
  function newChat(){onView("chat");onNewConversation();}
  async function openArchive(){archiveOpen=true;loadingArchive=true;try{archivedConversations=await onListArchivedConversations();}finally{loadingArchive=false;}}
  async function restoreArchived(conversation:Conversation){restoringId=conversation.id;try{await onRestoreConversation(conversation);archivedConversations=archivedConversations.filter(item=>item.id!==conversation.id);if(!archivedConversations.length)archiveOpen=false;}finally{restoringId=undefined;}}
</script>

<aside class="sidebar">
  <div class="brand"><div class="pixel"><Sprout size={16}/></div><div><strong>Sprite Studio</strong><small>AI sprite creation</small></div></div>
  <nav class="primary" aria-label="Main navigation">
    {#each primary as item}<button class:active={activeView===item.id} onclick={()=>item.id==="chat"?newChat():onView(item.id)}><item.icon size={15}/><span>{item.label}</span>{#if item.id==="chat"}<kbd>⌘N</kbd>{/if}</button>{/each}
  </nav>
  <div class="divider"></div>
  <div class="projects-head"><span>Projects</span><button onclick={onAddProject} title="Add project" aria-label="Add project"><Plus size={14}/></button></div>
  <div class="projects">
    {#each workspaces as project}
      {@const isActive=project.id===workspace?.id}
      {@const expanded=expandedProjects.includes(project.id)}
      <section class="project" class:active={isActive}>
        <div class="project-row">
          <button class="project-main" onclick={()=>{if(isActive)expandedProjects=expanded?expandedProjects.filter(id=>id!==project.id):[...expandedProjects,project.id];else{expandedProjects=[...expandedProjects.filter(id=>id!==project.id),project.id];void onProject(project);}}}>
            {#if expanded&&isActive}<FolderOpen size={15}/>{:else}<FolderClosed size={15}/>{/if}<span>{project.name}</span>{#if expanded&&isActive}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}
          </button>
          {#if isActive}<button class="project-action" onclick={()=>onNewConversation()} title="New chat"><Plus size={12}/></button><button class="project-action" onclick={onManageProject} title="Project settings"><Settings2 size={13}/></button>{/if}
        </div>
        {#if expanded&&isActive}
          <div class="project-content">
            {#each worktrees as worktree}
              <div class="worktree">
                {#each chatsFor(worktree) as conversation}
                  <div class="chat" class:selected={conversation.id===selectedConversationId}>
                    <button class="chat-main" onclick={()=>onConversation(conversation)} title={`${conversation.title} · ${conversation.provider}`}><ProviderLogo providerId={conversation.provider} size={14}/><span>{conversation.title}</span>{#if runningConversationIds.includes(conversation.id)}<LoaderCircle class="running" size={12}/>{/if}</button>
                    <div class="chat-actions"><button onclick={()=>renameTarget=conversation} title="Rename"><PenLine size={11}/></button><button onclick={()=>onArchiveConversation(conversation)} title="Archive"><Archive size={11}/></button></div>
                  </div>
                {/each}
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    {#if !workspaces.length}<button class="empty-project" onclick={onAddProject}><Plus size={13}/>Add your first project</button>{/if}
  </div>
  <footer>{#if workspace}<button onclick={openArchive}><ArchiveRestore size={15}/><span>Archived chats</span></button>{/if}<button onclick={onSettings}><SlidersHorizontal size={15}/><span>Settings</span></button></footer>
</aside>

{#if renameTarget}<ConversationRenameDialog conversation={renameTarget} busy={renaming} onRename={rename} onClose={()=>renameTarget=undefined}/>{/if}
{#if archiveOpen}<ArchivedChatsDialog conversations={archivedConversations} {worktrees} loading={loadingArchive} {restoringId} onRestore={restoreArchived} onClose={()=>archiveOpen=false}/>{/if}

<style>
  .sidebar{width:304px;min-width:304px;height:100%;background:color-mix(in srgb,var(--sidebar) 84%,transparent);backdrop-filter:blur(20px) saturate(1.6);-webkit-backdrop-filter:blur(20px) saturate(1.6);border-right:1px solid var(--border);display:flex;flex-direction:column;color:var(--text);overflow:hidden}.brand{height:60px;min-height:60px;display:flex;align-items:center;gap:10px;padding:0 18px}.pixel{width:31px;height:31px;border:1px solid var(--border-strong);border-radius:8px;background:var(--surface);color:var(--accent);display:grid;place-items:center}.brand strong,.brand small{display:block}.brand strong{font-size:13px;font-weight:650}.brand small{font-size:9px;color:var(--faint);margin-top:2px}.primary{padding:2px 13px;display:flex;flex-direction:column;gap:1px}.primary button{height:30px;border:0;border-radius:7px;background:transparent;color:var(--muted);display:grid;grid-template-columns:19px minmax(0,1fr) auto;align-items:center;gap:8px;padding:0 10px;font:inherit;font-size:12px;text-align:left;cursor:pointer}.primary button:hover,.primary button.active{background:var(--surface-hover);color:var(--text)}.primary button :global(svg){color:var(--faint)}.primary button.active :global(svg),.primary button:hover :global(svg){color:var(--accent)}kbd{border:0;background:transparent;color:var(--faint);font:inherit;font-size:9px}.divider{height:1px;background:var(--border);margin:8px 18px}.projects-head{height:26px;display:flex;align-items:center;justify-content:space-between;padding:0 17px 4px;color:var(--faint);font-size:10px;letter-spacing:.05em}.projects-head button,.project-action,.chat-actions button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer;border-radius:5px}.projects-head button{width:24px;height:24px}.projects-head button:hover,.project-action:hover,.chat-actions button:hover{background:var(--surface-hover);color:var(--text)}.projects{min-height:0;flex:1;overflow:auto;padding:0 10px 12px}.project{margin-bottom:1px}.project-row{height:32px;display:flex;align-items:center}.project-main{height:32px;min-width:0;flex:1;border:0;border-radius:7px;background:transparent;color:var(--muted);display:grid;grid-template-columns:18px minmax(0,1fr) 14px;gap:7px;align-items:center;padding:0 9px;text-align:left;font:inherit;font-size:12.5px;cursor:pointer}.project.active>.project-row .project-main{color:var(--text);font-weight:590}.project-main:hover{background:var(--surface-hover)}.project-main span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.project-main :global(svg){color:var(--faint)}.project.active>.project-row .project-main :global(svg:first-child){color:var(--accent)}.project-action{width:25px;height:25px;margin-left:1px}.project-content{margin:0 0 3px 24px;border-left:1px solid var(--border);padding-left:6px}.worktree{margin:0}.chat{height:28px;display:flex;align-items:center;border-radius:6px;min-width:0}.chat:hover,.chat.selected{background:var(--surface-hover)}.chat-main{height:100%;min-width:0;flex:1;border:0;background:transparent;color:var(--muted);display:grid;grid-template-columns:14px minmax(0,1fr) 15px;gap:7px;align-items:center;padding:0 8px;text-align:left;font:inherit;font-size:12px;cursor:pointer}.chat.selected .chat-main{color:var(--text)}.chat-main>span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.chat-main :global(.running){color:var(--accent);animation:spin .8s linear infinite}.chat-actions{display:flex;opacity:0;padding-right:3px}.chat:hover .chat-actions{opacity:1}.chat-actions button{width:22px;height:22px}.empty-project{width:100%;height:34px;border:1px dashed var(--border-strong);border-radius:7px;background:transparent;color:var(--faint);display:flex;align-items:center;justify-content:center;gap:7px;font:inherit;font-size:11px;cursor:pointer}.empty-project:hover{border-color:var(--accent);color:var(--text)}footer{min-height:52px;border-top:1px solid var(--border);padding:7px 12px;display:flex;flex-direction:column}footer button{width:100%;height:36px;border:0;border-radius:7px;background:transparent;color:var(--muted);display:flex;align-items:center;gap:9px;padding:0 10px;font:inherit;font-size:12px;cursor:pointer}footer button:hover{background:var(--surface-hover);color:var(--text)}footer button :global(svg){color:var(--faint)}@keyframes spin{to{transform:rotate(360deg)}}
  @media(max-width:820px){.sidebar{width:250px;min-width:250px}}
</style>
