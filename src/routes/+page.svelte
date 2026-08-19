<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Archive, X, Trash2, FolderMinus, Pencil, RotateCcw } from "lucide-svelte";
  import ProjectSidebar from "$lib/components/ProjectSidebar.svelte";
  import ProjectDialog from "$lib/components/ProjectDialog.svelte";
  import MediaNavigation from "$lib/components/MediaNavigation.svelte";
  import SkillsLibrary from "$lib/components/SkillsLibrary.svelte";
  import ArtsLibrary from "$lib/components/ArtsLibrary.svelte";
  import NoProjectView from "$lib/components/NoProjectView.svelte";
  import ConversationView from "$lib/components/ConversationView.svelte";
  import AssetBrowser from "$lib/components/AssetBrowser.svelte";
  import AssetInspector from "$lib/components/AssetInspector.svelte";
  import SpriteViewer from "$lib/components/SpriteViewer.svelte";
  import ReferenceLibrary from "$lib/components/ReferenceLibrary.svelte";
  import AnimationEditor from "$lib/components/AnimationEditor.svelte";
  import RigEditor from "$lib/components/RigEditor.svelte";
  import TerrainStudio from "$lib/components/TerrainStudio.svelte";
  import SpriteSheetStudio from "$lib/components/SpriteSheetStudio.svelte";
  import VfxStudio from "$lib/components/VfxStudio.svelte";
  import TestRoom from "$lib/components/TestRoom.svelte";
  import PackLibrary from "$lib/components/PackLibrary.svelte";
  import MotionPromptDialog from "$lib/components/MotionPromptDialog.svelte";
  import SettingsModal from "$lib/components/SettingsModal.svelte";
  import WorktreeDialog from "$lib/components/WorktreeDialog.svelte";
  import LogoMark from "$lib/components/LogoMark.svelte";
  import { api } from "$lib/api";
  import { normalizeGenerationProfile, slashCommand } from "$lib/generation-profiles";
  import { buildSpriteGroups, type SpriteGroup } from "$lib/sprite-groups";
  import { parseConversationStyle, parseStylePreset, stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import { parseCustomArts, parseCustomSkills, type CustomArtStyle, type CustomSkill } from "$lib/library-types";
  import { errorMessage, type Animation, type AnimationPolishMode, type AnimationTemplate, type Asset, type AssetPack, type ChatGenerationProfile, type Conversation, type ImageProviderInput, type Message, type PackGenerationMetadata, type ProviderEvent, type ProviderRequestOptions, type ProviderStatus, type ReferenceCategory, type ReferenceImage, type Rig, type SidebarSnapshot, type SpriteGenerationMetadata, type SpriteSlashCommand, type TemplateApplication, type Workspace, type Worktree, type WorktreeKind } from "$lib/types";

  type ActiveChatRequest = { id:string; conversationId:string; workspaceId:string; worktreeId?:string; prompt:string; command?:SpriteSlashCommand; generation:ProviderRequestOptions["generation"]; knownPackIds:string[]; previousGenerationFingerprint?:string; startedAt:number };

  let loading = $state(true);
  let workspaces = $state<Workspace[]>([]);
  let workspace = $state<Workspace>();
  let worktrees = $state<Worktree[]>([]);
  let selectedWorktree = $state<Worktree>();
  let worktreeAssetIds = $state<string[]>([]);
  let conversations = $state<Conversation[]>([]);
  let sidebarConversations = $state<Conversation[]>([]);
  let selectedConversation = $state<Conversation>();
  let messages = $state<Message[]>([]);
  let assets = $state<Asset[]>([]);
  let packs = $state<AssetPack[]>([]);
  let packFilter = $state("");
  let selectedPackId = $state("");
  let selectedAsset = $state<Asset>();
  let viewedAsset = $state<Asset>();
  let motionAsset = $state<Asset>();
  let references = $state<ReferenceImage[]>([]);
  let activeReferenceIds = $state<string[]>([]);
  let focusedReferenceId = $state<string>();
  let animations = $state<Animation[]>([]);
  let animationTemplates = $state<AnimationTemplate[]>([]);
  let selectedAnimation = $state<Animation>();
  let rigs = $state<Rig[]>([]);
  let selectedRigId = $state<string>();
  let rigDraftAssetId = $state<string>();
  let providers = $state<ProviderStatus[]>([]);
  let defaultProvider = $state("codex");
  let activeTab = $state("chat");
  let settingsOpen = $state(false);
  let projectDialogOpen = $state(false);
  let workspaceMenu = $state(false);
  let worktreeDialog = $state(false);
  let creatingWorktree = $state(false);
  let renameValue = $state("");
  let backupBusy = $state(false);
  let runningRequests = $state<Record<string,ActiveChatRequest>>({});
  let activityByConversation = $state<Record<string,string[]>>({});
  let toast = $state<{message:string;kind:"error"|"notice"}>();
  let theme = $state("system");
  let workspaceStyle = $state<StylePresetId>("pixel-rpg");
  let conversationStyle = $state<ConversationStyleId>("inherit");
  let generationProfile = $state<ChatGenerationProfile>(normalizeGenerationProfile(null));
  let chatDraft = $state("");
  let customSkills = $state<CustomSkill[]>([]);
  let customArts = $state<CustomArtStyle[]>([]);
  let toastTimer: number | undefined;
  let conversationSelection = 0;
  let workspaceSelection = 0;

  const currentProvider = $derived(providers.find(provider => provider.id === (selectedConversation?.provider ?? "codex")));
  const imageProviders = $derived(providers.filter(provider=>provider.kind==="image"));
  const currentRequest = $derived(selectedConversation ? runningRequests[selectedConversation.id] : undefined);
  const currentActivity = $derived(selectedConversation ? activityByConversation[selectedConversation.id] ?? [] : []);
  const runningConversationIds = $derived(Object.keys(runningRequests));
  const effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle,customArts));
  const animationAssetIds = $derived(new Set(animations.flatMap(animation => animation.frames.map(frame => frame.assetId))));
  const visibleAssets = $derived(selectedWorktree?.kind === "general" || !selectedWorktree ? assets : assets.filter(asset => worktreeAssetIds.includes(asset.id) || animationAssetIds.has(asset.id)));
  const visiblePacks = $derived(packs.filter(pack => pack.files.some(file => visibleAssets.some(asset => asset.relativePath === file))));
  const spriteCount = $derived(buildSpriteGroups(visibleAssets, animations).length);
  const mediaTabs=["sprites","references","animate","rig","terrain","vfx","sheets","packs","play"];
  const activePrimary=$derived(activeTab==="chat"?"chat":mediaTabs.includes(activeTab)?"media":activeTab);
  function activeWorktreeId(){return selectedWorktree?.kind === "general" ? undefined : selectedWorktree?.id;}
  function generationOptions():ProviderRequestOptions["generation"]{return {quality:generationProfile.quality,width:generationProfile.width,height:generationProfile.height,frames:generationProfile.frames,fps:generationProfile.fps,frameMode:generationProfile.frameMode,minFrames:generationProfile.minFrames,maxFrames:generationProfile.maxFrames,allowInterpolation:generationProfile.allowInterpolation,allowAutoAdjust:generationProfile.allowAutoAdjust};}
  async function currentMotionPlan(){const user=messages.findLast(message=>message.role==="user");if(!user)return undefined;return api.planMotion(user.content,generationOptions()).catch(()=>undefined);}

  function notify(message:string,kind:"error"|"notice"="notice") {
    toast={message,kind}; if(toastTimer)window.clearTimeout(toastTimer);toastTimer=window.setTimeout(()=>toast=undefined,4200);
  }
  async function loadWorkspaces() {
    workspaces=(await api.loadSidebarState()).workspaces;
  }
  // The worktree chat list is a pure filter of the workspace snapshot the
  // sidebar already holds, so switching worktrees never re-queries chats.
  function chatsForWorktree(all:Conversation[],worktree?:Worktree){return !worktree||worktree.kind==="general"?all:all.filter(item=>item.worktreeId===worktree.id);}
  async function loadConversationFocus(conversation:Conversation|undefined, worktreeReferences:ReferenceImage[], activeIds:string[]){
    if(!conversation){focusedReferenceId=undefined;return activeIds;}
    const saved=await api.getSetting(`conversation-focus:${conversation.id}`);
    const candidate=typeof saved==="string"?saved:undefined;
    const focusId=candidate&&activeIds.includes(candidate)&&worktreeReferences.some(reference=>reference.id===candidate)?candidate:undefined;
    focusedReferenceId=focusId;
    if(focusId){await api.setSetting(`conversation-focus:${conversation.id}`,focusId);await api.setConversationReference(conversation.id,focusId,true,2);}
    return focusId?[focusId,...activeIds.filter(id=>id!==focusId)]:activeIds;
  }
  async function loadWorkspace(selected:Workspace,preloaded?:SidebarSnapshot) {
    const selection=++workspaceSelection;
    let loadStage="opening project";
    try {
      // Switch the shell immediately. Expensive filesystem scans and secondary
      // database reads hydrate the selected project without replacing the whole
      // window with a blocking loading screen.
      workspace=selected;
      worktrees=[];sidebarConversations=[];conversations=[];selectedWorktree=undefined;selectedConversation=undefined;messages=[];
      const openedWorkspace=await api.touchWorkspace(selected.id);
      if(selection!==workspaceSelection)return;
      workspace=openedWorkspace;
      loadStage="saving the active project";
      await api.setSetting("activeWorkspaceId",workspace.id);
      loadStage="loading project assets and tools";
      // One snapshot carries projects, worktrees, and chats from a single
      // database read; the per-worktree chat list is derived locally from it.
      const [scan,snapshot]=await Promise.all([
        Promise.all([api.listAssets(workspace.id),api.listAssetPacks(workspace.id)]),
        Promise.resolve(preloaded??api.loadSidebarState(workspace.id)),
      ]);
      if(selection!==workspaceSelection)return;
      [assets,packs]=scan;
      const activeWorkspace=workspace;
      workspaces=preloaded&&activeWorkspace?[activeWorkspace,...snapshot.workspaces.filter(item=>item.id!==activeWorkspace.id)]:snapshot.workspaces;
      worktrees=snapshot.worktrees;
      sidebarConversations=snapshot.conversations;
      loadStage="restoring the active worktree";
      const savedWorktree=await api.getSetting(`active-worktree:${workspace.id}`);
      selectedWorktree=worktrees.find(item=>item.id===savedWorktree)??worktrees[0];
      loadStage="loading chats and animations";
      conversations=chatsForWorktree(snapshot.conversations,selectedWorktree);
      const worktreeId=activeWorktreeId();
      [worktreeAssetIds,animations,rigs,animationTemplates,workspaceStyle,references]=await Promise.all([
        selectedWorktree?api.listWorktreeAssetIds(selectedWorktree.id):Promise.resolve([]),
        api.listAnimations(workspace.id,worktreeId),
        api.listRigs(workspace.id,worktreeId),
        api.listAnimationTemplates(workspace.id),
        api.getSetting(`workspace-style:${workspace.id}`).then(value=>parseStylePreset(value,customArts)),
        selectedWorktree?api.listReferenceImages(selectedWorktree.id):Promise.resolve([]),
      ]);
      if(selection!==workspaceSelection)return;
      await reconcileGenerationManifest().catch(error=>notify(`Project opened, but the latest generation could not be reconciled: ${errorMessage(error)}`,"error"));
      selectedAnimation=animations[0];selectedAsset=undefined;activeTab="chat";
      selectedConversation=conversations[0];
      if(selectedConversation){
        const [nextMessages,nextReferenceIds,nextStyle,nextProfile]=await Promise.all([
          api.listMessages(selectedConversation.id),
          api.listConversationReferenceIds(selectedConversation.id),
          api.getSetting(`conversation-style:${selectedConversation.id}`).then(value=>parseConversationStyle(value,customArts)),
          loadGenerationProfile(selectedConversation),
        ]);
        if(selection!==workspaceSelection)return;
        messages=nextMessages;
        activeReferenceIds=nextReferenceIds.filter(id=>references.some(reference=>reference.id===id));
        conversationStyle=nextStyle;
        generationProfile=nextProfile;
      }else{
        messages=[];activeReferenceIds=[];conversationStyle="inherit";generationProfile=normalizeGenerationProfile(null);
      }
      activeReferenceIds=await loadConversationFocus(selectedConversation,references,activeReferenceIds);
      await hydrateLatestGeneration().catch(error=>notify(`Project opened, but the latest chat preview could not be restored: ${errorMessage(error)}`,"error"));
      // A full scan decodes and hashes every image. Refresh it only after the
      // cached project has rendered, and ignore the result if the user switched
      // projects while it was running.
      void api.scanAssets(workspace.id).then(nextAssets=>{if(selection===workspaceSelection)assets=nextAssets;}).catch(error=>notify(`Project opened, but its asset refresh could not finish: ${errorMessage(error)}`,"error"));
    } catch(error) { if(selection===workspaceSelection){workspace=undefined;notify(`Could not finish ${loadStage}: ${errorMessage(error)}`,"error");} }
  }
  async function initialLoad() {
    try {
      const [saved,nextSkills,nextArts,nextDefaultProvider,nextTheme]=await Promise.all([
        api.getSetting("activeWorkspaceId"),
        api.getSetting("custom-skills").then(parseCustomSkills),
        api.getSetting("custom-arts").then(parseCustomArts),
        api.getSetting("default-agent-provider").then(value=>String(value??"codex")),
        api.getSetting("theme").then(value=>String(value??"system")),
      ]);
      customSkills=nextSkills;customArts=nextArts;defaultProvider=nextDefaultProvider;applyTheme(nextTheme);
      const activeId=typeof saved==="string"?saved:undefined;
      // Projects and chats arrive in one snapshot so the shell renders after a
      // single database round trip instead of three sequential queries.
      const snapshot=await api.loadSidebarState(activeId);
      workspaces=snapshot.workspaces;
      // First paint and sidebar expansion must never wait for external CLIs.
      // Provider detection updates independently once the local shell is ready.
      loading=false;
      void api.detectProviders().then(value=>providers=value).catch(error=>notify(`Provider detection could not finish: ${errorMessage(error)}`,"error"));
      const recent=activeId?workspaces.find(item=>item.id===activeId):undefined;
      if(recent){await loadWorkspace(recent,snapshot);return;}
    } catch(error){notify(errorMessage(error),"error");}
    loading=false;
  }
  async function acceptWorkspace(value:Workspace){await loadWorkspace(value);}
  async function goHome(){workspace=undefined;worktrees=[];selectedWorktree=undefined;worktreeAssetIds=[];references=[];activeReferenceIds=[];focusedReferenceId=undefined;animationTemplates=[];packs=[];packFilter="";conversations=[];sidebarConversations=[];selectedConversation=undefined;messages=[];generationProfile=normalizeGenerationProfile(null);rigs=[];selectedRigId=undefined;rigDraftAssetId=undefined;await api.setSetting("activeWorkspaceId",null);await loadWorkspaces();}
  async function chooseWorktree(value:Worktree){selectedWorktree=value;selectedAsset=undefined;packFilter="";activeTab="chat";if(!workspace)return;[worktreeAssetIds,animations,rigs,references]=await Promise.all([api.listWorktreeAssetIds(value.id),api.listAnimations(workspace.id,value.id),api.listRigs(workspace.id,value.id),api.listReferenceImages(value.id)]);conversations=chatsForWorktree(sidebarConversations,value);selectedAnimation=animations[0];selectedRigId=rigs[0]?.id;selectedConversation=conversations[0];if(selectedConversation){await chooseConversation(selectedConversation);}else{messages=[];activeReferenceIds=[];focusedReferenceId=undefined;conversationStyle="inherit";generationProfile=normalizeGenerationProfile(null);}await api.setSetting(`active-worktree:${workspace.id}`,value.id);}
  async function createWorktree(name:string,kind:WorktreeKind,description?:string){if(!workspace)return;creatingWorktree=true;try{const created=await api.createWorktree(workspace.id,name,kind,description);worktrees=await api.listWorktrees(workspace.id);await chooseWorktree(created);if(!conversations.length)await newConversation();worktreeDialog=false;notify(`${created.name} worktree created — its first chat is ready`);}catch(error){notify(errorMessage(error),"error");}finally{creatingWorktree=false;}}
  async function newConversation(worktree=selectedWorktree){if(!workspace){projectDialogOpen=true;return;}if(!worktree)return;try{if(selectedWorktree?.id!==worktree.id)await chooseWorktree(worktree);selectedAsset=undefined;viewedAsset=undefined;activeReferenceIds=[];focusedReferenceId=undefined;const conversation=await api.createConversation(workspace.id,worktree.id,undefined,defaultProvider);conversations=[conversation,...conversations];sidebarConversations=[conversation,...sidebarConversations];await chooseConversation(conversation);}catch(error){notify(errorMessage(error),"error");}}
  async function chooseConversation(conversation:Conversation){
    const selection=++conversationSelection;
    selectedConversation=conversation;
    selectedAsset=undefined;
    viewedAsset=undefined;
    packFilter="";
    activeTab="chat";
    if(!workspace)return;
    const conversationWorktree=conversation.worktreeId?worktrees.find(item=>item.id===conversation.worktreeId):undefined;
    if(conversationWorktree&&conversationWorktree.id!==selectedWorktree?.id){
      selectedWorktree=conversationWorktree;
      selectedAsset=undefined;
      const [nextAssetIds,nextAnimations,nextRigs,nextReferences]=await Promise.all([
        api.listWorktreeAssetIds(conversationWorktree.id),
        api.listAnimations(workspace.id,conversationWorktree.id),
        api.listRigs(workspace.id,conversationWorktree.id),
        api.listReferenceImages(conversationWorktree.id),
      ]);
      if(selection!==conversationSelection)return;
      worktreeAssetIds=nextAssetIds;
      animations=nextAnimations;
      rigs=nextRigs;
      references=nextReferences;
      conversations=chatsForWorktree(sidebarConversations,conversationWorktree);
      selectedAnimation=nextAnimations[0];
      selectedRigId=nextRigs[0]?.id;
      await api.setSetting(`active-worktree:${workspace.id}`,conversationWorktree.id);
    }
    const [nextMessages,nextStyle,nextProfile,nextReferenceIds]=await Promise.all([
      api.listMessages(conversation.id),
      api.getSetting(`conversation-style:${conversation.id}`).then(value=>parseConversationStyle(value,customArts)),
      loadGenerationProfile(conversation),
      api.listConversationReferenceIds(conversation.id),
    ]);
    if(selection!==conversationSelection)return;
    messages=nextMessages;
    conversationStyle=nextStyle;
    generationProfile=nextProfile;
    activeReferenceIds=nextReferenceIds.filter(id=>references.some(reference=>reference.id===id));
    activeReferenceIds=await loadConversationFocus(conversation,references,activeReferenceIds);
  }
  async function renameChat(conversation:Conversation,title:string){try{await api.renameConversation(conversation.id,title);const renamed={...conversation,title};sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);conversations=conversations.map(item=>item.id===conversation.id?renamed:item);if(selectedConversation?.id===conversation.id)selectedConversation={...selectedConversation,title};notify("Chat renamed");}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function archiveChat(conversation:Conversation){if(runningRequests[conversation.id]){notify("Stop this chat’s generation before archiving it","error");return;}try{await api.archiveConversation(conversation.id);sidebarConversations=sidebarConversations.filter(item=>item.id!==conversation.id);conversations=conversations.filter(item=>item.id!==conversation.id);if(selectedConversation?.id===conversation.id){const next=conversations[0];if(next)await chooseConversation(next);else await newConversation(selectedWorktree);}notify("Chat archived");}catch(error){notify(errorMessage(error),"error");}}
  async function listArchivedChats(){if(!workspace)return [];try{return await api.listArchivedConversations(workspace.id);}catch(error){notify(errorMessage(error),"error");return [];}}
  async function restoreArchivedChat(conversation:Conversation){try{const restored=await api.restoreConversation(conversation.id);sidebarConversations=[restored,...sidebarConversations.filter(item=>item.id!==restored.id)];conversations=chatsForWorktree(sidebarConversations,selectedWorktree);await chooseConversation(restored);notify("Chat restored");}catch(error){notify(errorMessage(error),"error");throw error;}}
  function providerFor(conversation:Conversation){return providers.find(provider=>provider.id===conversation.provider);}
  async function loadGenerationProfile(conversation:Conversation){return normalizeGenerationProfile(await api.getSetting(`conversation-generation:${conversation.id}`),providerFor(conversation)?.modes??[]);}
  function composerReferenceCategory():ReferenceCategory{return activeTab==="vfx"?"vfx":"other";}
  function referenceSlots(){const maximum=currentProvider?.capabilities.maximumReferenceImages??0;return maximum>0?Math.max(0,maximum-activeReferenceIds.length):0;}
  async function activateImportedReferences(created:ReferenceImage[]){
    if(!selectedConversation)return;
    for(const reference of created)await api.setConversationReference(selectedConversation.id,reference.id,true);
    references=[...created,...references];activeReferenceIds=[...activeReferenceIds,...created.map(reference=>reference.id)];
    notify(`Attached ${created.length} reference image${created.length===1?"":"s"} to this chat`);
  }
  async function attachReferencePaths(paths:string[]){
    if(!selectedWorktree||!selectedConversation){notify("Select a worktree chat before adding a reference","error");return;}
    const slots=referenceSlots();if(!slots){notify("This provider cannot accept more reference images","error");return;}
    try{const created=[];for(const path of paths.slice(0,slots))created.push(await api.importReferenceImage(selectedWorktree.id,path,composerReferenceCategory()));await activateImportedReferences(created);if(paths.length>slots)notify(`Attached ${slots}; this provider allows ${currentProvider?.capabilities.maximumReferenceImages} references`,"error");}
    catch(error){notify(errorMessage(error),"error");}
  }
  async function attachReferenceFiles(files:File[]){
    if(!selectedWorktree||!selectedConversation){notify("Select a worktree chat before pasting a reference","error");return;}
    const slots=referenceSlots();if(!slots){notify("This provider cannot accept more reference images","error");return;}
    try{const created=[];for(const file of files.slice(0,slots)){if(file.size>25*1024*1024)throw new Error(`${file.name||"Pasted image"} is larger than 25 MB`);const bytes=Array.from(new Uint8Array(await file.arrayBuffer()));created.push(await api.importReferenceBytes(selectedWorktree.id,file.name||`pasted-reference-${Date.now()}.png`,bytes,composerReferenceCategory()));}await activateImportedReferences(created);if(files.length>slots)notify(`Attached ${slots}; this provider allows ${currentProvider?.capabilities.maximumReferenceImages} references`,"error");}
    catch(error){notify(errorMessage(error),"error");}
  }
  async function focusConversationReference(id?:string){if(!selectedConversation)return;try{const previous=focusedReferenceId;if(previous&&previous!==id)await api.setConversationReference(selectedConversation.id,previous,true,1);await Promise.all([api.setSetting(`conversation-focus:${selectedConversation.id}`,id??null),api.setSetting(`conversation-master:${selectedConversation.id}`,null)]);if(id){await api.setConversationReference(selectedConversation.id,id,true,2);activeReferenceIds=[id,...activeReferenceIds.filter(value=>value!==id)];}focusedReferenceId=id;notify(id?"Focused reference updated":"Reference focus cleared");}catch(error){notify(errorMessage(error),"error");}}
  async function removeConversationReference(id:string){if(!selectedConversation)return;try{if(id===focusedReferenceId)await focusConversationReference(undefined);await api.setConversationReference(selectedConversation.id,id,false);activeReferenceIds=activeReferenceIds.filter(value=>value!==id);}catch(error){notify(errorMessage(error),"error");}}
  async function send(prompt:string){
    if(!selectedConversation||!workspace)return;
    const conversation=selectedConversation;const worktree=selectedWorktree;const style=effectiveStyle;const profile=generationProfile;const referenceIds=[...activeReferenceIds];
    if(runningRequests[conversation.id]){notify("This chat is already generating — open another chat to work in parallel","error");return;}
    activityByConversation={...activityByConversation,[conversation.id]:[]};
    try{
      const focused=references.find(reference=>reference.id===focusedReferenceId);
      const enabledSkillContext=customSkills.filter(skill=>skill.enabled).map(skill=>`USER SKILL — ${skill.name}: ${skill.instructions}`).join("\n");
      const context=[worktree?`Active project section: ${worktree.name}. ${worktree.description??""}`:"",focused?`FOCUSED CHAT REFERENCE: ${focused.path} (content hash ${focused.contentHash}). Treat this as the primary visual subject for this turn. The user may change or clear focus later; do not describe it as permanently locked.`:"",selectedAsset?`Context asset: ${selectedAsset.relativePath}`:"",`Selected art direction: ${style.name}. ${style.prompt}.`,enabledSkillContext].filter(Boolean).join("\n");
      const generation={quality:profile.quality,width:profile.width,height:profile.height,frames:profile.frames,fps:profile.fps,frameMode:profile.frameMode,minFrames:profile.minFrames,maxFrames:profile.maxFrames,allowInterpolation:profile.allowInterpolation,allowAutoAdjust:profile.allowAutoAdjust};
      const command=slashCommand(prompt)??(/\b(?:asset\s+|sprite\s+|art\s+)?packs?\b/i.test(prompt)?"pack":undefined);
      const options:ProviderRequestOptions={model:profile.model||undefined,reasoningEffort:profile.reasoningEffort||undefined,command,generation,referenceIds,imageProviderId:profile.imageProviderId};
      if(command==="pack")activityByConversation={...activityByConversation,[conversation.id]:["AI is planning one coordinated asset pack","Items will share one style and remain separate static sprites"]};
      else if(/\b(?:terrain|tileset|tilemap|ground tiles?)\b/i.test(prompt))activityByConversation={...activityByConversation,[conversation.id]:["AI is planning one complete terrain atlas","Output will be one PNG containing compatible fills, edges, corners, strips, and transitions"]};
      else if(generation.frameMode==="auto")activityByConversation={...activityByConversation,[conversation.id]:["AI is inspecting the reference and will recommend a frame count",`Allowed range: ${generation.minFrames}–${generation.maxFrames} frames`]};
      const previousGenerationFingerprint=await api.getGenerationFingerprint(workspace.id).catch(()=>null);
      const startedAt=Date.now();
      const requestId=await api.startProviderMessage(conversation.id,prompt,context,options);
      runningRequests={...runningRequests,[conversation.id]:{id:requestId,conversationId:conversation.id,workspaceId:workspace.id,worktreeId:worktree?.id,prompt,command,generation,knownPackIds:packs.map(pack=>pack.id),previousGenerationFingerprint:previousGenerationFingerprint??undefined,startedAt}};
      if(selectedConversation?.id===conversation.id)messages=await api.listMessages(conversation.id);
      if(conversation.title==="New conversation"){
        const title=prompt.trim().replace(/^\/[a-z]+\s*/i,"").replace(/\s+/g," ").slice(0,42)||"New conversation";
        await api.renameConversation(conversation.id,title);const renamed={...conversation,title};
        conversations=conversations.map(item=>item.id===conversation.id?renamed:item);sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);if(selectedConversation?.id===conversation.id)selectedConversation=renamed;
      }
    }catch(error){const next={...runningRequests};delete next[conversation.id];runningRequests=next;notify(errorMessage(error),"error");}
  }
  async function cancel(){if(!selectedConversation)return;const request=runningRequests[selectedConversation.id];if(!request)return;try{await api.cancelProviderRequest(request.id);}catch(error){notify(errorMessage(error),"error");}}
  async function finalizeChatGeneration(request:ActiveChatRequest,response:string){
    if(!workspace||workspace.id!==request.workspaceId)return;
    const manifest=await api.getGenerationManifest(request.workspaceId).catch(()=>null);
    const manifestFingerprint=manifest?await api.getGenerationFingerprint(request.workspaceId).catch(()=>null):null;
    // Never infer output from the assistant's prose. Re-renders commonly keep
    // the same asset IDs and may not mention file paths at all. The renderer's
    // manifest is the only authoritative handoff, but it must be newer than
    // this request so a stale workspace manifest cannot open unrelated art.
    const manifestTime=manifest?Date.parse(manifest.generatedAt):Number.NaN;
    const freshManifest=Boolean(manifest&&manifestFingerprint&&manifestFingerprint!==request.previousGenerationFingerprint&&Number.isFinite(manifestTime)&&manifestTime>=request.startedAt-2_000);
    const generatedAssets=freshManifest?await api.scanGenerationAssets(request.workspaceId):[];
    const assetMap=new Map(assets.map(asset=>[asset.id,asset]));
    for(const asset of generatedAssets)assetMap.set(asset.id,asset);
    const nextAssets=[...assetMap.values()].sort((a,b)=>a.category.localeCompare(b.category)||a.name.localeCompare(b.name));
    const nextPacks=await api.listAssetPacks(request.workspaceId).catch(()=>packs);const knownPacks=new Set(request.knownPackIds);const createdPacks=nextPacks.filter(pack=>!knownPacks.has(pack.id));
    const responseText=response.toLowerCase();
    const generatedPack=request.command==="pack"?(createdPacks.find(pack=>responseText.includes(pack.name.toLowerCase())||responseText.includes(pack.id.toLowerCase())||pack.files.some(file=>responseText.includes(file.toLowerCase())))??(createdPacks.length===1?createdPacks[0]:undefined)):undefined;
    const manifestAssets=freshManifest?manifest!.files.map(path=>nextAssets.find(asset=>asset.relativePath===path)).filter((asset):asset is Asset=>Boolean(asset)):[];
    // An explicit animation request must never be presented as complete when
    // the renderer only handed back a static fallback.
    const rejectedStaticAnimation=request.command==="animate"&&manifestAssets.length>0&&manifestAssets.length<2;
    const acceptedManifestAssets=rejectedStaticAnimation?[]:manifestAssets;
    const manifestFps=manifest&&Number.isFinite(manifest.fps)&&manifest.fps>0?manifest.fps:request.generation.fps;
    const packAssets=generatedPack?.files.map(path=>nextAssets.find(asset=>asset.relativePath===path)).filter((asset):asset is Asset=>Boolean(asset))??[];
    // Sprite Studio's component handoff is manifest-only. A tool response can
    // say “completed” after editing prose or an unrelated file; accepting
    // guessed paths here is what previously left the UI in chat with no asset
    // to show. Packs retain their own explicit pack handoff.
    const related=request.command==="pack"?packAssets:acceptedManifestAssets;
    assets=nextAssets;
    if(request.worktreeId&&related.length)await Promise.all(related.map(asset=>api.linkAssetToWorktree(request.worktreeId!,asset.id)));
    let animationId:string|undefined;
    const ordered=acceptedManifestAssets.length?[...related]:[...related].sort((a,b)=>a.relativePath.localeCompare(b.relativePath,undefined,{numeric:true}));
    if(request.command!=="pack"&&ordered.length>1){
      const existing=await api.listAnimations(request.workspaceId,request.worktreeId);const match=existing.find(animation=>animation.frames.length===ordered.length&&ordered.every((asset,index)=>animation.frames[index]?.assetId===asset.id));
      if(match)animationId=match.id;else{const actualGeneration={...request.generation,frameMode:"fixed" as const,frames:ordered.length,minFrames:ordered.length,maxFrames:ordered.length,fps:manifestFps};const motionPlan=await api.planMotion(request.prompt,actualGeneration).catch(()=>undefined);const baseName=ordered[0].name.replace(/[_-]?\d+$/i,"");const createdAnimation=await api.saveAnimation({workspaceId:request.workspaceId,worktreeId:request.worktreeId,name:baseName,fps:manifestFps,looping:true,frames:ordered.map(asset=>({assetId:asset.id})),motionPlan});animationId=createdAnimation.id;void api.queueQualityAnalysis(createdAnimation.id).catch(()=>undefined);}
    }
    if(request.command!=="pack"&&ordered.length){
      const requestMessages=await api.listMessages(request.conversationId);const assistant=requestMessages.findLast(message=>message.role==="assistant"&&message.status==="completed");
      if(assistant){const generation:SpriteGenerationMetadata={kind:"sprite-generation",name:ordered[0].name.replace(/[_-]?\d+$/i,""),category:ordered[0].category,fps:ordered.length>1?manifestFps:1,assetIds:ordered.map(asset=>asset.id),animationId};await api.updateMessageMetadata(assistant.id,{...assistant.metadata,generation});}
    }
    if(generatedPack){const requestMessages=await api.listMessages(request.conversationId);const assistant=requestMessages.findLast(message=>message.role==="assistant"&&message.status==="completed");if(assistant){const packGeneration:PackGenerationMetadata={kind:"pack-generation",packId:generatedPack.id};await api.updateMessageMetadata(assistant.id,{...assistant.metadata,packGeneration});}}
    packs=nextPacks;
    rigs=await api.listRigs(request.workspaceId,request.worktreeId).catch(()=>rigs);
    if(selectedWorktree?.id===request.worktreeId){
      worktreeAssetIds=request.worktreeId?await api.listWorktreeAssetIds(request.worktreeId):[];
      animations=await api.listAnimations(request.workspaceId,request.worktreeId);
      // A completed generation should hand the user directly to the thing that
      // changed. Leaving the chat visible makes an update look like prose-only
      // output, and choosing animations[0] can open an unrelated older asset.
      if(request.conversationId===selectedConversation?.id&&animationId){
        const generatedAnimation=animations.find(animation=>animation.id===animationId);
        if(generatedAnimation){
          selectedAnimation=generatedAnimation;
          selectedAsset=undefined;
          viewedAsset=undefined;
          activeTab="animate";
        }
      }else if(request.conversationId===selectedConversation?.id&&ordered[0]){
        selectedAsset=ordered[0];
        viewedAsset=undefined;
        activeTab="sprites";
        // Single-sprite generations are the natural rig masters: profile the
        // silhouette immediately so the next step is one click away.
        if(ordered.length===1){
          void api.analyzeRigFit(ordered[0].id).then(report=>{
            const top=report.detections[0];
            if(top)notify(`Rig check: ${top.morphology} ${Math.round(top.confidence*100)}%${report.warnings.length?" (needs a cleaner master)":""} — open the Rig tab to animate it with points`);
          }).catch(()=>undefined);
        }
      }else if(request.conversationId===selectedConversation?.id&&request.command!=="pack"){
        notify(rejectedStaticAnimation?"Animation needs at least two fresh frames. The static fallback was kept as an asset but was not accepted as a completed animation.":"Generation completed without a fresh valid sprite manifest, so it was not accepted as a component update.","error");
      }
    }
  }
  async function refreshCreatedAssets() {
    if (!workspace) return;
    const known = new Set(assets.map(asset => asset.id));
    const nextAssets = await api.scanAssets(workspace.id);
    const created = nextAssets.filter(asset => !known.has(asset.id));
    assets = nextAssets;
    animations = await api.listAnimations(workspace.id,activeWorktreeId());
    if (!created.length) return;
    if(selectedWorktree){await Promise.all(created.map(asset=>api.linkAssetToWorktree(selectedWorktree!.id,asset.id)));worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id);}
    const manifest = await api.getGenerationManifest(workspace.id).catch(() => null);
    const ordered = manifest?.files.map(path => created.find(asset => asset.relativePath === path)).filter((asset): asset is Asset => Boolean(asset)) ?? created;
    if (manifest?.kind !== "pack" && ordered.length > 1) {
      const animation = await api.saveAnimation({workspaceId:workspace.id,worktreeId:activeWorktreeId(),name:manifest?.name ?? ordered[0].name.replace(/[_-]?\d+$/, ""),fps:manifest?.fps ?? 8,looping:true,frames:ordered.map(asset => ({assetId:asset.id})),motionPlan:await currentMotionPlan()});
      animations = await api.listAnimations(workspace.id,activeWorktreeId());
      selectedAnimation = animation;
      void api.queueQualityAnalysis(animation.id).catch(()=>undefined);
      selectedAsset = undefined;
      notify(`Created ${ordered.length} sprite frames — preview added to chat`);
      await attachGenerationCard(ordered,manifest?.name ?? animation.name,manifest?.category ?? ordered[0].category,manifest?.fps ?? animation.fps,animation.id);
    } else {
      selectedAsset = ordered[0];
      notify(`Created ${ordered[0].name}.png — preview added to chat`);
      await attachGenerationCard(ordered,manifest?.name ?? ordered[0].name,manifest?.category ?? ordered[0].category,manifest?.fps ?? 1);
    }
  }
  async function attachGenerationCard(cardAssets:Asset[],name:string,category:string,fps:number,animationId?:string) {
    if(!selectedConversation)return;
    const assistant=messages.findLast(message=>message.role==="assistant"&&message.status==="completed");
    if(!assistant)return;
    const generation:SpriteGenerationMetadata={kind:"sprite-generation",name,category,fps,assetIds:cardAssets.map(asset=>asset.id),animationId};
    await api.updateMessageMetadata(assistant.id,{...assistant.metadata,generation});
    messages=await api.listMessages(selectedConversation.id);
  }
  async function reconcileGenerationManifest() {
    if(!workspace)return;
    const manifest=await api.getGenerationManifest(workspace.id).catch(()=>null);
    if(!manifest||manifest.kind==="pack"||manifest.files.length<2)return;
    const ordered=manifest.files.map(path=>assets.find(asset=>asset.relativePath===path));
    if(ordered.some(asset=>!asset))return;
    const cardAssets=ordered as Asset[];
    if(selectedWorktree){await Promise.all(cardAssets.map(asset=>api.linkAssetToWorktree(selectedWorktree!.id,asset.id)));worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id);}
    const existing=animations.find(animation=>animation.frames.length===cardAssets.length&&cardAssets.every((asset,index)=>animation.frames[index]?.assetId===asset.id));
    if(existing){selectedAnimation=existing;return;}
    selectedAnimation=await api.saveAnimation({workspaceId:workspace.id,worktreeId:activeWorktreeId(),name:manifest.name,fps:manifest.fps,looping:true,frames:cardAssets.map(asset=>({assetId:asset.id})),motionPlan:await currentMotionPlan()});
    animations=await api.listAnimations(workspace.id,activeWorktreeId());
  }
  async function hydrateLatestGeneration() {
    if(!workspace||!selectedConversation||!messages.length)return;
    const assistant=messages.findLast(message=>message.role==="assistant"&&message.status==="completed");
    if(!assistant||assistant.metadata.generation)return;
    const manifest=await api.getGenerationManifest(workspace.id).catch(()=>null);
    if(!manifest||manifest.kind==="pack")return;
    const cardAssets=manifest.files.map(path=>assets.find(asset=>asset.relativePath===path)).filter((asset):asset is Asset=>Boolean(asset));
    if(!cardAssets.length||!manifest.files.some(path=>assistant.content.includes(path)||assistant.content.toLowerCase().includes(manifest.name.toLowerCase())))return;
    const animation=animations.find(item=>cardAssets.every(asset=>item.frames.some(frame=>frame.assetId===asset.id)));
    await attachGenerationCard(cardAssets,manifest.name,manifest.category,manifest.fps,animation?.id);
  }
  function editAssetFromChat(asset:Asset){selectedAsset=asset;viewedAsset=asset;activeTab="sprites";}
  function editAnimationFromChat(animation:Animation){selectedAnimation=animation;selectedAsset=undefined;activeTab="animate";}
  function prepareTemplateInChat(application:TemplateApplication){selectedAsset=application.targetAsset;chatDraft=application.prompt;activeTab="chat";}
  async function generateVfxFromStudio(prompt:string){if(!selectedConversation)throw new Error("Create a VFX chat before generating an effect");if(!["ready","detected"].includes(currentProvider?.status??""))throw new Error("Open Settings to install or sign in to this chat's provider");const conversationId=selectedConversation.id;activeTab="chat";await send(prompt);if(!runningRequests[conversationId])throw new Error("VFX generation could not start");}
  async function refreshVfxAssets(){if(!workspace||!selectedWorktree)return;assets=await api.scanAssets(workspace.id);worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id);animations=await api.listAnimations(workspace.id,activeWorktreeId());selectedAnimation=animations[0];}
  function openVfxSheet(animation:Animation){selectedAnimation=animation;activeTab="sheets";}
  async function exportAnimationFromChat(animation:Animation){
    try {
      const result=await api.exportAnimation(animation.id);
      notify(`Exported ${result.width}×${result.height} spritesheet`);
      await revealItemInDir(result.pngPath);
    } catch(error){notify(errorMessage(error),"error");}
  }
  async function exportAssetFromChat(asset:Asset){
    try {
      const result=await api.exportAsset(asset.id);
      notify(`Exported ${result.width}×${result.height} sprite`);
      await revealItemInDir(result.pngPath);
    } catch(error){notify(errorMessage(error),"error");}
  }
  function selectTab(value:string){activeTab=value;}
  function selectPrimary(value:"chat"|"media"|"skills"|"arts"){
    if(value==="chat"){activeTab="chat";return;}
    if(value==="media"){activeTab=mediaTabs.includes(activeTab)?activeTab:"sprites";return;}
    activeTab=value;
  }
  function selectAsset(asset:Asset){selectedAsset=asset;}
  function viewPack(pack:AssetPack){selectedPackId=pack.id;selectedAsset=undefined;viewedAsset=undefined;activeTab="packs";}
  function openPackFromChat(pack:AssetPack){viewPack(pack);}
  function openPackAsset(asset:Asset){selectedAsset=asset;viewedAsset=asset;}
  async function openSpriteGroup(group:SpriteGroup){
    if(group.frames.length===1){selectedAsset=group.preview;viewedAsset=group.preview;return;}
    try{
      let animation=group.animationId?animations.find(item=>item.id===group.animationId):undefined;
      if(!animation&&workspace){animation=await api.saveAnimation({workspaceId:workspace.id,worktreeId:activeWorktreeId(),name:group.name,fps:group.fps??generationProfile.fps,looping:true,frames:group.frames.map(asset=>({assetId:asset.id}))});animations=await api.listAnimations(workspace.id,activeWorktreeId());}
      if(animation){selectedAnimation=animation;selectedAsset=undefined;viewedAsset=undefined;activeTab="animate";}
    }catch(error){notify(errorMessage(error),"error");}
  }
  function animateViewedAsset(asset:Asset){motionAsset=asset;}
  function rigAsset(asset:Asset){rigDraftAssetId=asset.id;selectedRigId=undefined;viewedAsset=undefined;motionAsset=undefined;activeTab="rig";notify(`${asset.name} is staged in the Rig editor — place points or ask the AI`);}
  async function rigRendered(animation:Animation,assetIds:string[]){
    if(!workspace)return;
    assets=await api.scanAssets(workspace.id);
    animations=await api.listAnimations(workspace.id,activeWorktreeId());
    if(selectedWorktree&&assetIds.length){await Promise.all(assetIds.map(id=>api.linkAssetToWorktree(selectedWorktree!.id,id)));worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id);}
    selectedAnimation=animation;selectedAsset=undefined;viewedAsset=undefined;activeTab="animate";
    void api.queueQualityAnalysis(animation.id).catch(()=>undefined);
    notify(`Rendered ${animation.frames.length} rig frames — quality analysis started`);
  }
  async function prepareMotionInChat(asset:Asset,motion:string,polishMode:AnimationPolishMode){
    if(!selectedConversation)await newConversation();
    if(!selectedConversation){notify("Create a chat before preparing an animation","error");return;}
    selectedAsset=asset;viewedAsset=undefined;motionAsset=undefined;activeTab="chat";
    const frameBudget=generationProfile.frameMode==="fixed"?`${generationProfile.frames} frames`:`between ${generationProfile.minFrames} and ${generationProfile.maxFrames} frames, choosing the smallest mechanically complete count`;
    const finishInstruction=polishMode==="ai-polish"
      ? "Polish mode: AI polish. Build and validate the deterministic rig first, render all rough frames, then repair only small joint, seam, and outline defects while preserving each rough pose exactly."
      :polishMode==="full-redraw"
        ? "Polish mode: Full redraw (experimental). Build and validate the deterministic rig first, then use each rough rig frame as the exact pose, timing, scale, and canvas authority for its redraw."
        :"Polish mode: Rig only. Do not use ImageGen for animation frames. Build, validate, and render the complete animation deterministically from the exact source pixels.";
    chatDraft=`/animate Use ${asset.relativePath} as the exact source master. Motion: ${motion}. Plan a repeatable ${frameBudget} loop at ${generationProfile.fps} FPS. ${finishInstruction} Preserve the source anatomy, markings, palette, proportions, facing direction, pivot, and ground line. Keep near/far limb identity and layer order stable through crossings, preview at least three cycles, save the rig and playback manifest, and run native quality analysis before reporting success.`;
    notify(`${asset.name} is attached — review the motion prompt and send when ready`);
  }
  async function prepareRigPolishInChat(animation:Animation,assetIds:string[]){
    if(!workspace){notify("Open a project before polishing","error");return;}
    if(!selectedConversation)await newConversation();
    if(!selectedConversation){notify("Create a chat before polishing","error");return;}
    if(selectedWorktree&&assetIds.length){await Promise.all(assetIds.map(id=>api.linkAssetToWorktree(selectedWorktree!.id,id)));worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id);}
    assets=await api.scanAssets(workspace.id);
    const frameAssets=animation.frames.map(frame=>assets.find(asset=>asset.id===frame.assetId)).filter((asset):asset is Asset=>Boolean(asset));
    if(!frameAssets.length){notify("Render the rig before polishing it","error");return;}
    const paths=frameAssets.map(asset=>asset.relativePath);
    selectedAsset=frameAssets[0];viewedAsset=undefined;activeTab="chat";
    // The rig's frames are pose-canonical: deterministic render first, AI detail second.
    chatDraft=`Enhance the rig-rendered animation "${animation.name}" with AI polish. These frames were rendered deterministically from a joint rig, so their poses, limb layering, and contact timing are canonical: ${paths.join(", ")}. For each rig frame, generate one high-quality transparent AI frame using the corresponding rig frame as the exact pose reference. Keep the identical pose, timing, and canvas placement; keep the NEAR limb always occluding the FAR limb and the FAR limb about 20–30% darker — never swap leg identity or shading roles. Fix only rendering detail and quality, never retime, reorder, or reinterpret the poses. Write the polished frames in playback order under assets/${frameAssets[0].category}/, update the generation manifest, and run quality analysis before reporting success.`;
    notify("Rig frames staged — review the polish prompt and send when ready");
  }
  function updateAsset(asset:Asset){selectedAsset=asset;assets=assets.map(item=>item.id===asset.id?asset:item);}
  async function assetDeleted(){selectedAsset=undefined;if(workspace)assets=await api.scanAssets(workspace.id);}
  function applyTheme(value:string){theme=value;document.documentElement.dataset.theme=value;}
  async function changeTheme(value:string){applyTheme(value);try{await api.setSetting("theme",value);}catch(error){notify(errorMessage(error),"error");}}
  async function changeWorkspaceStyle(value:StylePresetId){if(!workspace){notify("Open a project before choosing its art direction","error");return;}workspaceStyle=value;try{await api.setSetting(`workspace-style:${workspace.id}`,value);notify(`${stylePreset(value,customArts).name} is now the project art direction`);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationStyle(value:ConversationStyleId){if(!selectedConversation)return;conversationStyle=value;try{await api.setSetting(`conversation-style:${selectedConversation.id}`,value);notify(value==="inherit"?"Chat now follows the project art direction":`${stylePreset(value,customArts).name} applied to this chat`);}catch(error){notify(errorMessage(error),"error");}}
  async function saveCustomSkills(value:CustomSkill[]){customSkills=value;try{await api.setSetting("custom-skills",value);notify("Skills library saved");}catch(error){notify(errorMessage(error),"error");}}
  async function saveCustomArts(value:CustomArtStyle[]){customArts=value;try{await api.setSetting("custom-arts",value);notify("Arts library saved");}catch(error){notify(errorMessage(error),"error");}}
  async function changeGenerationProfile(value:ChatGenerationProfile){if(!selectedConversation)return;generationProfile=normalizeGenerationProfile(value,currentProvider?.modes??[]);try{await api.setSetting(`conversation-generation:${selectedConversation.id}`,generationProfile);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationProvider(providerId:string){
    if(!selectedConversation)return;
    if(runningRequests[selectedConversation.id]){notify("Stop this chat’s generation before switching providers","error");return;}
    if(selectedConversation.provider===providerId)return;
    try{
      const changed=await api.switchConversationProvider(selectedConversation.id,providerId);
      const nextProvider=providers.find(provider=>provider.id===providerId);
      const nextProfile=normalizeGenerationProfile({...generationProfile,model:"",reasoningEffort:"",imageProviderId:providerId==="codex"?"imagegen":"provider-native"},nextProvider?.modes??[]);
      selectedConversation=changed;
      conversations=conversations.map(item=>item.id===changed.id?changed:item);
      sidebarConversations=sidebarConversations.map(item=>item.id===changed.id?changed:item);
      generationProfile=nextProfile;
      await api.setSetting(`conversation-generation:${changed.id}`,nextProfile);
      notify(`${nextProvider?.name??"Provider"} selected. This starts a new provider session; chat history remains visible.`);
    }catch(error){notify(errorMessage(error),"error");}
  }
  async function refreshProviders(){try{providers=await api.detectProviders();notify("Provider detection refreshed");}catch(error){notify(errorMessage(error),"error");}}
  async function changeDefaultProvider(provider:string){defaultProvider=provider;try{await api.setSetting("default-agent-provider",provider);notify(`${providers.find(item=>item.id===provider)?.name??provider} will be used for new chats`);}catch(error){notify(errorMessage(error),"error");}}
  async function saveImageProvider(input:ImageProviderInput){try{await api.saveImageProvider(input);providers=await api.detectProviders();notify(`${input.name} saved`);}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function deleteImageProvider(id:string){try{await api.deleteImageProvider(id);providers=await api.detectProviders();notify("Custom provider removed");}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function testImageProvider(input:ImageProviderInput){try{return await api.testImageProvider(input);}catch(error){const message=errorMessage(error);notify(message,"error");throw new Error(message);}}
  async function renameWorkspace(){if(!workspace||!renameValue.trim())return;try{await api.renameWorkspace(workspace.id,renameValue);workspace={...workspace,name:renameValue.trim()};workspaceMenu=false;await loadWorkspaces();notify("Project renamed");}catch(error){notify(errorMessage(error),"error");}}
  async function removeWorkspace(deleteFiles:boolean){if(!workspace)return;const question=deleteFiles?`Permanently delete ${workspace.name} and every file in its folder?`:`Remove ${workspace.name} from Sprite Studio? Its files will stay on disk.`;if(!window.confirm(question))return;try{if(deleteFiles)await api.deleteWorkspace(workspace.id);else await api.removeWorkspace(workspace.id);workspaceMenu=false;await goHome();notify(deleteFiles?"Project files deleted":"Project removed from the app");}catch(error){notify(errorMessage(error),"error");}}
  async function createBackup(){
    if(!workspace)return;
    const destination=await open({directory:true,multiple:false,title:"Choose backup destination"});
    if(typeof destination!=="string")return;
    backupBusy=true;
    try{const backup=await api.createProjectBackup(workspace.id,destination);notify(`Backup created with ${backup.fileCount} files`);await revealItemInDir(backup.backupPath);}
    catch(error){notify(errorMessage(error),"error");}
    finally{backupBusy=false;}
  }
  async function restoreBackup(){
    if(!workspace)return;
    const backupPath=await open({directory:true,multiple:false,title:"Choose a Sprite Studio backup"});
    if(typeof backupPath!=="string")return;
    if(!window.confirm(`Restore ${workspace.name} from this backup? Current files and project data will be replaced after Sprite Studio creates a safety backup.`))return;
    backupBusy=true;
    try{const restored=await api.restoreProjectBackup(workspace.id,backupPath);workspaceMenu=false;await loadWorkspace(restored);notify("Project restored; a safety backup of the previous state was kept beside the project");}
    catch(error){notify(errorMessage(error),"error");}
    finally{backupBusy=false;}
  }

  onMount(()=>{
    initialLoad();
    const shortcuts=(event:KeyboardEvent)=>{
      if(!(event.metaKey||event.ctrlKey)||event.altKey||event.shiftKey)return;
      if(event.key.toLowerCase()==="n"){event.preventDefault();void newConversation();return;}
      const order=["chat","sprites","references","animate","rig","terrain","vfx","sheets","packs"];
      const tab=order[Number(event.key)-1];
      if(tab){event.preventDefault();activeTab=tab;}
    };
    window.addEventListener("keydown",shortcuts);
    const unlistenPromise=listen<ProviderEvent>("provider-event",async({payload})=>{
      const selected=payload.conversationId===selectedConversation?.id;
      if(payload.eventType==="activity"||payload.eventType==="started"){const current=activityByConversation[payload.conversationId]??[];activityByConversation={...activityByConversation,[payload.conversationId]:[...current,payload.content]};}
      if(payload.eventType==="content"&&selected){
        const index=messages.findLastIndex(message=>message.role==="assistant"&&message.status==="running");
        if(index>=0){const next=[...messages];next[index]={...next[index],content:next[index].content+payload.content};messages=next;}
      }
      if(["completed","failed","cancelled"].includes(payload.eventType)){
        const request=runningRequests[payload.conversationId];
        if(payload.eventType==="completed"&&request){
          const current=activityByConversation[payload.conversationId]??[];
          activityByConversation={...activityByConversation,[payload.conversationId]:[...current,"Registering the generated sprite"]};
          try{await finalizeChatGeneration(request,payload.content);}
          catch(error){notify(`The provider finished, but the sprite preview could not be refreshed: ${errorMessage(error)}`,"error");}
        }
        const next={...runningRequests};if(next[payload.conversationId]?.id===payload.requestId)delete next[payload.conversationId];runningRequests=next;
        if(payload.conversationId===selectedConversation?.id)messages=await api.listMessages(payload.conversationId);
        if(payload.eventType==="failed")notify(payload.content,"error");
      }
    });
    return()=>{unlistenPromise.then(unlisten=>unlisten());window.removeEventListener("keydown",shortcuts);if(toastTimer)clearTimeout(toastTimer);};
  });
</script>

<svelte:head><title>{workspace ? `${workspace.name} — Sprite Studio` : "Sprite Studio"}</title><meta name="description" content="Local-first AI game asset development environment"/></svelte:head>

{#if loading}
  <div class="boot"><div class="boot-mark"><LogoMark size={27}/></div><span></span><p>Opening Sprite Studio</p></div>
{:else}
  <main class="studio">
    <ProjectSidebar {workspaces} {workspace} {worktrees} conversations={sidebarConversations} selectedWorktreeId={selectedWorktree?.id} selectedConversationId={selectedConversation?.id} {runningConversationIds} activeView={activePrimary} onView={selectPrimary} onProject={loadWorkspace} onAddProject={()=>projectDialogOpen=true} onConversation={chooseConversation} onNewConversation={newConversation} onRenameConversation={renameChat} onArchiveConversation={archiveChat} onListArchivedConversations={listArchivedChats} onRestoreConversation={restoreArchivedChat} onSettings={()=>settingsOpen=true} onManageProject={()=>{if(workspace){renameValue=workspace.name;workspaceMenu=true;}}}/>
    <section class="main-pane">
      <div class="tab-stack">
        {#if activeTab==="skills"}<SkillsLibrary skills={customSkills} onChange={saveCustomSkills}/>
        {:else if activeTab==="arts"}<ArtsLibrary arts={customArts} selected={workspaceStyle} onChange={saveCustomArts} onSelect={changeWorkspaceStyle}/>
        {:else if !workspace}<NoProjectView onAdd={()=>projectDialogOpen=true}/>
        {:else if activeTab==="chat"}
          {#key selectedConversation?.id}<ConversationView conversation={selectedConversation} {messages} provider={currentProvider} availableProviders={providers} {imageProviders} customStyles={customArts} runningRequestId={currentRequest?.id} activity={currentActivity} {selectedAsset} {assets} {animations} packs={visiblePacks} {references} {activeReferenceIds} {focusedReferenceId} draftPrompt={chatDraft} onDraftConsumed={()=>chatDraft=""} workspacePath={workspace.path} {workspaceStyle} {conversationStyle} {generationProfile} onSend={send} onCancel={cancel} onClearAsset={()=>selectedAsset=undefined} onEditAsset={editAssetFromChat} onEditAnimation={editAnimationFromChat} onViewPack={openPackFromChat} onExportAsset={exportAssetFromChat} onExportAnimation={exportAnimationFromChat} onConversationStyle={changeConversationStyle} onGenerationProfile={changeGenerationProfile} onProviderSwitch={changeConversationProvider} onAttachReferencePaths={attachReferencePaths} onAttachReferenceFiles={attachReferenceFiles} onFocusReference={focusConversationReference} onRemoveReference={removeConversationReference} onLinkError={(message)=>notify(message,"error")}/>{/key}
        {:else}
          <div class="media-view"><MediaNavigation active={activeTab} counts={{sprites:spriteCount,references:references.length,animate:animations.length,rigs:rigs.length,packs:visiblePacks.length}} onSelect={selectTab}/><div class="media-content">
            {#if activeTab==="sprites"}<AssetBrowser workspaceId={workspace.id} worktreeId={selectedWorktree?.id} assets={visibleAssets} {animations} packs={visiblePacks} packId={packFilter} selectedAssetId={selectedAsset?.id} onAssets={(value)=>assets=value} onSelect={selectAsset} onOpen={openSpriteGroup} onPack={(value)=>packFilter=value} onLinked={async()=>{if(selectedWorktree)worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id)}} onError={(message)=>notify(message,"error")}/>
            {:else if activeTab==="references"&&selectedWorktree}<ReferenceLibrary worktreeId={selectedWorktree.id} conversationId={selectedConversation?.id} {references} activeIds={activeReferenceIds} maximumActive={currentProvider?.capabilities.maximumReferenceImages ?? 0} onReferences={(value)=>references=value} onActiveIds={(value)=>activeReferenceIds=value} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="animate"}<AnimationEditor workspaceId={workspace.id} worktreeId={activeWorktreeId()} assets={visibleAssets} {animations} templates={animationTemplates} {selectedAnimation} active onAnimations={(value)=>animations=value} onTemplates={(value)=>animationTemplates=value} onSelected={(value)=>selectedAnimation=value} onTemplateApplication={prepareTemplateInChat} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="rig"}<RigEditor workspaceId={workspace.id} worktreeId={activeWorktreeId()} assets={assets} {rigs} {providers} {selectedRigId} initialAssetId={rigDraftAssetId} onRigs={(value)=>rigs=value} onSelected={(id)=>{selectedRigId=id;rigDraftAssetId=undefined;}} onRendered={rigRendered} onPolish={prepareRigPolishInChat} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="terrain"&&selectedWorktree}<TerrainStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} assets={visibleAssets} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="vfx"&&selectedWorktree}<VfxStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} {animations} assets={visibleAssets} active onCreated={refreshVfxAssets} onOpenSheets={openVfxSheet} onGenerate={generateVfxFromStudio} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="sheets"}<SpriteSheetStudio workspaceId={workspace.id} worktreeId={activeWorktreeId()} {animations} assets={visibleAssets} active onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="packs"}<PackLibrary packs={visiblePacks} assets={visibleAssets} {selectedPackId} onView={viewPack} onBack={()=>selectedPackId=""} onOpen={openPackAsset}/>
            {:else if activeTab==="play"}<TestRoom {animations} assets={visibleAssets} {selectedAnimation} active/>{/if}
          </div></div>
        {/if}
      </div>
    </section>
    {#if selectedAsset && activeTab==="sprites"}<AssetInspector asset={selectedAsset} {animations} onClose={()=>selectedAsset=undefined} onChanged={updateAsset} onDeleted={assetDeleted} onError={(message)=>notify(message,"error")}/>{/if}
  </main>
{/if}

{#if projectDialogOpen}<ProjectDialog onCreated={acceptWorkspace} onClose={()=>projectDialogOpen=false} onError={(message)=>notify(message,"error")}/>{/if}
{#if settingsOpen}<SettingsModal {providers} {defaultProvider} {theme} {workspaceStyle} customStyles={customArts} onDefaultProvider={changeDefaultProvider} onTheme={changeTheme} onWorkspaceStyle={changeWorkspaceStyle} onRefresh={refreshProviders} onSaveImageProvider={saveImageProvider} onDeleteImageProvider={deleteImageProvider} onTestImageProvider={testImageProvider} onClose={()=>settingsOpen=false}/>{/if}
{#if worktreeDialog}<WorktreeDialog busy={creatingWorktree} onCreate={createWorktree} onClose={()=>worktreeDialog=false}/>{/if}
{#if viewedAsset}<SpriteViewer asset={viewedAsset} onAnimate={animateViewedAsset} onDownload={exportAssetFromChat} onClose={()=>viewedAsset=undefined}/>{/if}
{#if motionAsset}<MotionPromptDialog asset={motionAsset} onContinue={(motion,polishMode)=>prepareMotionInChat(motionAsset!,motion,polishMode)} onRig={()=>rigAsset(motionAsset!)} onClose={()=>motionAsset=undefined}/>{/if}
{#if workspaceMenu && workspace}<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&(workspaceMenu=false)}><div class="workspace-dialog"><header><div><p>PROJECT</p><h2>Manage {workspace.name}</h2></div><button onclick={()=>workspaceMenu=false}><X size={15}/></button></header><label>Name<div><input bind:value={renameValue}/><button onclick={renameWorkspace}><Pencil size={12}/>Rename</button></div></label><p class="path">{workspace.path}</p><div class="backup-actions"><button onclick={createBackup} disabled={backupBusy}><Archive size={13}/>{backupBusy?"Working…":"Create backup…"}</button><button onclick={restoreBackup} disabled={backupBusy}><RotateCcw size={13}/>Restore backup…</button></div><div class="danger-actions"><button onclick={()=>removeWorkspace(false)}><FolderMinus size={13}/>Remove from app</button><button onclick={()=>removeWorkspace(true)}><Trash2 size={13}/>Delete files…</button></div></div></div>{/if}
{#if toast}<div class="toast" class:error={toast.kind==="error"}><span></span><p>{toast.message}</p><button onclick={()=>toast=undefined}><X size={12}/></button></div>{/if}

<style>
  :global(*){box-sizing:border-box}:global(html){font-family:"Avenir Next","SF Pro Display",-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-size:15px;letter-spacing:-.01em;background:var(--bg);color:var(--text);font-synthesis:none;text-rendering:optimizeLegibility;-webkit-font-smoothing:antialiased}:global(body){margin:0;overflow:hidden}:global(button),:global(input),:global(textarea),:global(select){font-family:inherit}:global(:focus-visible){outline:2px solid var(--accent)!important;outline-offset:2px}
  :global(:root),:global([data-theme="dark"]){--bg:#151515;--sidebar:#292929;--surface:#242424;--surface-hover:#353535;--composer:#2a2a2a;--selected:#383838;--preview:#111;--checker:#1d1d1d;--text:#ececec;--muted:#b8b8b8;--faint:#858585;--border:#3b3b3b;--border-strong:#555;--accent:#b7d34b;--accent-dim:#b7d34b20}
  :global([data-theme="light"]){--bg:#f6f8fb;--sidebar:#eef1f5;--surface:#fff;--surface-hover:#e6ebf2;--composer:#fff;--selected:#e4eaf3;--preview:#eef2f6;--checker:#dce2ea;--text:#18202b;--muted:#586575;--faint:#8490a0;--border:#d7dee8;--border-strong:#bcc7d6;--accent:#3b78d8;--accent-dim:#3b78d81f}
  @media(prefers-color-scheme:light){:global([data-theme="system"]){--bg:#f6f8fb;--sidebar:#eef1f5;--surface:#fff;--surface-hover:#e6ebf2;--composer:#fff;--selected:#e4eaf3;--preview:#eef2f6;--checker:#dce2ea;--text:#18202b;--muted:#586575;--faint:#8490a0;--border:#d7dee8;--border-strong:#bcc7d6;--accent:#3b78d8;--accent-dim:#3b78d81f}}
  .studio{width:100vw;height:100vh;display:flex;background:var(--bg);color:var(--text)}.main-pane{flex:1;min-width:0;height:100%;position:relative}.tab-stack{position:absolute;inset:0;overflow:hidden}.media-view{height:100%;display:grid;grid-template-rows:58px minmax(0,1fr)}.media-content{min-height:0;position:relative}.media-content>:global(*){max-width:100%}
  .boot{width:100vw;height:100vh;background:#090a0a;display:flex;flex-direction:column;align-items:center;justify-content:center;color:#707174}.boot-mark{--logo-pixel:#f4f4f5;width:46px;height:46px;border:1px solid #3b3c3d;border-radius:10px;display:grid;place-items:center;color:#f5a524;background:#171818}.boot>span{width:90px;height:1px;background:#282929;position:relative;margin-top:22px;overflow:hidden}.boot>span:after{content:"";position:absolute;width:35px;height:1px;background:#8b5cf6;animation:load 1.2s ease-in-out infinite}.boot p{font-size:12px;letter-spacing:.06em;margin-top:13px}@keyframes load{from{left:-35px}to{left:90px}}
  .toast{position:fixed;right:16px;bottom:16px;z-index:70;min-width:260px;max-width:420px;min-height:40px;background:var(--surface);border:1px solid var(--border-strong);box-shadow:0 12px 34px #0006;border-radius:6px;display:grid;grid-template-columns:7px minmax(0,1fr) 24px;align-items:center;padding:0 7px}.toast>span{width:6px;height:6px;border-radius:50%;background:#5cad7b}.toast.error>span{background:#d16f69}.toast p{font-size:12px;line-height:1.4;margin:10px 8px;color:var(--text)}.toast button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}
  .backdrop{position:fixed;inset:0;background:#0009;display:grid;place-items:center;z-index:45}.workspace-dialog{width:min(430px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:8px;box-shadow:0 24px 70px #0009;padding:20px}.workspace-dialog header{display:flex;align-items:flex-start;justify-content:space-between}.workspace-dialog header p{font-size:10px;letter-spacing:.14em;color:var(--faint);font-weight:700;margin:0 0 7px}.workspace-dialog h2{font-size:16px;margin:0}.workspace-dialog header button{border:0;background:transparent;color:var(--faint);width:26px;height:26px;display:grid;place-items:center;cursor:pointer}.workspace-dialog label{font-size:11px;color:var(--muted);display:block;margin-top:24px}.workspace-dialog label>div{display:flex;gap:6px;margin-top:7px}.workspace-dialog input{height:31px;min-width:0;flex:1;background:var(--bg);border:1px solid var(--border-strong);border-radius:4px;color:var(--text);padding:0 8px;font-size:12px;outline:0}.workspace-dialog label button,.backup-actions button,.danger-actions button{height:31px;border:1px solid var(--border);background:var(--bg);color:var(--muted);border-radius:4px;display:flex;align-items:center;gap:6px;padding:0 9px;font:inherit;font-size:11px;cursor:pointer}.workspace-dialog .path{font-size:10px;color:var(--faint);overflow-wrap:anywhere;margin:10px 0 16px}.backup-actions{display:flex;gap:7px;padding-bottom:15px}.backup-actions button{flex:1;justify-content:center}.backup-actions button:disabled{opacity:.5;cursor:not-allowed}.danger-actions{border-top:1px solid var(--border);padding-top:15px;display:flex;justify-content:space-between}.danger-actions button:last-child{color:#cf7772}
</style>
