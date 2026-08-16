<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Archive, Menu, MoreHorizontal, X, Trash2, FolderMinus, Pencil, RotateCcw } from "lucide-svelte";
  import WelcomeScreen from "$lib/components/WelcomeScreen.svelte";
  import WorkspaceSidebar from "$lib/components/WorkspaceSidebar.svelte";
  import WorkspaceTabs from "$lib/components/WorkspaceTabs.svelte";
  import ConversationView from "$lib/components/ConversationView.svelte";
  import AssetBrowser from "$lib/components/AssetBrowser.svelte";
  import AssetInspector from "$lib/components/AssetInspector.svelte";
  import SpriteViewer from "$lib/components/SpriteViewer.svelte";
  import ReferenceLibrary from "$lib/components/ReferenceLibrary.svelte";
  import AnimationEditor from "$lib/components/AnimationEditor.svelte";
  import SpriteSheetStudio from "$lib/components/SpriteSheetStudio.svelte";
  import TerrainStudio from "$lib/components/TerrainStudio.svelte";
  import VfxStudio from "$lib/components/VfxStudio.svelte";
  import TestRoom from "$lib/components/TestRoom.svelte";
  import PackLibrary from "$lib/components/PackLibrary.svelte";
  import MotionPromptDialog from "$lib/components/MotionPromptDialog.svelte";
  import SettingsModal from "$lib/components/SettingsModal.svelte";
  import WorktreeDialog from "$lib/components/WorktreeDialog.svelte";
  import LogoMark from "$lib/components/LogoMark.svelte";
  import { api } from "$lib/api";
  import { normalizeGenerationProfile, slashCommand } from "$lib/generation-profiles";
  import { CREATABLE_WORKTREE_KINDS, type CreatableWorktreeKind } from "$lib/worktree-kinds";
  import { buildSpriteGroups, type SpriteGroup } from "$lib/sprite-groups";
  import { parseConversationStyle, parseStylePreset, stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import { errorMessage, type Animation, type AnimationPolishMode, type AnimationTemplate, type Asset, type AssetPack, type ChatGenerationProfile, type Conversation, type Message, type PackGenerationMetadata, type ProviderEvent, type ProviderRequestOptions, type ProviderStatus, type ReferenceCategory, type ReferenceImage, type SpriteGenerationMetadata, type SpriteSlashCommand, type TemplateApplication, type Workspace, type Worktree, type WorktreeKind } from "$lib/types";

  type ActiveChatRequest = { id:string; conversationId:string; workspaceId:string; worktreeId?:string; prompt:string; command?:SpriteSlashCommand; generation:ProviderRequestOptions["generation"]; knownAssetIds:string[]; knownPackIds:string[] };

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
  let providers = $state<ProviderStatus[]>([]);
  let activeTab = $state("chat");
  let sidebarOpen = $state(true);
  let settingsOpen = $state(false);
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
  let toastTimer: number | undefined;
  let conversationSelection = 0;

  const currentProvider = $derived(providers.find(provider => provider.id === (selectedConversation?.provider ?? "codex")));
  const currentRequest = $derived(selectedConversation ? runningRequests[selectedConversation.id] : undefined);
  const currentActivity = $derived(selectedConversation ? activityByConversation[selectedConversation.id] ?? [] : []);
  const runningConversationIds = $derived(Object.keys(runningRequests));
  const effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle));
  const animationAssetIds = $derived(new Set(animations.flatMap(animation => animation.frames.map(frame => frame.assetId))));
  const visibleAssets = $derived(selectedWorktree?.kind === "general" || !selectedWorktree ? assets : assets.filter(asset => worktreeAssetIds.includes(asset.id) || animationAssetIds.has(asset.id)));
  const visiblePacks = $derived(packs.filter(pack => pack.files.some(file => visibleAssets.some(asset => asset.relativePath === file))));
  const spriteCount = $derived(buildSpriteGroups(visibleAssets, animations).length);
  function activeWorktreeId(){return selectedWorktree?.kind === "general" ? undefined : selectedWorktree?.id;}
  function generationOptions():ProviderRequestOptions["generation"]{return {quality:generationProfile.quality,width:generationProfile.width,height:generationProfile.height,frames:generationProfile.frames,fps:generationProfile.fps,frameMode:generationProfile.frameMode,minFrames:generationProfile.minFrames,maxFrames:generationProfile.maxFrames,allowInterpolation:generationProfile.allowInterpolation,allowAutoAdjust:generationProfile.allowAutoAdjust};}
  async function currentMotionPlan(){const user=messages.findLast(message=>message.role==="user");if(!user)return undefined;return api.planMotion(user.content,generationOptions()).catch(()=>undefined);}

  function notify(message:string,kind:"error"|"notice"="notice") {
    toast={message,kind}; if(toastTimer)window.clearTimeout(toastTimer);toastTimer=window.setTimeout(()=>toast=undefined,4200);
  }
  async function loadWorkspaces() {
    workspaces=await api.listWorkspaces();
  }
  async function loadConversationFocus(conversation:Conversation|undefined, worktreeReferences:ReferenceImage[], activeIds:string[]){
    if(!conversation){focusedReferenceId=undefined;return activeIds;}
    const saved=await api.getSetting(`conversation-focus:${conversation.id}`);
    const candidate=typeof saved==="string"?saved:undefined;
    const focusId=candidate&&activeIds.includes(candidate)&&worktreeReferences.some(reference=>reference.id===candidate)?candidate:undefined;
    focusedReferenceId=focusId;
    if(focusId){await api.setSetting(`conversation-focus:${conversation.id}`,focusId);await api.setConversationReference(conversation.id,focusId,true,2);}
    return focusId?[focusId,...activeIds.filter(id=>id!==focusId)]:activeIds;
  }
  async function loadWorkspace(selected:Workspace) {
    loading=true;
    let loadStage="opening workspace";
    try {
      workspace=await api.touchWorkspace(selected.id);
      loadStage="saving the active workspace";
      await api.setSetting("activeWorkspaceId",workspace.id);
      loadStage="loading project assets and tools";
      [assets,packs,providers,worktrees]=await Promise.all([api.scanAssets(workspace.id),api.listAssetPacks(workspace.id),api.detectProviders(),api.listWorktrees(workspace.id)]);
      loadStage="restoring the active worktree";
      const savedWorktree=await api.getSetting(`active-worktree:${workspace.id}`);
      selectedWorktree=worktrees.find(item=>item.id===savedWorktree)??worktrees[0];
      loadStage="loading chats and animations";
      [conversations,sidebarConversations]=await Promise.all([api.listConversations(workspace.id,selectedWorktree?.id),api.listConversations(workspace.id)]);
      worktreeAssetIds=selectedWorktree?await api.listWorktreeAssetIds(selectedWorktree.id):[];
      animations=await api.listAnimations(workspace.id,activeWorktreeId());
      animationTemplates=await api.listAnimationTemplates(workspace.id);
      await reconcileGenerationManifest().catch(error=>notify(`Workspace opened, but the latest generation could not be reconciled: ${errorMessage(error)}`,"error"));
      workspaceStyle=parseStylePreset(await api.getSetting(`workspace-style:${workspace.id}`));
      selectedAnimation=animations[0];selectedAsset=undefined;activeTab="chat";
      selectedConversation=conversations[0];messages=selectedConversation?await api.listMessages(selectedConversation.id):[];
      references=selectedWorktree?await api.listReferenceImages(selectedWorktree.id):[];
      activeReferenceIds=selectedConversation?(await api.listConversationReferenceIds(selectedConversation.id)).filter(id=>references.some(reference=>reference.id===id)):[];
      activeReferenceIds=await loadConversationFocus(selectedConversation,references,activeReferenceIds);
      if(selectedConversation){[conversationStyle,generationProfile]=await Promise.all([api.getSetting(`conversation-style:${selectedConversation.id}`).then(parseConversationStyle),loadGenerationProfile(selectedConversation)]);}else{conversationStyle="inherit";generationProfile=normalizeGenerationProfile(null);}
      await hydrateLatestGeneration().catch(error=>notify(`Workspace opened, but the latest chat preview could not be restored: ${errorMessage(error)}`,"error"));
    } catch(error) { workspace=undefined;notify(`Could not finish ${loadStage}: ${errorMessage(error)}`,"error"); }
    finally {loading=false;}
  }
  async function initialLoad() {
    try {
      await loadWorkspaces();providers=await api.detectProviders();
      const saved=await api.getSetting("activeWorkspaceId");
      theme=String((await api.getSetting("theme"))??"system");applyTheme(theme);
      const recent=workspaces.find(item=>item.id===saved);
      if(recent){await loadWorkspace(recent);return;}
    } catch(error){notify(errorMessage(error),"error");}
    loading=false;
  }
  async function acceptWorkspace(value:Workspace){await loadWorkspaces();await loadWorkspace(value);}
  async function goHome(){workspace=undefined;worktrees=[];selectedWorktree=undefined;worktreeAssetIds=[];references=[];activeReferenceIds=[];focusedReferenceId=undefined;animationTemplates=[];packs=[];packFilter="";conversations=[];sidebarConversations=[];selectedConversation=undefined;messages=[];generationProfile=normalizeGenerationProfile(null);await api.setSetting("activeWorkspaceId",null);await loadWorkspaces();}
  async function chooseWorktree(value:Worktree){selectedWorktree=value;selectedAsset=undefined;packFilter="";activeTab="chat";if(!workspace)return;[worktreeAssetIds,animations,references,conversations]=await Promise.all([api.listWorktreeAssetIds(value.id),api.listAnimations(workspace.id,value.id),api.listReferenceImages(value.id),api.listConversations(workspace.id,value.id)]);selectedAnimation=animations[0];selectedConversation=conversations[0];if(selectedConversation){await chooseConversation(selectedConversation);}else{messages=[];activeReferenceIds=[];focusedReferenceId=undefined;conversationStyle="inherit";generationProfile=normalizeGenerationProfile(null);}await api.setSetting(`active-worktree:${workspace.id}`,value.id);}
  async function createWorktree(name:string,kind:WorktreeKind,description?:string){if(!workspace)return;creatingWorktree=true;try{const created=await api.createWorktree(workspace.id,name,kind,description);worktrees=await api.listWorktrees(workspace.id);await chooseWorktree(created);if(!conversations.length)await newConversation();worktreeDialog=false;notify(`${created.name} worktree created — its first chat is ready`);}catch(error){notify(errorMessage(error),"error");}finally{creatingWorktree=false;}}
  async function updateExistingWorktree(worktree:Worktree,name:string,kind:CreatableWorktreeKind,description?:string){
    try{
      const updated=await api.updateWorktree(worktree.id,name,kind,description);
      worktrees=worktrees.map(item=>item.id===updated.id?updated:item);
      if(selectedWorktree?.id===updated.id)selectedWorktree=updated;
      const label=CREATABLE_WORKTREE_KINDS.find(option=>option.value===updated.kind)?.label??updated.kind;
      notify(`${updated.name} now routes as ${label}`);
    }catch(error){notify(errorMessage(error),"error");throw error;}
  }
  async function deleteExistingWorktree(worktree:Worktree){
    if(!workspace)return;
    const runningChats=sidebarConversations.filter(item=>item.worktreeId===worktree.id&&runningConversationIds.includes(item.id));
    if(runningChats.length){const error=new Error("Stop active chat generation before deleting this worktree");notify(error.message,"error");throw error;}
    try{
      await api.deleteWorktree(worktree.id);
      worktrees=await api.listWorktrees(workspace.id);
      sidebarConversations=await api.listConversations(workspace.id);
      if(selectedWorktree?.id===worktree.id){
        const general=worktrees.find(item=>item.kind==="general")??worktrees[0];
        if(general)await chooseWorktree(general);
      }
      notify(`${worktree.name} deleted — its contents are preserved in General`);
    }catch(error){notify(errorMessage(error),"error");throw error;}
  }
  async function newConversation(worktree=selectedWorktree){if(!workspace||!worktree)return;try{if(selectedWorktree?.id!==worktree.id)await chooseWorktree(worktree);selectedAsset=undefined;viewedAsset=undefined;activeReferenceIds=[];focusedReferenceId=undefined;const conversation=await api.createConversation(workspace.id,worktree.id);conversations=[conversation,...conversations];sidebarConversations=[conversation,...sidebarConversations];await chooseConversation(conversation);}catch(error){notify(errorMessage(error),"error");}}
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
      const [nextAssetIds,nextAnimations,nextReferences,nextConversations]=await Promise.all([
        api.listWorktreeAssetIds(conversationWorktree.id),
        api.listAnimations(workspace.id,conversationWorktree.id),
        api.listReferenceImages(conversationWorktree.id),
        api.listConversations(workspace.id,conversationWorktree.id),
      ]);
      if(selection!==conversationSelection)return;
      worktreeAssetIds=nextAssetIds;
      animations=nextAnimations;
      references=nextReferences;
      conversations=nextConversations;
      selectedAnimation=nextAnimations[0];
      await api.setSetting(`active-worktree:${workspace.id}`,conversationWorktree.id);
    }
    const [nextMessages,nextStyle,nextProfile,nextReferenceIds]=await Promise.all([
      api.listMessages(conversation.id),
      api.getSetting(`conversation-style:${conversation.id}`).then(parseConversationStyle),
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
  async function listArchivedChats(){if(!workspace)return[];try{return await api.listArchivedConversations(workspace.id);}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function restoreChat(conversation:Conversation){try{const restored=await api.restoreConversation(conversation.id);sidebarConversations=[restored,...sidebarConversations.filter(item=>item.id!==restored.id)];if(restored.worktreeId===selectedWorktree?.id)conversations=[restored,...conversations.filter(item=>item.id!==restored.id)];await chooseConversation(restored);notify("Chat restored");}catch(error){notify(errorMessage(error),"error");throw error;}}
  function providerFor(conversation:Conversation){return providers.find(provider=>provider.id===conversation.provider);}
  async function loadGenerationProfile(conversation:Conversation){return normalizeGenerationProfile(await api.getSetting(`conversation-generation:${conversation.id}`),providerFor(conversation)?.modes??[]);}
  function composerReferenceCategory():ReferenceCategory{return selectedWorktree?.kind==="vfx"?"vfx":"other";}
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
      const context=[worktree?`Active worktree: ${worktree.name} (${worktree.kind}). ${worktree.description??""}`:"",focused?`FOCUSED CHAT REFERENCE: ${focused.path} (content hash ${focused.contentHash}). Treat this as the primary visual subject for this turn. The user may change or clear focus later; do not describe it as permanently locked.`:"",selectedAsset?`Context asset: ${selectedAsset.relativePath}`:"",`Selected style preset: ${style.name}. ${style.prompt}.`].filter(Boolean).join("\n");
      const generation={quality:profile.quality,width:profile.width,height:profile.height,frames:profile.frames,fps:profile.fps,frameMode:profile.frameMode,minFrames:profile.minFrames,maxFrames:profile.maxFrames,allowInterpolation:profile.allowInterpolation,allowAutoAdjust:profile.allowAutoAdjust};
      const command=slashCommand(prompt)??(/\b(?:asset\s+|sprite\s+|art\s+)?packs?\b/i.test(prompt)?"pack":undefined);
      const options:ProviderRequestOptions={model:profile.model||undefined,reasoningEffort:profile.reasoningEffort||undefined,command,generation,referenceIds};
      if(command==="pack")activityByConversation={...activityByConversation,[conversation.id]:["AI is planning one coordinated asset pack","Items will share one style and remain separate static sprites"]};
      else if(worktree?.kind==="tileset")activityByConversation={...activityByConversation,[conversation.id]:["AI is planning one complete terrain atlas","Output will be one PNG containing compatible fills, edges, corners, strips, and transitions"]};
      else if(generation.frameMode==="auto")activityByConversation={...activityByConversation,[conversation.id]:["AI is inspecting the reference and will recommend a frame count",`Allowed range: ${generation.minFrames}–${generation.maxFrames} frames`]};
      const requestId=await api.startProviderMessage(conversation.id,prompt,context,options);
      runningRequests={...runningRequests,[conversation.id]:{id:requestId,conversationId:conversation.id,workspaceId:workspace.id,worktreeId:worktree?.id,prompt,command,generation,knownAssetIds:assets.map(asset=>asset.id),knownPackIds:packs.map(pack=>pack.id)}};
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
    const nextAssets=await api.scanAssets(request.workspaceId);const known=new Set(request.knownAssetIds);const created=nextAssets.filter(asset=>!known.has(asset.id));
    const nextPacks=await api.listAssetPacks(request.workspaceId).catch(()=>packs);const knownPacks=new Set(request.knownPackIds);const createdPacks=nextPacks.filter(pack=>!knownPacks.has(pack.id));
    const responseText=response.toLowerCase();const mentioned=created.filter(asset=>responseText.includes(asset.path.toLowerCase())||responseText.includes(asset.relativePath.toLowerCase())||responseText.includes(asset.name.toLowerCase()));
    const generatedPack=request.command==="pack"?(createdPacks.find(pack=>responseText.includes(pack.name.toLowerCase())||responseText.includes(pack.id.toLowerCase())||pack.files.some(file=>responseText.includes(file.toLowerCase())))??(createdPacks.length===1?createdPacks[0]:undefined)):undefined;
    const manifest=await api.getGenerationManifest(request.workspaceId).catch(()=>null);
    const manifestMatches=Boolean(manifest&&(responseText.includes(manifest.name.toLowerCase())||manifest.files.some(path=>responseText.includes(path.toLowerCase()))));
    const manifestAssets=manifestMatches?manifest!.files.map(path=>nextAssets.find(asset=>asset.relativePath===path)).filter((asset):asset is Asset=>Boolean(asset)):[];
    const otherRunning=Object.values(runningRequests).filter(value=>value.id!==request.id&&value.workspaceId===request.workspaceId).length;
    const packAssets=generatedPack?.files.map(path=>nextAssets.find(asset=>asset.relativePath===path)).filter((asset):asset is Asset=>Boolean(asset))??[];
    const related=packAssets.length?packAssets:manifestAssets.length?manifestAssets:mentioned.length?mentioned:(otherRunning===0?created:[]);assets=nextAssets;
    if(request.worktreeId&&related.length)await Promise.all(related.map(asset=>api.linkAssetToWorktree(request.worktreeId!,asset.id)));
    let animationId:string|undefined;
    const ordered=manifestAssets.length?[...related]:[...related].sort((a,b)=>a.relativePath.localeCompare(b.relativePath,undefined,{numeric:true}));
    if(request.command!=="pack"&&ordered.length>1){
      const existing=await api.listAnimations(request.workspaceId,request.worktreeId);const match=existing.find(animation=>animation.frames.length===ordered.length&&ordered.every((asset,index)=>animation.frames[index]?.assetId===asset.id));
      if(match)animationId=match.id;else{const actualGeneration={...request.generation,frameMode:"fixed" as const,frames:ordered.length,minFrames:ordered.length,maxFrames:ordered.length};const motionPlan=await api.planMotion(request.prompt,actualGeneration).catch(()=>undefined);const baseName=ordered[0].name.replace(/[_-]?\d+$/i,"");const createdAnimation=await api.saveAnimation({workspaceId:request.workspaceId,worktreeId:request.worktreeId,name:baseName,fps:request.generation.fps,looping:true,frames:ordered.map(asset=>({assetId:asset.id})),motionPlan});animationId=createdAnimation.id;void api.queueQualityAnalysis(createdAnimation.id).catch(()=>undefined);}
    }
    if(request.command!=="pack"&&ordered.length){
      const requestMessages=await api.listMessages(request.conversationId);const assistant=requestMessages.findLast(message=>message.role==="assistant"&&message.status==="completed");
      if(assistant){const generation:SpriteGenerationMetadata={kind:"sprite-generation",name:ordered[0].name.replace(/[_-]?\d+$/i,""),category:ordered[0].category,fps:ordered.length>1?request.generation.fps:1,assetIds:ordered.map(asset=>asset.id),animationId};await api.updateMessageMetadata(assistant.id,{...assistant.metadata,generation});}
    }
    if(generatedPack){const requestMessages=await api.listMessages(request.conversationId);const assistant=requestMessages.findLast(message=>message.role==="assistant"&&message.status==="completed");if(assistant){const packGeneration:PackGenerationMetadata={kind:"pack-generation",packId:generatedPack.id};await api.updateMessageMetadata(assistant.id,{...assistant.metadata,packGeneration});}}
    packs=nextPacks;
    if(selectedWorktree?.id===request.worktreeId){worktreeAssetIds=request.worktreeId?await api.listWorktreeAssetIds(request.worktreeId):[];animations=await api.listAnimations(request.workspaceId,request.worktreeId);selectedAnimation=animations[0];}
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
  async function generateVfxFromStudio(prompt:string){if(!selectedConversation)throw new Error("Create a VFX chat before generating an effect");if(currentProvider?.status!=="ready")throw new Error("Connect Codex before generating an effect");const conversationId=selectedConversation.id;activeTab="chat";await send(prompt);if(!runningRequests[conversationId])throw new Error("VFX generation could not start");}
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
  async function prepareMotionInChat(asset:Asset,motion:string,polishMode:AnimationPolishMode){
    if(!selectedConversation)await newConversation();
    if(!selectedConversation){notify("Create a chat before preparing an animation","error");return;}
    selectedAsset=asset;viewedAsset=undefined;motionAsset=undefined;activeTab="chat";
    const motionReadyInstruction="If the focused source is not motion-ready because required body parts are merged or hidden, automatically create one motion-ready source-master revision with ImageGen, guided by the focused source, and continue without asking permission. ";
    const polishInstruction=polishMode==="ai-polish"?`Polish mode: AI polish. ${motionReadyInstruction}Build the complete deterministic rig first, then use ImageGen only to edit difficult frames while preserving each rigged pose; normalize and validate every polished frame with sprite_polish.py.`:polishMode==="full-redraw"?`Polish mode: Full redraw experimental. ${motionReadyInstruction}Build the complete deterministic rig first, then redraw every rough frame using the master, rough pose, and adjacent frames as strict references; normalize and drift-check every result with sprite_polish.py.`:"Polish mode: Rig only. Do not use ImageGen for animation frames.";
    chatDraft=`/animate Use ${asset.relativePath} as the exact source sprite. Motion: ${motion}. ${polishInstruction} Inspect the visible subject and use the natural mechanics of that anatomy; preserve its identity, palette, and proportions.`;
    notify(`${asset.name} is attached — review the motion prompt and send when ready`);
  }
  function updateAsset(asset:Asset){selectedAsset=asset;assets=assets.map(item=>item.id===asset.id?asset:item);}
  async function assetDeleted(){selectedAsset=undefined;if(workspace)assets=await api.scanAssets(workspace.id);}
  function applyTheme(value:string){theme=value;document.documentElement.dataset.theme=value;}
  async function changeTheme(value:string){applyTheme(value);try{await api.setSetting("theme",value);}catch(error){notify(errorMessage(error),"error");}}
  async function changeWorkspaceStyle(value:StylePresetId){if(!workspace)return;workspaceStyle=value;try{await api.setSetting(`workspace-style:${workspace.id}`,value);notify(`${stylePreset(value).name} is now the workspace style`);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationStyle(value:ConversationStyleId){if(!selectedConversation)return;conversationStyle=value;try{await api.setSetting(`conversation-style:${selectedConversation.id}`,value);notify(value==="inherit"?"Chat now follows the workspace style":`${stylePreset(value).name} applied to this chat`);}catch(error){notify(errorMessage(error),"error");}}
  async function changeGenerationProfile(value:ChatGenerationProfile){if(!selectedConversation)return;generationProfile=normalizeGenerationProfile(value,currentProvider?.modes??[]);try{await api.setSetting(`conversation-generation:${selectedConversation.id}`,generationProfile);}catch(error){notify(errorMessage(error),"error");}}
  async function refreshProviders(){try{providers=await api.detectProviders();notify("Provider detection refreshed");}catch(error){notify(errorMessage(error),"error");}}
  async function renameWorkspace(){if(!workspace||!renameValue.trim())return;try{await api.renameWorkspace(workspace.id,renameValue);workspace={...workspace,name:renameValue.trim()};workspaceMenu=false;await loadWorkspaces();notify("Workspace renamed");}catch(error){notify(errorMessage(error),"error");}}
  async function removeWorkspace(deleteFiles:boolean){if(!workspace)return;const question=deleteFiles?`Permanently delete ${workspace.name} and every file in its folder?`:`Remove ${workspace.name} from Sprite Studio? Its files will stay on disk.`;if(!window.confirm(question))return;try{if(deleteFiles)await api.deleteWorkspace(workspace.id);else await api.removeWorkspace(workspace.id);workspaceMenu=false;await goHome();notify(deleteFiles?"Workspace files deleted":"Workspace removed from recents");}catch(error){notify(errorMessage(error),"error");}}
  async function createProjectBackup(){if(!workspace)return;const destination=await open({directory:true,multiple:false,title:"Choose where to save the project backup"});if(typeof destination!=="string")return;backupBusy=true;try{const backup=await api.createProjectBackup(workspace.id,destination);notify(`Backup created with ${backup.fileCount} files`);await revealItemInDir(backup.backupPath);}catch(error){notify(errorMessage(error),"error");}finally{backupBusy=false;}}
  async function restoreProjectBackup(){if(!workspace)return;const selected=await open({directory:true,multiple:false,title:"Choose a .sprite-studio-backup folder"});if(typeof selected!=="string")return;if(!window.confirm(`Restore ${workspace.name} from this backup? Sprite Studio will first create a safety backup of the current project, then replace its files and metadata.`))return;backupBusy=true;try{const restored=await api.restoreProjectBackup(workspace.id,selected);workspaceMenu=false;await loadWorkspace(restored);notify("Project restored — a safety backup preserved the previous state");}catch(error){notify(errorMessage(error),"error");}finally{backupBusy=false;}}

  onMount(()=>{
    initialLoad();
    const shortcuts=(event:KeyboardEvent)=>{
      if(!(event.metaKey||event.ctrlKey)||event.altKey||event.shiftKey)return;
      const order=selectedWorktree?.kind==="vfx"?["chat","sprites","references","animate","vfx","sheets","packs","play"]:selectedWorktree?.kind==="tileset"?["chat","sprites","references","animate","terrain","sheets","packs","play"]:["chat","sprites","references","animate","sheets","packs","play"];
      const tab=order[Number(event.key)-1];
      if(tab){event.preventDefault();activeTab=tab;}
    };
    window.addEventListener("keydown",shortcuts);
    const unlistenPromise=listen<ProviderEvent>("provider-event",async({payload})=>{
      const selected=payload.conversationId===selectedConversation?.id;
      if(payload.eventType==="activity"||payload.eventType==="started"){const current=activityByConversation[payload.conversationId]??[];activityByConversation={...activityByConversation,[payload.conversationId]:[...current,payload.content]};}
      if(payload.eventType==="content"&&selected){
        const index=messages.findLastIndex(message=>message.role==="assistant"&&message.status==="running");
        if(index>=0){const next=[...messages];next[index]={...next[index],content:(next[index].content?next[index].content+"\n":"")+payload.content};messages=next;}
      }
      if(["completed","failed","cancelled"].includes(payload.eventType)){
        const request=runningRequests[payload.conversationId];const next={...runningRequests};if(next[payload.conversationId]?.id===payload.requestId)delete next[payload.conversationId];runningRequests=next;
        if(payload.eventType==="completed"&&request)await finalizeChatGeneration(request,payload.content);
        if(selected)messages=await api.listMessages(payload.conversationId);
        if(payload.eventType==="failed")notify(payload.content,"error");
      }
    });
    return()=>{unlistenPromise.then(unlisten=>unlisten());window.removeEventListener("keydown",shortcuts);if(toastTimer)clearTimeout(toastTimer);};
  });
</script>

<svelte:head><title>{workspace ? `${workspace.name} — Sprite Studio` : "Sprite Studio"}</title><meta name="description" content="Local-first AI game asset development environment"/></svelte:head>

{#if loading}
  <div class="boot"><div class="boot-mark"><LogoMark size={27}/></div><span></span><p>Opening Sprite Studio</p></div>
{:else if !workspace}
  <WelcomeScreen {workspaces} onOpen={loadWorkspace} onCreated={acceptWorkspace} onChanged={loadWorkspaces} onError={(message)=>notify(message,"error")}/>
{:else}
  <main class="studio">
    {#if sidebarOpen}<WorkspaceSidebar {workspace} {worktrees} selectedWorktreeId={selectedWorktree?.id} conversations={sidebarConversations} selectedConversationId={selectedConversation?.id} {runningConversationIds} onWorktree={chooseWorktree} onNewWorktree={()=>worktreeDialog=true} onUpdateWorktree={updateExistingWorktree} onDeleteWorktree={deleteExistingWorktree} onConversation={chooseConversation} onNewConversation={newConversation} onRenameConversation={renameChat} onArchiveConversation={archiveChat} onListArchivedConversations={listArchivedChats} onRestoreConversation={restoreChat} onSettings={()=>settingsOpen=true} onHome={goHome} onCollapse={()=>sidebarOpen=false}/>{:else}<div class="rail"><button onclick={()=>sidebarOpen=true} title="Open project navigation"><Menu size={15}/></button><button onclick={()=>{renameValue=workspace?.name??"";workspaceMenu=true}} title="Project options"><MoreHorizontal size={15}/></button></div>{/if}
    <section class="main-pane">
      {#if sidebarOpen}<button class="workspace-menu-button" onclick={()=>{renameValue=workspace?.name??"";workspaceMenu=true}} title="Workspace options"><MoreHorizontal size={14}/></button>{/if}
      <WorkspaceTabs active={activeTab} conversationTitle={selectedConversation?.title} worktreeName={selectedWorktree?.name} assetCount={spriteCount} referenceCount={references.length} animationCount={animations.length} packCount={visiblePacks.length} showVfx={selectedWorktree?.kind==="vfx"} showTerrain={selectedWorktree?.kind==="tileset"} onSelect={selectTab}/>
      <div class="tab-stack">
        <div class="tab-panel" class:active={activeTab==="chat"} aria-hidden={activeTab!=="chat"}>{#key selectedConversation?.id}<ConversationView conversation={selectedConversation} {messages} provider={currentProvider} runningRequestId={currentRequest?.id} activity={currentActivity} {selectedAsset} {assets} {animations} packs={visiblePacks} {references} {activeReferenceIds} {focusedReferenceId} draftPrompt={chatDraft} onDraftConsumed={()=>chatDraft=""} workspacePath={workspace.path} {workspaceStyle} {conversationStyle} {generationProfile} onSend={send} onCancel={cancel} onClearAsset={()=>selectedAsset=undefined} onEditAsset={editAssetFromChat} onEditAnimation={editAnimationFromChat} onViewPack={openPackFromChat} onExportAsset={exportAssetFromChat} onExportAnimation={exportAnimationFromChat} onConversationStyle={changeConversationStyle} onGenerationProfile={changeGenerationProfile} onAttachReferencePaths={attachReferencePaths} onAttachReferenceFiles={attachReferenceFiles} onFocusReference={focusConversationReference} onRemoveReference={removeConversationReference} onLinkError={(message)=>notify(message,"error")}/>{/key}</div>
        <div class="tab-panel" class:active={activeTab==="sprites"} aria-hidden={activeTab!=="sprites"}><AssetBrowser workspaceId={workspace.id} worktreeId={selectedWorktree?.id} assets={visibleAssets} {animations} packs={visiblePacks} packId={packFilter} selectedAssetId={selectedAsset?.id} onAssets={(value)=>assets=value} onSelect={selectAsset} onOpen={openSpriteGroup} onPack={(value)=>packFilter=value} onLinked={async()=>{if(selectedWorktree)worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id)}} onError={(message)=>notify(message,"error")}/></div>
        <div class="tab-panel" class:active={activeTab==="references"} aria-hidden={activeTab!=="references"}>{#if selectedWorktree}<ReferenceLibrary worktreeId={selectedWorktree.id} conversationId={selectedConversation?.id} {references} activeIds={activeReferenceIds} maximumActive={currentProvider?.capabilities.maximumReferenceImages ?? 0} onReferences={(value)=>references=value} onActiveIds={(value)=>activeReferenceIds=value} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>{/if}</div>
        <div class="tab-panel" class:active={activeTab==="animate"} aria-hidden={activeTab!=="animate"}><AnimationEditor workspaceId={workspace.id} worktreeId={activeWorktreeId()} assets={visibleAssets} {animations} templates={animationTemplates} {selectedAnimation} active={activeTab==="animate"} onAnimations={(value)=>animations=value} onTemplates={(value)=>animationTemplates=value} onSelected={(value)=>selectedAnimation=value} onTemplateApplication={prepareTemplateInChat} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/></div>
        {#if selectedWorktree?.kind==="vfx"}<div class="tab-panel" class:active={activeTab==="vfx"} aria-hidden={activeTab!=="vfx"}><VfxStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} {animations} assets={visibleAssets} active={activeTab==="vfx"} onCreated={refreshVfxAssets} onOpenSheets={openVfxSheet} onGenerate={generateVfxFromStudio} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/></div>{/if}
        {#if selectedWorktree?.kind==="tileset"}<div class="tab-panel" class:active={activeTab==="terrain"} aria-hidden={activeTab!=="terrain"}><TerrainStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} assets={visibleAssets} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/></div>{/if}
        <div class="tab-panel" class:active={activeTab==="sheets"} aria-hidden={activeTab!=="sheets"}><SpriteSheetStudio workspaceId={workspace.id} worktreeId={activeWorktreeId()} {animations} assets={visibleAssets} active={activeTab==="sheets"} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/></div>
        <div class="tab-panel" class:active={activeTab==="packs"} aria-hidden={activeTab!=="packs"}><PackLibrary packs={visiblePacks} assets={visibleAssets} {selectedPackId} onView={viewPack} onBack={()=>selectedPackId=""} onOpen={openPackAsset}/></div>
        <div class="tab-panel" class:active={activeTab==="play"} aria-hidden={activeTab!=="play"}><TestRoom {animations} assets={visibleAssets} {selectedAnimation} active={activeTab==="play"}/></div>
      </div>
    </section>
    {#if selectedAsset && activeTab==="sprites"}<AssetInspector asset={selectedAsset} {animations} onClose={()=>selectedAsset=undefined} onChanged={updateAsset} onDeleted={assetDeleted} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>{/if}
  </main>
{/if}

{#if settingsOpen}<SettingsModal {providers} {theme} {workspaceStyle} onTheme={changeTheme} onWorkspaceStyle={changeWorkspaceStyle} onRefresh={refreshProviders} onClose={()=>settingsOpen=false}/>{/if}
{#if worktreeDialog}<WorktreeDialog busy={creatingWorktree} onCreate={createWorktree} onClose={()=>worktreeDialog=false}/>{/if}
{#if viewedAsset}<SpriteViewer asset={viewedAsset} onAnimate={animateViewedAsset} onDownload={exportAssetFromChat} onClose={()=>viewedAsset=undefined}/>{/if}
{#if motionAsset}<MotionPromptDialog asset={motionAsset} onContinue={(motion,polishMode)=>prepareMotionInChat(motionAsset!,motion,polishMode)} onClose={()=>motionAsset=undefined}/>{/if}
{#if workspaceMenu && workspace}<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&!backupBusy&&(workspaceMenu=false)}><div class="workspace-dialog"><header><div><p>WORKSPACE</p><h2>Manage {workspace.name}</h2></div><button onclick={()=>workspaceMenu=false} disabled={backupBusy}><X size={15}/></button></header><label>Name<div><input bind:value={renameValue} disabled={backupBusy}/><button onclick={renameWorkspace} disabled={backupBusy}><Pencil size={12}/>Rename</button></div></label><p class="path">{workspace.path}</p><div class="backup-actions"><div><strong>Project backups</strong><span>Preserve files, chats, worktrees, timelines, and metadata together.</span></div><div><button onclick={createProjectBackup} disabled={backupBusy}><Archive size={13}/>{backupBusy?"Working…":"Create backup"}</button><button onclick={restoreProjectBackup} disabled={backupBusy}><RotateCcw size={13}/>Restore…</button></div></div><div class="danger-actions"><button onclick={()=>removeWorkspace(false)} disabled={backupBusy}><FolderMinus size={13}/>Remove from app</button><button onclick={()=>removeWorkspace(true)} disabled={backupBusy}><Trash2 size={13}/>Delete files…</button></div></div></div>{/if}
{#if toast}<div class="toast" class:error={toast.kind==="error"}><span></span><p>{toast.message}</p><button onclick={()=>toast=undefined}><X size={12}/></button></div>{/if}

<style>
  :global(*){box-sizing:border-box}:global(html){font-family:Inter,"SF Pro Text",-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-size:15px;letter-spacing:-.01em;background:var(--bg);color:var(--text);font-synthesis:none;text-rendering:optimizeLegibility;-webkit-font-smoothing:antialiased}:global(body){margin:0;overflow:hidden}:global(button),:global(input),:global(textarea),:global(select){font-family:inherit}:global(:focus-visible){outline:2px solid var(--accent)!important;outline-offset:2px}
  :global(:root),:global([data-theme="dark"]){--bg:#090a0a;--sidebar:#111212;--surface:#1b1c1c;--surface-hover:#222323;--composer:#202121;--selected:#272828;--preview:#0b0c0c;--checker:#161717;--text:#f4f4f5;--muted:#a1a1a4;--faint:#707174;--border:#282929;--border-strong:#3b3c3d;--accent:#8b5cf6;--accent-dim:#8b5cf633}
  :global([data-theme="light"]){--bg:#f7f7f5;--sidebar:#efefed;--surface:#fff;--surface-hover:#e8e8e5;--composer:#fff;--selected:#dfe1e3;--preview:#e9eae8;--checker:#d9dad7;--text:#202327;--muted:#60666e;--faint:#8b9198;--border:#d7d8d5;--border-strong:#bfc2c4;--accent:#496fc2;--accent-dim:#6687cc33}
  @media(prefers-color-scheme:light){:global([data-theme="system"]){--bg:#f7f7f5;--sidebar:#efefed;--surface:#fff;--surface-hover:#e8e8e5;--composer:#fff;--selected:#dfe1e3;--preview:#e9eae8;--checker:#d9dad7;--text:#202327;--muted:#60666e;--faint:#8b9198;--border:#d7d8d5;--border-strong:#bfc2c4;--accent:#496fc2;--accent-dim:#6687cc33}}
  .studio{width:100vw;height:100vh;display:flex;background:var(--bg);color:var(--text)}.main-pane{flex:1;min-width:0;height:100%;position:relative;display:grid;grid-template-rows:44px minmax(0,1fr)}.tab-stack{position:relative;min-width:0;min-height:0;overflow:hidden}.tab-panel{position:absolute;inset:0;visibility:hidden;pointer-events:none}.tab-panel.active{visibility:visible;pointer-events:auto}.workspace-menu-button{position:absolute;z-index:10;top:8px;left:-39px;width:30px;height:30px;border:0;background:transparent;color:var(--faint);border-radius:6px;display:grid;place-items:center;cursor:pointer}.workspace-menu-button:hover{background:var(--surface-hover);color:var(--text)}.rail{width:48px;min-width:48px;border-right:1px solid var(--border);background:var(--sidebar);display:flex;flex-direction:column;align-items:center;padding:7px 0;gap:6px}.rail button{width:31px;height:31px;border:0;background:transparent;color:var(--faint);border-radius:6px;display:grid;place-items:center;cursor:pointer}.rail button:hover{background:var(--surface-hover);color:var(--text)}
  .boot{width:100vw;height:100vh;background:#090a0a;display:flex;flex-direction:column;align-items:center;justify-content:center;color:#707174}.boot-mark{--logo-pixel:#f4f4f5;width:46px;height:46px;border:1px solid #3b3c3d;border-radius:10px;display:grid;place-items:center;color:#f5a524;background:#171818}.boot>span{width:90px;height:1px;background:#282929;position:relative;margin-top:22px;overflow:hidden}.boot>span:after{content:"";position:absolute;width:35px;height:1px;background:#8b5cf6;animation:load 1.2s ease-in-out infinite}.boot p{font-size:12px;letter-spacing:.06em;margin-top:13px}@keyframes load{from{left:-35px}to{left:90px}}
  .toast{position:fixed;right:16px;bottom:16px;z-index:70;min-width:260px;max-width:420px;min-height:40px;background:var(--surface);border:1px solid var(--border-strong);box-shadow:0 12px 34px #0006;border-radius:6px;display:grid;grid-template-columns:7px minmax(0,1fr) 24px;align-items:center;padding:0 7px}.toast>span{width:6px;height:6px;border-radius:50%;background:#5cad7b}.toast.error>span{background:#d16f69}.toast p{font-size:12px;line-height:1.4;margin:10px 8px;color:var(--text)}.toast button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}
  .backdrop{position:fixed;inset:0;background:#0009;display:grid;place-items:center;z-index:45}.workspace-dialog{width:min(480px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:8px;box-shadow:0 24px 70px #0009;padding:20px}.workspace-dialog header{display:flex;align-items:flex-start;justify-content:space-between}.workspace-dialog header p{font-size:10px;letter-spacing:.14em;color:var(--faint);font-weight:700;margin:0 0 7px}.workspace-dialog h2{font-size:16px;margin:0}.workspace-dialog header button{border:0;background:transparent;color:var(--faint);width:26px;height:26px;display:grid;place-items:center;cursor:pointer}.workspace-dialog label{font-size:11px;color:var(--muted);display:block;margin-top:24px}.workspace-dialog label>div{display:flex;gap:6px;margin-top:7px}.workspace-dialog input{height:31px;min-width:0;flex:1;background:var(--bg);border:1px solid var(--border-strong);border-radius:4px;color:var(--text);padding:0 8px;font-size:12px;outline:0}.workspace-dialog label button,.danger-actions button,.backup-actions button{height:31px;border:1px solid var(--border);background:var(--bg);color:var(--muted);border-radius:4px;display:flex;align-items:center;gap:6px;padding:0 9px;font:inherit;font-size:11px;cursor:pointer}.workspace-dialog .path{font-size:10px;color:var(--faint);overflow-wrap:anywhere;margin:10px 0 18px}.backup-actions{border-top:1px solid var(--border);padding:15px 0;display:flex;align-items:flex-end;justify-content:space-between;gap:14px}.backup-actions strong,.backup-actions span{display:block}.backup-actions strong{font-size:11px}.backup-actions span{font-size:9px;line-height:1.45;color:var(--faint);margin-top:4px;max-width:230px}.backup-actions>div:last-child{display:flex;gap:6px}.backup-actions button:first-child{color:var(--accent)}.danger-actions{border-top:1px solid var(--border);padding-top:15px;display:flex;justify-content:space-between}.danger-actions button:last-child{color:#cf7772}
</style>
