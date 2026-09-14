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
  import UserGuide from "$lib/components/UserGuide.svelte";
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
  import ProductionSessionsGallery from "$lib/components/ProductionSessionsGallery.svelte";
  import MotionPromptDialog from "$lib/components/MotionPromptDialog.svelte";
  import SettingsModal from "$lib/components/SettingsModal.svelte";
  import WorktreeDialog from "$lib/components/WorktreeDialog.svelte";
  import LogoMark from "$lib/components/LogoMark.svelte";
  import { api } from "$lib/api";
  import { displayPath } from "$lib/display-path";
  import { defaultImageProviderId, normalizeGenerationProfile } from "$lib/generation-profiles";
  import { buildSpriteGroups, type SpriteGroup } from "$lib/sprite-groups";
  import { animationPolishModeOption } from "$lib/animation-polish-modes";
  import { stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import { type CustomArtStyle, type CustomSkill } from "$lib/library-types";
  import { errorMessage, type Animation, type AnimationPolishMode, type AnimationTemplate, type Asset, type AssetPack, type ChatGenerationProfile, type Conversation, type GenerationManifest, type ImageProviderInput, type Message, type ProviderEvent, type ProviderStatus, type ReferenceImage, type Rig, type SidebarSnapshot, type TemplateApplication, type Workspace, type Worktree, type WorktreeKind } from "$lib/types";
  import { hydrateGenerationData, reconcileManifestData } from "$lib/generation-reconcile";
  import {
    completeChatGeneration, continueAfterMasterProviderPhase, qualityNoticeForAnimation, runNativeRigChatAnimation, startAiPolishHandoff, startChatRequest,
  } from "$lib/chat-generation-run";
  import { normalizeManifestPath } from "$lib/manifest-path";
  import {
    extractAnimateMotion, isAnimateWithoutResolvableMaster, needsNativeRigMasterPhase,
    resolveAnimateMasterAsset, resolvePolishMode, shouldOrchestrateNativeRig, shouldRunNativeRigFirst,
  } from "$lib/rig-animation-orchestrator";
  import {
    type ActiveChatRequest, animationFrameAssets, appendAssistantDelta, appendConversationActivity, buildChatContext,
    applyAnimationPolishModeToPrompt, buildFullRedrawPrompt, buildMotionPrompt, buildProviderOptions, buildRigPolishPrompt, chatActivityLines, clearRunningRequest, parallelGenerationsInWorkspace,
    generationRequestFromProfile, inferChatCommand, isTerminalProviderEvent, unacceptedGenerationNotice,
  } from "$lib/chat-generation-finalize";
  import {
    attachedReferenceNotice, composerReferenceCategory, importReferenceFiles, importReferencePaths, mergeImportedReferences,
    persistConversationReferences, referenceOverflowNotice, remainingReferenceSlots,
  } from "$lib/reference-import";
  import {
    activeWorktreeId as worktreeQueryId, chatsForWorktree, fetchConversationSession, fetchInitialShell, fetchWorkspaceLoad,
    fetchWorktreeStudio, refreshWorktreeMedia, restoreConversationFocus, selectSavedConversation,
  } from "$lib/workspace-load";

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
  let nativeRigAbortControllers = $state<Record<string, AbortController>>({});
  let generationManifest = $state<GenerationManifest | null>(null);
  let activityByConversation = $state<Record<string,import("$lib/generation-status").GenerationActivityEntry[]>>({});
  let toast = $state<{message:string;kind:"error"|"notice"}>();
  let theme = $state("system");
  let workspaceStyle = $state<StylePresetId>("pixel-rpg");
  let conversationStyle = $state<ConversationStyleId>("inherit");
  let conversationAnimationMode = $state<AnimationPolishMode>("rig");
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
  function linkedAnimationForRig(rigId: string): Animation | undefined {
    if (!generationManifest?.rigId || generationManifest.rigId !== rigId) return undefined;
    const manifestPaths = new Set(generationManifest.files.map(normalizeManifestPath));
    return animations.find(animation => {
      const paths = animationFrameAssets(animation, assets).map(asset => normalizeManifestPath(asset.relativePath));
      return paths.length > 0 && paths.every(path => manifestPaths.has(path));
    });
  }
  function linkedAnimationNameForRig(rigId: string): string | undefined {
    return linkedAnimationForRig(rigId)?.name;
  }
  function linkedAnimationIdForRig(rigId: string): string | undefined {
    return linkedAnimationForRig(rigId)?.id;
  }
  const currentActivity = $derived(selectedConversation ? activityByConversation[selectedConversation.id] ?? [] : []);
  const runningConversationIds = $derived(Object.keys(runningRequests));
  const effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle,customArts));
  const animationAssetIds = $derived(new Set(animations.flatMap(animation => animation.frames.map(frame => frame.assetId))));
  const visibleAssets = $derived(selectedWorktree?.kind === "general" || !selectedWorktree ? assets : assets.filter(asset => worktreeAssetIds.includes(asset.id) || animationAssetIds.has(asset.id)));
  const visiblePacks = $derived(packs.filter(pack => pack.files.some(file => visibleAssets.some(asset => asset.relativePath === file))));
  const spriteCount = $derived(buildSpriteGroups(visibleAssets, animations).length);
  let sessionCount = $state(0);
  async function refreshSessionCount() {
    if (!workspace) { sessionCount = 0; return; }
    try {
      sessionCount = (await api.listGenerationSessions(workspace.id, activeWorktreeId())).length;
    } catch {
      sessionCount = 0;
    }
  }
  $effect(() => {
    if (workspace) void refreshSessionCount();
  });
  const mediaTabs=["sprites","references","animate","rig","sessions","terrain","vfx","sheets","packs","play"];
  const activePrimary=$derived(activeTab==="chat"?"chat":mediaTabs.includes(activeTab)?"media":activeTab);
  function activeWorktreeId(){return worktreeQueryId(selectedWorktree);}
  function generationOptions(){return generationRequestFromProfile(generationProfile);}
  async function currentMotionPlan(){const user=messages.findLast(message=>message.role==="user");if(!user)return undefined;return api.planMotion(user.content,generationOptions()).catch(()=>undefined);}

  function notify(message:string,kind:"error"|"notice"="notice") {
    toast={message,kind}; if(toastTimer)window.clearTimeout(toastTimer);toastTimer=window.setTimeout(()=>toast=undefined,4200);
  }
  function resetGenerationSessions(){
    for(const controller of Object.values(nativeRigAbortControllers))controller.abort();
    nativeRigAbortControllers={};
    runningRequests={};
    activityByConversation={};
  }
  async function loadWorkspaces() {
    workspaces=(await api.loadSidebarState()).workspaces;
  }
  async function loadWorkspace(selected:Workspace,preloaded?:SidebarSnapshot) {
    const selection=++workspaceSelection;
    let loadStage="opening project";
    try {
      resetGenerationSessions();
      workspace=selected;
      worktrees=[];sidebarConversations=[];conversations=[];selectedWorktree=undefined;selectedConversation=undefined;messages=[];selectedRigId=undefined;rigDraftAssetId=undefined;
      const result=await fetchWorkspaceLoad(selected,preloaded,customArts,providers,()=>selection!==workspaceSelection,stage=>{loadStage=stage;});
      if(!result)return;
      workspace=result.workspace;workspaces=result.workspaces;worktrees=result.worktrees;sidebarConversations=result.sidebarConversations;selectedWorktree=result.selectedWorktree;conversations=result.conversations;assets=result.assets;packs=result.packs;worktreeAssetIds=result.worktreeAssetIds;animations=result.animations;rigs=result.rigs;animationTemplates=result.animationTemplates;workspaceStyle=result.workspaceStyle;references=result.references;selectedConversation=result.selectedConversation;messages=result.messages;conversationStyle=result.conversationStyle;conversationAnimationMode=result.conversationAnimationMode;generationProfile=result.generationProfile;activeReferenceIds=result.activeReferenceIds;focusedReferenceId=result.focusedReferenceId;selectedRigId=result.rigs[0]?.id;rigDraftAssetId=undefined;
      generationManifest=await api.getGenerationManifest(selected.id).catch(()=>null);
      await reconcileGenerationManifest().catch(error=>notify(`Project opened, but the latest generation could not be reconciled: ${errorMessage(error)}`,"error"));
      selectedAnimation=animations[0];selectedAsset=undefined;activeTab="chat";
      await hydrateLatestGeneration().catch(error=>notify(`Project opened, but the latest chat preview could not be restored: ${errorMessage(error)}`,"error"));
      void api.scanAssets(workspace.id).then(nextAssets=>{if(selection===workspaceSelection)assets=nextAssets;}).catch(error=>notify(`Project opened, but its asset refresh could not finish: ${errorMessage(error)}`,"error"));
    } catch(error) { if(selection===workspaceSelection){workspace=undefined;notify(`Could not finish ${loadStage}: ${errorMessage(error)}`,"error");} }
  }
  async function initialLoad() {
    try {
      const shell=await fetchInitialShell();
      customSkills=shell.customSkills;customArts=shell.customArts;defaultProvider=shell.defaultProvider;applyTheme(shell.theme);
      workspaces=shell.snapshot.workspaces;loading=false;
      void api.detectProviders().then(value=>providers=value).catch(error=>notify(`Provider detection could not finish: ${errorMessage(error)}`,"error"));
      if(shell.recent){await loadWorkspace(shell.recent,shell.snapshot);return;}
    } catch(error){notify(errorMessage(error),"error");}
    loading=false;
  }
  async function acceptWorkspace(value:Workspace){await loadWorkspace(value);}
  async function goHome(){resetGenerationSessions();workspace=undefined;worktrees=[];selectedWorktree=undefined;worktreeAssetIds=[];references=[];activeReferenceIds=[];focusedReferenceId=undefined;animationTemplates=[];packs=[];packFilter="";conversations=[];sidebarConversations=[];selectedConversation=undefined;messages=[];generationProfile=normalizeGenerationProfile(null);rigs=[];selectedRigId=undefined;rigDraftAssetId=undefined;await api.setSetting("activeWorkspaceId",null);await loadWorkspaces();}
  async function chooseWorktree(value:Worktree){resetGenerationSessions();selectedWorktree=value;selectedAsset=undefined;packFilter="";activeTab="chat";if(!workspace)return;const loaded=await fetchWorktreeStudio(workspace.id,value);worktreeAssetIds=loaded.worktreeAssetIds;animations=loaded.animations;rigs=loaded.rigs;references=loaded.references;conversations=chatsForWorktree(sidebarConversations,value);selectedAnimation=animations[0];selectedRigId=rigs[0]?.id;const savedConversationId=await api.getSetting(`active-conversation:${value.id}`);selectedConversation=selectSavedConversation(conversations,savedConversationId);if(selectedConversation){await chooseConversation(selectedConversation);}else{messages=[];activeReferenceIds=[];focusedReferenceId=undefined;conversationStyle="inherit";conversationAnimationMode="rig";generationProfile=normalizeGenerationProfile(null);}await api.setSetting(`active-worktree:${workspace.id}`,value.id);}
  async function createWorktree(name:string,kind:WorktreeKind,description?:string){if(!workspace)return;creatingWorktree=true;try{const created=await api.createWorktree(workspace.id,name,kind,description);worktrees=await api.listWorktrees(workspace.id);await chooseWorktree(created);if(!conversations.length)await newConversation();worktreeDialog=false;notify(`${created.name} worktree created — its first chat is ready`);}catch(error){notify(errorMessage(error),"error");}finally{creatingWorktree=false;}}
  async function newConversation(worktree=selectedWorktree){if(!workspace){projectDialogOpen=true;return;}if(!worktree)return;try{if(selectedWorktree?.id!==worktree.id)await chooseWorktree(worktree);selectedAsset=undefined;viewedAsset=undefined;activeReferenceIds=[];focusedReferenceId=undefined;const conversation=await api.createConversation(workspace.id,worktree.id,undefined,defaultProvider);conversations=[conversation,...conversations];sidebarConversations=[conversation,...sidebarConversations];await chooseConversation(conversation);}catch(error){notify(errorMessage(error),"error");}}
  async function chooseConversation(conversation:Conversation){
    const selection=++conversationSelection;selectedConversation=conversation;selectedAsset=undefined;viewedAsset=undefined;packFilter="";activeTab="chat";if(!workspace)return;
    const conversationWorktree=conversation.worktreeId?worktrees.find(item=>item.id===conversation.worktreeId):undefined;
    if(conversationWorktree&&conversationWorktree.id!==selectedWorktree?.id){selectedWorktree=conversationWorktree;selectedAsset=undefined;const loaded=await fetchWorktreeStudio(workspace.id,conversationWorktree);if(selection!==conversationSelection)return;worktreeAssetIds=loaded.worktreeAssetIds;animations=loaded.animations;rigs=loaded.rigs;references=loaded.references;conversations=chatsForWorktree(sidebarConversations,conversationWorktree);selectedAnimation=loaded.animations[0];selectedRigId=loaded.rigs[0]?.id;await api.setSetting(`active-worktree:${workspace.id}`,conversationWorktree.id);}
    const session=await fetchConversationSession(conversation,customArts,providers.find(provider=>provider.id===conversation.provider));if(selection!==conversationSelection)return;
    messages=session.messages;conversationStyle=session.style;conversationAnimationMode=session.animationMode;generationProfile=session.profile;activeReferenceIds=session.referenceIds.filter(id=>references.some(reference=>reference.id===id));
    const focus=await restoreConversationFocus(conversation,references,activeReferenceIds);focusedReferenceId=focus.focusedReferenceId;activeReferenceIds=focus.activeReferenceIds;
    if(selectedWorktree)await api.setSetting(`active-conversation:${selectedWorktree.id}`,conversation.id);
  }
  async function renameChat(conversation:Conversation,title:string){try{await api.renameConversation(conversation.id,title);const renamed={...conversation,title};sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);conversations=conversations.map(item=>item.id===conversation.id?renamed:item);if(selectedConversation?.id===conversation.id)selectedConversation={...selectedConversation,title};notify("Chat renamed");}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function archiveChat(conversation:Conversation){if(runningRequests[conversation.id]){notify("Stop this chat’s generation before archiving it","error");return;}try{await api.archiveConversation(conversation.id);sidebarConversations=sidebarConversations.filter(item=>item.id!==conversation.id);conversations=conversations.filter(item=>item.id!==conversation.id);if(selectedConversation?.id===conversation.id){const next=conversations[0];if(next)await chooseConversation(next);else await newConversation(selectedWorktree);}notify("Chat archived");}catch(error){notify(errorMessage(error),"error");}}
  async function listArchivedChats(){if(!workspace)return [];try{return await api.listArchivedConversations(workspace.id);}catch(error){notify(errorMessage(error),"error");return [];}}
  async function restoreArchivedChat(conversation:Conversation){try{const restored=await api.restoreConversation(conversation.id);sidebarConversations=[restored,...sidebarConversations.filter(item=>item.id!==restored.id)];conversations=chatsForWorktree(sidebarConversations,selectedWorktree);await chooseConversation(restored);notify("Chat restored");}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function activateImportedReferences(created:ReferenceImage[]){if(!selectedConversation)return;await persistConversationReferences(selectedConversation.id,created);const merged=mergeImportedReferences(references,activeReferenceIds,created);references=merged.references;activeReferenceIds=merged.activeReferenceIds;notify(attachedReferenceNotice(created.length));}
  async function attachReferencePaths(paths:string[]){
    if(!selectedWorktree||!selectedConversation){notify("Select a worktree chat before adding a reference","error");return;}
    const slots=remainingReferenceSlots(currentProvider?.capabilities.maximumReferenceImages??0,activeReferenceIds.length);if(!slots){notify("This provider cannot accept more reference images","error");return;}
    try{const created=await importReferencePaths(selectedWorktree.id,paths,slots,composerReferenceCategory(activeTab));await activateImportedReferences(created);if(paths.length>slots)notify(referenceOverflowNotice(slots,currentProvider?.capabilities.maximumReferenceImages),"error");}
    catch(error){notify(errorMessage(error),"error");}
  }
  async function attachReferenceFiles(files:File[]){
    if(!selectedWorktree||!selectedConversation){notify("Select a worktree chat before pasting a reference","error");return;}
    const slots=remainingReferenceSlots(currentProvider?.capabilities.maximumReferenceImages??0,activeReferenceIds.length);if(!slots){notify("This provider cannot accept more reference images","error");return;}
    try{const created=await importReferenceFiles(selectedWorktree.id,files,slots,composerReferenceCategory(activeTab));await activateImportedReferences(created);if(files.length>slots)notify(referenceOverflowNotice(slots,currentProvider?.capabilities.maximumReferenceImages),"error");}
    catch(error){notify(errorMessage(error),"error");}
  }
  async function focusConversationReference(id?:string){if(!selectedConversation)return;try{const previous=focusedReferenceId;if(previous&&previous!==id)await api.setConversationReference(selectedConversation.id,previous,true,1);await Promise.all([api.setSetting(`conversation-focus:${selectedConversation.id}`,id??null),api.setSetting(`conversation-master:${selectedConversation.id}`,null)]);if(id){await api.setConversationReference(selectedConversation.id,id,true,2);activeReferenceIds=[id,...activeReferenceIds.filter(value=>value!==id)];}focusedReferenceId=id;notify(id?"Focused reference updated":"Reference focus cleared");}catch(error){notify(errorMessage(error),"error");}}
  async function removeConversationReference(id:string){if(!selectedConversation)return;try{if(id===focusedReferenceId)await focusConversationReference(undefined);await api.setConversationReference(selectedConversation.id,id,false);activeReferenceIds=activeReferenceIds.filter(value=>value!==id);}catch(error){notify(errorMessage(error),"error");}}
  async function send(prompt:string){
    if(!selectedConversation||!workspace)throw new Error("Open a project chat before sending");
    const conversation=selectedConversation;const worktree=selectedWorktree;const style=effectiveStyle;const profile=generationProfile;const referenceIds=[...activeReferenceIds];
    if(runningRequests[conversation.id]){notify("This chat is already generating — open another chat to work in parallel","error");throw new Error("Chat already generating");}
    activityByConversation={...activityByConversation,[conversation.id]:[]};
    try{
      const context=buildChatContext({worktree,focused:references.find(reference=>reference.id===focusedReferenceId),selectedAsset,styleName:style.name,stylePrompt:style.prompt,customSkills});
      const command=inferChatCommand(prompt);
      const polishMode=resolvePolishMode(prompt,command,conversationAnimationMode);
      const outboundPrompt=(command==="animate"||/^\/animate\b/i.test(prompt.trim()))?applyAnimationPolishModeToPrompt(prompt,polishMode):prompt;
      const motion=extractAnimateMotion(prompt);
      const animateMaster=resolveAnimateMasterAsset(prompt,assets,selectedAsset,selectedAsset?.relativePath);
      if(isAnimateWithoutResolvableMaster(command,polishMode,prompt,assets,selectedAsset,selectedAsset?.relativePath)){
        notify("Select a sprite or include its path (Use assets/... as the exact source master) before /animate.","error");
        throw new Error("Animate requires a resolvable master asset");
      }
      if(shouldRunNativeRigFirst(command,polishMode,animateMaster)){
        if(!["ready","detected"].includes(currentProvider?.status??"")){notify("Open Settings to install or sign in to this chat's provider","error");throw new Error("Provider not ready");}
        const requestId=`native-rig-${Date.now()}`;
        const abortController=new AbortController();
        nativeRigAbortControllers={...nativeRigAbortControllers,[conversation.id]:abortController};
        runningRequests={...runningRequests,[conversation.id]:{id:requestId,conversationId:conversation.id,workspaceId:workspace.id,worktreeId:worktree?.id,prompt:outboundPrompt,command,generation:generationOptions(),knownPackIds:packs.map(pack=>pack.id),startedAt:Date.now(),polishMode,motion}};
        const native=await runNativeRigChatAnimation({
          conversation,workspaceId:workspace.id,worktree,prompt:outboundPrompt,asset:animateMaster!,motion,profile,
          providerId:conversation.provider,model:profile.model||undefined,reasoningEffort:profile.reasoningEffort||undefined,
          knownPackIds:packs.map(pack=>pack.id),currentAssets:assets,polishMode,signal:abortController.signal,
          onFitWarning:(message)=>{activityByConversation=appendConversationActivity(activityByConversation,conversation.id,[message],"warning");notify(message,"error");},
          onActivity:(line)=>{activityByConversation=appendConversationActivity(activityByConversation,conversation.id,[line]);},
        });
        const nextControllers={...nativeRigAbortControllers};delete nextControllers[conversation.id];nativeRigAbortControllers=nextControllers;
        generationManifest=await api.getGenerationManifest(workspace.id).catch(()=>null);
        if(selectedConversation?.id===conversation.id){
          messages=native.messages;
          assets=native.completion.assets;rigs=native.completion.rigs;
          if(native.completion.worktreeAssetIds)worktreeAssetIds=native.completion.worktreeAssetIds;
          if(native.completion.animations)animations=native.completion.animations;
          if(native.renamed){conversations=conversations.map(item=>item.id===conversation.id?native.renamed!:item);sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?native.renamed!:item);selectedConversation=native.renamed;}
          if(native.completion.handoff.kind==="animation"){
            const {animationId,rigId}=native.completion.handoff;
            const generatedAnimation=animations.find(animation=>animation.id===animationId);
            if(generatedAnimation){selectedAnimation=generatedAnimation;selectedAsset=undefined;viewedAsset=undefined;activeTab=polishMode==="ai-polish"?"chat":"animate";}
            if(rigId)selectedRigId=rigId;
          }
        }
        if(polishMode==="ai-polish"&&native.completion.handoff.kind==="animation"){
          const handoff=native.completion.handoff;
          const generatedAnimation=animations.find(animation=>animation.id===handoff.animationId)
            ??native.completion.animations?.find(animation=>animation.id===handoff.animationId);
          const frameAssets=generatedAnimation?animationFrameAssets(generatedAnimation,assets):native.completion.ordered;
          const animationForPolish=generatedAnimation??(frameAssets.length?{id:handoff.animationId,name:motion||"rig animation",workspaceId:workspace.id,worktreeId:worktree?.id,fps:profile.fps,looping:true,frames:frameAssets.map(asset=>({assetId:asset.id})),createdAt:new Date().toISOString(),updatedAt:new Date().toISOString()}:undefined);
          if(animationForPolish&&frameAssets.length){
            try{
              const polishContext=buildChatContext({worktree,focused:references.find(reference=>reference.id===focusedReferenceId),selectedAsset,styleName:style.name,stylePrompt:style.prompt,customSkills});
              const started=await startAiPolishHandoff({conversation,workspaceId:workspace.id,worktree,animation:animationForPolish,frameAssets,masterPath:animateMaster?.relativePath,profile,referenceIds,knownPackIds:packs.map(pack=>pack.id),motion,context:polishContext});
              runningRequests={...runningRequests,[conversation.id]:started.request};
              if(selectedConversation?.id===conversation.id){
                messages=started.messages;
                if(started.renamed){const renamed=started.renamed;conversations=conversations.map(item=>item.id===conversation.id?renamed:item);sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);selectedConversation=renamed;}
              }
              notify("Rough rig frames rendered — starting AI polish pass");
              return;
            }catch(error){
              runningRequests=clearRunningRequest(runningRequests,conversation.id,requestId);
              notify(`Rough frames rendered but AI polish could not start: ${errorMessage(error)}`,"error");
            }
          }else{
            runningRequests=clearRunningRequest(runningRequests,conversation.id,requestId);
            notify("Rough rig frames rendered but AI polish could not start — no frame assets were found.","error");
          }
        }else if(polishMode==="full-redraw"&&native.completion.handoff.kind==="animation"){
          try{
            const handoff=native.completion.handoff;
            const generatedAnimation=animations.find(animation=>animation.id===handoff.animationId)
              ??native.completion.animations?.find(animation=>animation.id===handoff.animationId);
            const frameAssets=generatedAnimation?animationFrameAssets(generatedAnimation,assets):native.completion.ordered;
            if(!generatedAnimation||!frameAssets.length){runningRequests=clearRunningRequest(runningRequests,conversation.id,requestId);notify("Rough rig frames rendered but full redraw could not start — no frame assets were found.","error");return;}
            const polishContext=buildChatContext({worktree,focused:references.find(reference=>reference.id===focusedReferenceId),selectedAsset,styleName:style.name,stylePrompt:style.prompt,customSkills});
            const redrawPrompt=buildFullRedrawPrompt(generatedAnimation,frameAssets,motion);
            const options=buildProviderOptions(profile,command,referenceIds);
            const started=await startChatRequest({conversation,workspaceId:workspace.id,worktree,prompt:redrawPrompt,context:polishContext,options,knownPackIds:packs.map(pack=>pack.id),polishMode,motion});
            runningRequests={...runningRequests,[conversation.id]:started.request};
            if(selectedConversation?.id===conversation.id){
              messages=started.messages;
              if(started.renamed){const renamed=started.renamed;conversations=conversations.map(item=>item.id===conversation.id?renamed:item);sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);selectedConversation=renamed;}
            }
            notify("Rough rig frames rendered — starting full redraw pass");
            return;
          }catch(error){
            runningRequests=clearRunningRequest(runningRequests,conversation.id,requestId);
            notify(`Rough frames rendered but full redraw could not start: ${errorMessage(error)}`,"error");
          }
        }else{
          runningRequests=clearRunningRequest(runningRequests,conversation.id,requestId);
        }
        if(shouldOrchestrateNativeRig(command,polishMode,animateMaster)){
          const qualityNotice=native.completion.handoff.kind==="animation"?await qualityNoticeForAnimation(native.completion.handoff.animationId):undefined;
          notify(qualityNotice??"Native rig animation rendered — open the Rig tab to refine poses",qualityNotice?"error":"notice");
        }
        return;
      }
      const options=buildProviderOptions(profile,command,referenceIds);
      if(needsNativeRigMasterPhase(prompt,command,animateMaster,conversationAnimationMode)){
        options.nativeRigMasterOnly=true;
        options.generation={...options.generation,frames:1,fps:1,frameMode:"fixed",minFrames:1,maxFrames:1};
      }
      const activity=chatActivityLines(command,prompt,options.generation);
      if(activity.length)activityByConversation=appendConversationActivity(activityByConversation,conversation.id,activity);
      const started=await startChatRequest({
        conversation,workspaceId:workspace.id,worktree,prompt:outboundPrompt,context,options,knownPackIds:packs.map(pack=>pack.id),
        polishMode,phase:options.nativeRigMasterOnly?"master":undefined,motion,
      });
      runningRequests={...runningRequests,[conversation.id]:started.request};
      if(selectedConversation?.id===conversation.id)messages=started.messages;
      if(started.renamed){const renamed=started.renamed;conversations=conversations.map(item=>item.id===conversation.id?renamed:item);sidebarConversations=sidebarConversations.map(item=>item.id===conversation.id?renamed:item);if(selectedConversation?.id===conversation.id)selectedConversation=renamed;}
    }catch(error){const next={...runningRequests};delete next[conversation.id];runningRequests=next;const nextControllers={...nativeRigAbortControllers};delete nextControllers[conversation.id];nativeRigAbortControllers=nextControllers;if(error instanceof DOMException&&error.name==="AbortError"){notify("Native rig orchestration cancelled");return;}activityByConversation=appendConversationActivity(activityByConversation,conversation.id,[errorMessage(error)],"error");notify(errorMessage(error),"error");throw error;}
  }
  async function cancel(){if(!selectedConversation)return;const request=runningRequests[selectedConversation.id];if(!request)return;if(request.id.startsWith("native-rig-")){nativeRigAbortControllers[selectedConversation.id]?.abort();const nextControllers={...nativeRigAbortControllers};delete nextControllers[selectedConversation.id];nativeRigAbortControllers=nextControllers;runningRequests=clearRunningRequest(runningRequests,selectedConversation.id,request.id);notify("Native rig orchestration cancelled");return;}try{await api.cancelProviderRequest(request.id);}catch(error){notify(errorMessage(error),"error");}}
  async function finalizeChatGeneration(request:ActiveChatRequest,response:string){
    if(!workspace||workspace.id!==request.workspaceId)return;
    const result=await completeChatGeneration(request,response,{assets,packs,rigs,selectedWorktreeId:selectedWorktree?.id,selectedConversationId:selectedConversation?.id,parallelGenerationActive:parallelGenerationsInWorkspace(runningRequests,workspace.id)});
    generationManifest=await api.getGenerationManifest(workspace.id).catch(()=>generationManifest);
    assets=result.assets;packs=result.packs;rigs=result.rigs;if(result.worktreeAssetIds)worktreeAssetIds=result.worktreeAssetIds;if(result.animations)animations=result.animations;
    const handoff=result.handoff;
    if(handoff.kind==="animation"){const generatedAnimation=animations.find(animation=>animation.id===handoff.animationId);if(generatedAnimation){selectedAnimation=generatedAnimation;selectedAsset=undefined;viewedAsset=undefined;activeTab="animate";}if(handoff.rigId)selectedRigId=handoff.rigId;if(result.polishWarning)notify(result.polishWarning,"error");const qualityNotice=handoff.animationId?await qualityNoticeForAnimation(handoff.animationId):undefined;if(qualityNotice)notify(qualityNotice,"error");}
    else if(handoff.kind==="sprite"){selectedAsset=handoff.asset;viewedAsset=undefined;activeTab="sprites";if(handoff.analyzeRig){void api.analyzeRigFit(result.ordered[0].id).then(report=>{const top=report.detections[0];if(top)notify(`Rig check: ${top.morphology} ${Math.round(top.confidence*100)}%${report.warnings.length?" (needs a cleaner master)":""} — open the Rig tab to animate it with points`);}).catch(()=>undefined);}}
    else if(handoff.kind==="unaccepted"){notify(unacceptedGenerationNotice(handoff.rejectedStaticAnimation),"error");}
  }
  async function reconcileGenerationManifest() {if(!workspace)return;const result=await reconcileManifestData(workspace.id,assets,animations,selectedWorktree?.id,activeWorktreeId(),await currentMotionPlan());if(!result)return;if(result.worktreeAssetIds)worktreeAssetIds=result.worktreeAssetIds;if(result.selectedAnimation)selectedAnimation=result.selectedAnimation;if(result.animations)animations=result.animations;}
  async function hydrateLatestGeneration() {if(!workspace||!selectedConversation||!messages.length)return;const next=await hydrateGenerationData(workspace.id,selectedConversation.id,messages,assets,animations);if(next)messages=next;}
  function editAssetFromChat(asset:Asset){selectedAsset=asset;viewedAsset=asset;activeTab="sprites";}
  function editAnimationFromChat(animation:Animation){selectedAnimation=animation;selectedAsset=undefined;activeTab="animate";}
  function prepareTemplateInChat(application:TemplateApplication){selectedAsset=application.targetAsset;chatDraft=application.prompt;activeTab="chat";}
  async function generateVfxFromStudio(prompt:string){if(!selectedConversation)throw new Error("Create a VFX chat before generating an effect");if(!["ready","detected"].includes(currentProvider?.status??""))throw new Error("Open Settings to install or sign in to this chat's provider");const conversationId=selectedConversation.id;activeTab="chat";await send(prompt);if(!runningRequests[conversationId])throw new Error("VFX generation could not start");}
  async function refreshVfxAssets(){if(!workspace||!selectedWorktree)return;const loaded=await refreshWorktreeMedia(workspace.id,activeWorktreeId(),selectedWorktree.id);assets=loaded.assets;worktreeAssetIds=loaded.worktreeAssetIds;animations=loaded.animations;selectedAnimation=animations[0];}
  function openVfxSheet(animation:Animation){selectedAnimation=animation;activeTab="sheets";}
  async function exportAnimationFromChat(animation:Animation){
    try {const result=await api.exportAnimation(animation.id);notify(`Exported ${result.width}×${result.height} spritesheet`);await revealItemInDir(result.pngPath);}
    catch(error){notify(errorMessage(error),"error");}
  }
  async function exportAssetFromChat(asset:Asset){
    try {const result=await api.exportAsset(asset.id);notify(`Exported ${result.width}×${result.height} sprite`);await revealItemInDir(result.pngPath);}
    catch(error){notify(errorMessage(error),"error");}
  }
  function selectTab(value:string){activeTab=value;}
  function selectPrimary(value:"chat"|"media"|"skills"|"arts"|"guide"){if(value==="chat"){activeTab="chat";return;}if(value==="media"){activeTab=mediaTabs.includes(activeTab)?activeTab:"sprites";return;}activeTab=value;}
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
  function rigAsset(asset:Asset,rigId?:string){const existing=rigId??rigs.find(rig=>rig.assetId===asset.id)?.id;rigDraftAssetId=asset.id;selectedRigId=existing;viewedAsset=undefined;motionAsset=undefined;activeTab="rig";notify(existing?`${asset.name} rig opened in the editor`:`${asset.name} is staged in the Rig editor — place points or ask the AI`);}
  async function rigRendered(animation:Animation,assetIds:string[]){
    if(!workspace)return;
    const loaded=await refreshWorktreeMedia(workspace.id,activeWorktreeId(),selectedWorktree?.id,assetIds);
    assets=loaded.assets;worktreeAssetIds=loaded.worktreeAssetIds;animations=loaded.animations;
    selectedAnimation=animation;selectedAsset=undefined;viewedAsset=undefined;activeTab="animate";
    void api.queueQualityAnalysis(animation.id).catch(()=>undefined);
    notify(`Rendered ${animation.frames.length} rig frames — quality analysis started`);
  }
  async function prepareMotionInChat(asset:Asset,motion:string){
    if(!selectedConversation)await newConversation();
    if(!selectedConversation){notify("Create a chat before preparing an animation","error");return;}
    try{await api.setSetting(`conversation-animation-mode:${selectedConversation.id}`,conversationAnimationMode);}catch(error){notify(errorMessage(error),"error");}
    selectedAsset=asset;viewedAsset=undefined;motionAsset=undefined;
    const polishMode=conversationAnimationMode;
    if(polishMode==="rig"){
      activeTab="chat";
      await send(buildMotionPrompt(asset,motion,polishMode,generationProfile));
      return;
    }
    activeTab="chat";
    chatDraft=buildMotionPrompt(asset,motion,polishMode,generationProfile);
    notify(`${asset.name} is attached — review the motion prompt and send when ready`);
  }
  async function prepareRigPolishInChat(animation:Animation,assetIds:string[]){
    if(!workspace){notify("Open a project before polishing","error");return;}
    if(!["ready","detected"].includes(currentProvider?.status??"")){notify("Open Settings to install or sign in to this chat's provider","error");return;}
    if(!selectedConversation)await newConversation();
    if(!selectedConversation){notify("Create a chat before polishing","error");return;}
    if(runningRequests[selectedConversation.id]){notify("This chat is already generating — wait for it to finish","error");return;}
    const loaded=await refreshWorktreeMedia(workspace.id,activeWorktreeId(),selectedWorktree?.id,assetIds);
    assets=loaded.assets;worktreeAssetIds=loaded.worktreeAssetIds;
    const frameAssets=animationFrameAssets(animation,assets);
    if(!frameAssets.length){notify("Render the rig before polishing it","error");return;}
    selectedAsset=frameAssets[0];viewedAsset=undefined;activeTab="chat";
    const style=effectiveStyle;
    const polishContext=buildChatContext({worktree:selectedWorktree,focused:references.find(reference=>reference.id===focusedReferenceId),selectedAsset:frameAssets[0],styleName:style.name,stylePrompt:style.prompt,customSkills});
    try{
      const started=await startAiPolishHandoff({conversation:selectedConversation,workspaceId:workspace.id,worktree:selectedWorktree,animation,frameAssets,masterPath:frameAssets[0]?.relativePath,profile:generationProfile,referenceIds:activeReferenceIds,knownPackIds:packs.map(pack=>pack.id),motion:animation.name,context:polishContext});
      runningRequests={...runningRequests,[selectedConversation.id]:started.request};
      messages=started.messages;
      if(started.renamed){const renamed=started.renamed;conversations=conversations.map(item=>item.id===selectedConversation!.id?renamed:item);sidebarConversations=sidebarConversations.map(item=>item.id===selectedConversation!.id?renamed:item);selectedConversation=renamed;}
      notify("AI polish pass started from rig frames");
    }catch(error){notify(errorMessage(error),"error");}
  }
  function updateAsset(asset:Asset){selectedAsset=asset;assets=assets.map(item=>item.id===asset.id?asset:item);}
  async function assetDeleted(){selectedAsset=undefined;if(workspace)assets=await api.scanAssets(workspace.id);}
  function applyTheme(value:string){theme=value;document.documentElement.dataset.theme=value;}
  async function changeTheme(value:string){applyTheme(value);try{await api.setSetting("theme",value);}catch(error){notify(errorMessage(error),"error");}}
  async function changeWorkspaceStyle(value:StylePresetId){if(!workspace){notify("Open a project before choosing its art direction","error");return;}workspaceStyle=value;try{await api.setSetting(`workspace-style:${workspace.id}`,value);notify(`${stylePreset(value,customArts).name} is now the project art direction`);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationStyle(value:ConversationStyleId){if(!selectedConversation)return;conversationStyle=value;try{await api.setSetting(`conversation-style:${selectedConversation.id}`,value);notify(value==="inherit"?"Chat now follows the project art direction":`${stylePreset(value,customArts).name} applied to this chat`);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationAnimationMode(value:AnimationPolishMode){conversationAnimationMode=value;if(!selectedConversation)return;try{await api.setSetting(`conversation-animation-mode:${selectedConversation.id}`,value);notify(`${animationPolishModeOption(value).label} selected for animations in this chat`);}catch(error){notify(errorMessage(error),"error");}}
  async function saveCustomSkills(value:CustomSkill[]){customSkills=value;try{await api.setSetting("custom-skills",value);notify("Skills library saved");}catch(error){notify(errorMessage(error),"error");}}
  async function saveCustomArts(value:CustomArtStyle[]){customArts=value;try{await api.setSetting("custom-arts",value);notify("Arts library saved");}catch(error){notify(errorMessage(error),"error");}}
  async function changeGenerationProfile(value:ChatGenerationProfile){if(!selectedConversation)return;generationProfile=normalizeGenerationProfile(value,currentProvider?.modes??[],selectedConversation.provider);try{await api.setSetting(`conversation-generation:${selectedConversation.id}`,generationProfile);}catch(error){notify(errorMessage(error),"error");}}
  async function changeConversationProvider(providerId:string){
    if(!selectedConversation)return;
    if(runningRequests[selectedConversation.id]){notify("Stop this chat’s generation before switching providers","error");return;}
    if(selectedConversation.provider===providerId)return;
    try{
      const changed=await api.switchConversationProvider(selectedConversation.id,providerId);
      const nextProvider=providers.find(provider=>provider.id===providerId);
      const nextProfile=normalizeGenerationProfile({...generationProfile,model:"",reasoningEffort:"",imageProviderId:defaultImageProviderId(providerId)},nextProvider?.modes??[],providerId);
      selectedConversation=changed;conversations=conversations.map(item=>item.id===changed.id?changed:item);sidebarConversations=sidebarConversations.map(item=>item.id===changed.id?changed:item);
      generationProfile=nextProfile;await api.setSetting(`conversation-generation:${changed.id}`,nextProfile);
      notify(`${nextProvider?.name??"Provider"} selected. This starts a new provider session; chat history remains visible.`);
    }catch(error){notify(errorMessage(error),"error");}
  }
  async function refreshProviders(){try{providers=await api.detectProviders();notify("Provider detection refreshed");}catch(error){notify(errorMessage(error),"error");}}
  async function installAgentProvider(providerId:string){try{const result=await api.installAgentProvider(providerId);providers=await api.detectProviders();notify(result.detail);}catch(error){notify(errorMessage(error),"error");throw error;}}
  async function authenticateAgentProvider(providerId:string){try{const result=await api.authenticateAgentProvider(providerId);providers=await api.detectProviders();notify(result.detail);}catch(error){notify(errorMessage(error),"error");throw error;}}
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
      if(payload.eventType==="activity"||payload.eventType==="started"){activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,[payload.content]);}
      if(payload.eventType==="content"&&selected){const next=appendAssistantDelta(messages,payload.content);if(next)messages=next;}
      if(isTerminalProviderEvent(payload.eventType)){
        const request=runningRequests[payload.conversationId];
        let deferProviderClear=false;
        if(payload.eventType==="completed"&&request){
          activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,["Registering the generated sprite"]);
          try{
            if(request.phase==="master"){
              activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,["Master saved — building native rig animation"]);
              const originatingConversation=conversations.find(item=>item.id===payload.conversationId)
                ??sidebarConversations.find(item=>item.id===payload.conversationId)
                ??{id:payload.conversationId,provider:defaultProvider} as Conversation;
              const requestWorktree=request.worktreeId?worktrees.find(item=>item.id===request.worktreeId):selectedWorktree;
              const selected=originatingConversation.id===selectedConversation?.id;
              const style=selected?effectiveStyle:stylePreset(workspaceStyle,customArts);
              const masterProfile=selected?generationProfile:{
                profileVersion:8,quality:request.generation.quality,width:request.generation.width,height:request.generation.height,
                frames:request.generation.frames,fps:request.generation.fps,frameMode:request.generation.frameMode,
                minFrames:request.generation.minFrames,maxFrames:request.generation.maxFrames,
                allowInterpolation:request.generation.allowInterpolation,allowAutoAdjust:request.generation.allowAutoAdjust,
                model:"",reasoningEffort:"",imageProviderId:defaultImageProviderId(originatingConversation.provider??defaultProvider),
              };
              const nativeRequestId=`native-rig-${Date.now()}`;
              const abortController=new AbortController();
              nativeRigAbortControllers={...nativeRigAbortControllers,[payload.conversationId]:abortController};
              runningRequests={...runningRequests,[payload.conversationId]:{...request,id:nativeRequestId}};
              const parallelActive=parallelGenerationsInWorkspace(runningRequests,request.workspaceId);
              let continued;
              try{
                continued=await continueAfterMasterProviderPhase({
                  prior:request,
                  conversation:originatingConversation,
                  providerId:originatingConversation.provider??defaultProvider,
                  worktree:requestWorktree,
                  profile:masterProfile,
                  referenceIds:selected?activeReferenceIds:[],
                  context:buildChatContext({worktree:requestWorktree,focused:selected?references.find(reference=>reference.id===focusedReferenceId):undefined,selectedAsset:selected?selectedAsset:undefined,styleName:style.name,stylePrompt:style.prompt,customSkills}),
                  knownPackIds:packs.map(pack=>pack.id),
                  currentAssets:assets,
                  onActivity:(line)=>{activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,[line]);},
                  onFitWarning:(message)=>{activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,[message],"warning");notify(message,"error");},
                  signal:abortController.signal,
                  masterResponse:payload.content,
                  parallelGenerationActive:parallelActive,
                });
              }catch(error){
                const nextControllers={...nativeRigAbortControllers};delete nextControllers[payload.conversationId];nativeRigAbortControllers=nextControllers;
                runningRequests=clearRunningRequest(runningRequests,payload.conversationId,nativeRequestId);
                if(error instanceof DOMException&&error.name==="AbortError"){notify("Native rig orchestration cancelled");return;}
                throw error;
              }
              const nextControllers={...nativeRigAbortControllers};delete nextControllers[payload.conversationId];nativeRigAbortControllers=nextControllers;
              if(continued.kind==="handoff-started"){
                deferProviderClear=true;
                runningRequests={...runningRequests,[payload.conversationId]:continued.request};
                conversations=continued.renamed?conversations.map(item=>item.id===payload.conversationId?continued.renamed!:item):conversations;
                sidebarConversations=continued.renamed?sidebarConversations.map(item=>item.id===payload.conversationId?continued.renamed!:item):sidebarConversations;
              }
              if(selected){
                if(continued.kind==="rig-complete"){
                  messages=continued.messages;assets=continued.completion.assets;rigs=continued.completion.rigs;
                  if(continued.completion.worktreeAssetIds)worktreeAssetIds=continued.completion.worktreeAssetIds;
                  if(continued.completion.animations)animations=continued.completion.animations;
                  if(workspace)generationManifest=await api.getGenerationManifest(workspace.id).catch(()=>generationManifest);
                  if(continued.completion.handoff.kind==="animation"){
                    const {animationId,rigId}=continued.completion.handoff;
                    const generatedAnimation=animations.find(animation=>animation.id===animationId);
                    if(generatedAnimation){selectedAnimation=generatedAnimation;activeTab=request.polishMode==="ai-polish"?"chat":"animate";}
                    if(rigId)selectedRigId=rigId;
                    const qualityNotice=await qualityNoticeForAnimation(animationId);
                    if(qualityNotice)notify(qualityNotice,"error");
                  }
                }else if(continued.kind==="handoff-started"){
                  messages=continued.messages;
                  if(continued.renamed)selectedConversation=continued.renamed;
                  notify(request.polishMode==="full-redraw"?"Rough rig frames rendered — starting full redraw pass":"Rough rig frames rendered — starting AI polish pass");
                }else{
                  notify("Master saved, but native rig animation could not start automatically. Select the master and run /animate again.","error");
                }
              }
              if(!deferProviderClear)runningRequests=clearRunningRequest(runningRequests,payload.conversationId,nativeRequestId);
            }else{
              await finalizeChatGeneration(request,payload.content);
            }
          }
          catch(error){notify(`The provider finished, but the sprite preview could not be refreshed: ${errorMessage(error)}`,"error");}
        }
        if(!deferProviderClear)runningRequests=clearRunningRequest(runningRequests,payload.conversationId,payload.requestId);
        if(payload.conversationId===selectedConversation?.id)messages=await api.listMessages(payload.conversationId);
        if(payload.eventType==="failed"){
          activityByConversation=appendConversationActivity(activityByConversation,payload.conversationId,[payload.content],"error");
          notify(payload.content,"error");
        }
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
        {#if activeTab==="guide"}<UserGuide/>
        {:else if activeTab==="skills"}<SkillsLibrary skills={customSkills} onChange={saveCustomSkills}/>
        {:else if activeTab==="arts"}<ArtsLibrary arts={customArts} selected={workspaceStyle} onChange={saveCustomArts} onSelect={changeWorkspaceStyle}/>
        {:else if !workspace}<NoProjectView onAdd={()=>projectDialogOpen=true}/>
        {:else if activeTab==="chat"}
          {#key selectedConversation?.id}<ConversationView conversation={selectedConversation} {messages} provider={currentProvider} availableProviders={providers} {imageProviders} customStyles={customArts} runningRequestId={currentRequest?.id} generationStartedAt={currentRequest?.startedAt} generationRequest={currentRequest} activity={currentActivity} {selectedAsset} {assets} {animations} packs={visiblePacks} {references} {activeReferenceIds} {focusedReferenceId} draftPrompt={chatDraft} onDraftConsumed={()=>chatDraft=""} workspacePath={workspace.path} {workspaceStyle} {conversationStyle} animationMode={conversationAnimationMode} {generationProfile} onSend={send} onCancel={cancel} onClearAsset={()=>selectedAsset=undefined} onEditAsset={editAssetFromChat} onEditAnimation={editAnimationFromChat} onViewPack={openPackFromChat} onExportAsset={exportAssetFromChat} onExportAnimation={exportAnimationFromChat} onConversationStyle={changeConversationStyle} onAnimationMode={changeConversationAnimationMode} onGenerationProfile={changeGenerationProfile} onProviderSwitch={changeConversationProvider} onAttachReferencePaths={attachReferencePaths} onAttachReferenceFiles={attachReferenceFiles} onFocusReference={focusConversationReference} onRemoveReference={removeConversationReference} onLinkError={(message)=>notify(message,"error")}/>{/key}
        {:else}
          <div class="media-view"><MediaNavigation active={activeTab} counts={{sprites:spriteCount,references:references.length,animate:animations.length,rigs:rigs.length,packs:visiblePacks.length,sessions:sessionCount}} onSelect={selectTab}/><div class="media-content">
            {#if activeTab==="sprites"}<AssetBrowser workspaceId={workspace.id} worktreeId={selectedWorktree?.id} assets={visibleAssets} {animations} packs={visiblePacks} packId={packFilter} selectedAssetId={selectedAsset?.id} onAssets={(value)=>assets=value} onSelect={selectAsset} onOpen={openSpriteGroup} onPack={(value)=>packFilter=value} onLinked={async()=>{if(selectedWorktree)worktreeAssetIds=await api.listWorktreeAssetIds(selectedWorktree.id)}} onError={(message)=>notify(message,"error")}/>
            {:else if activeTab==="references"&&selectedWorktree}<ReferenceLibrary worktreeId={selectedWorktree.id} conversationId={selectedConversation?.id} {references} activeIds={activeReferenceIds} maximumActive={currentProvider?.capabilities.maximumReferenceImages ?? 0} onReferences={(value)=>references=value} onActiveIds={(value)=>activeReferenceIds=value} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="animate"}<AnimationEditor workspaceId={workspace.id} workspacePath={workspace.path} worktreeId={activeWorktreeId()} conversationId={selectedConversation?.id} assets={visibleAssets} {animations} templates={animationTemplates} {selectedAnimation} linkedRigId={selectedRigId} active onAnimations={(value)=>animations=value} onAssetsRefresh={async()=>{if(workspace)assets=await api.scanAssets(workspace.id);}} onTemplates={(value)=>animationTemplates=value} onSelected={(value)=>selectedAnimation=value} onOpenRig={(rigId)=>{selectedRigId=rigId;activeTab="rig";}} onTemplateApplication={prepareTemplateInChat} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="rig"}<RigEditor workspaceId={workspace.id} worktreeId={activeWorktreeId()} assets={assets} {rigs} {providers} {selectedRigId} linkedAnimationNameForRig={linkedAnimationNameForRig} linkedAnimationIdForRig={linkedAnimationIdForRig} initialAssetId={rigDraftAssetId} onRigs={(value)=>rigs=value} onSelected={(id)=>{selectedRigId=id;rigDraftAssetId=undefined;}} onRendered={rigRendered} onPolish={prepareRigPolishInChat} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="sessions"&&workspace}<ProductionSessionsGallery workspaceId={workspace.id} worktreeId={activeWorktreeId()} {animations} onOpenAnimation={(animationId)=>{selectedAnimation=animations.find(animation=>animation.id===animationId);activeTab="animate";}} onError={(message)=>notify(message,"error")}/>
            {:else if activeTab==="terrain"&&selectedWorktree}<TerrainStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} assets={visibleAssets} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="vfx"&&selectedWorktree}<VfxStudio workspaceId={workspace.id} worktreeId={selectedWorktree.id} {animations} assets={visibleAssets} active onCreated={refreshVfxAssets} onOpenSheets={openVfxSheet} onGenerate={generateVfxFromStudio} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="sheets"}<SpriteSheetStudio workspaceId={workspace.id} worktreeId={activeWorktreeId()} {animations} assets={visibleAssets} active onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message)}/>
            {:else if activeTab==="packs"}<PackLibrary packs={visiblePacks} assets={visibleAssets} {selectedPackId} onView={viewPack} onBack={()=>selectedPackId=""} onOpen={openPackAsset}/>
            {:else if activeTab==="play"}<TestRoom {animations} assets={visibleAssets} {selectedAnimation} active/>{/if}
          </div></div>
        {/if}
      </div>
    </section>
    {#if selectedAsset && activeTab==="sprites" && workspace}<AssetInspector asset={selectedAsset} workspaceId={workspace.id} {animations} onClose={()=>selectedAsset=undefined} onChanged={updateAsset} onDeleted={assetDeleted} onError={(message)=>notify(message,"error")} onNotice={(message)=>notify(message,"notice")}/>{/if}
  </main>
{/if}

{#if projectDialogOpen}<ProjectDialog onCreated={acceptWorkspace} onClose={()=>projectDialogOpen=false} onError={(message)=>notify(message,"error")}/>{/if}
{#if settingsOpen}<SettingsModal {providers} {defaultProvider} {theme} {workspaceStyle} customStyles={customArts} onDefaultProvider={changeDefaultProvider} onTheme={changeTheme} onWorkspaceStyle={changeWorkspaceStyle} onRefresh={refreshProviders} onInstallAgentProvider={installAgentProvider} onAuthenticateAgentProvider={authenticateAgentProvider} onSaveImageProvider={saveImageProvider} onDeleteImageProvider={deleteImageProvider} onTestImageProvider={testImageProvider} onClose={()=>settingsOpen=false}/>{/if}
{#if worktreeDialog}<WorktreeDialog busy={creatingWorktree} onCreate={createWorktree} onClose={()=>worktreeDialog=false}/>{/if}
{#if viewedAsset}<SpriteViewer asset={viewedAsset} onAnimate={animateViewedAsset} onDownload={exportAssetFromChat} onClose={()=>viewedAsset=undefined}/>{/if}
{#if motionAsset}<MotionPromptDialog asset={motionAsset} animationMode={conversationAnimationMode} onAnimationMode={changeConversationAnimationMode} onContinue={(motion)=>prepareMotionInChat(motionAsset!,motion)} onRig={()=>rigAsset(motionAsset!,rigs.find(rig=>rig.assetId===motionAsset!.id)?.id)} onClose={()=>motionAsset=undefined}/>{/if}
{#if workspaceMenu && workspace}<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&(workspaceMenu=false)}><div class="workspace-dialog"><header><div><p>PROJECT</p><h2>Manage {workspace.name}</h2></div><button onclick={()=>workspaceMenu=false}><X size={15}/></button></header><label>Name<div><input bind:value={renameValue}/><button onclick={renameWorkspace}><Pencil size={12}/>Rename</button></div></label><p class="path">{displayPath(workspace.path)}</p><div class="backup-actions"><button onclick={createBackup} disabled={backupBusy}><Archive size={13}/>{backupBusy?"Working…":"Create backup…"}</button><button onclick={restoreBackup} disabled={backupBusy}><RotateCcw size={13}/>Restore backup…</button></div><div class="danger-actions"><button onclick={()=>removeWorkspace(false)}><FolderMinus size={13}/>Remove from app</button><button onclick={()=>removeWorkspace(true)}><Trash2 size={13}/>Delete files…</button></div></div></div>{/if}
{#if toast}<div class="toast" class:error={toast.kind==="error"}><span></span><p>{toast.message}</p><button onclick={()=>toast=undefined}><X size={12}/></button></div>{/if}

<style>
  .studio{width:100vw;height:100vh;display:flex;background:var(--bg);color:var(--text)}.main-pane{flex:1;min-width:0;height:100%;position:relative}.tab-stack{position:absolute;inset:0;overflow:hidden}.media-view{height:100%;display:grid;grid-template-rows:58px minmax(0,1fr)}.media-content{min-height:0;position:relative}.media-content>:global(*){max-width:100%}
  .boot{width:100vw;height:100vh;background:#090a0a;display:flex;flex-direction:column;align-items:center;justify-content:center;color:#707174}.boot-mark{--logo-pixel:#f4f4f5;width:46px;height:46px;border:1px solid #3b3c3d;border-radius:10px;display:grid;place-items:center;color:#f5a524;background:#171818}.boot>span{width:90px;height:1px;background:#282929;position:relative;margin-top:22px;overflow:hidden}.boot>span:after{content:"";position:absolute;width:35px;height:1px;background:#8b5cf6;animation:load 1.2s ease-in-out infinite}.boot p{font-size:12px;letter-spacing:.06em;margin-top:13px}@keyframes load{from{left:-35px}to{left:90px}}
  .toast{position:fixed;right:16px;bottom:16px;z-index:70;min-width:260px;max-width:420px;min-height:40px;background:var(--surface);border:1px solid var(--border-strong);box-shadow:0 12px 34px #0006;border-radius:6px;display:grid;grid-template-columns:7px minmax(0,1fr) 24px;align-items:center;padding:0 7px}.toast>span{width:6px;height:6px;border-radius:50%;background:#5cad7b}.toast.error>span{background:#d16f69}.toast p{font-size:12px;line-height:1.4;margin:10px 8px;color:var(--text)}.toast button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}
  .backdrop{position:fixed;inset:0;background:#0009;display:grid;place-items:center;z-index:45}.workspace-dialog{width:min(430px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:8px;box-shadow:0 24px 70px #0009;padding:20px}.workspace-dialog header{display:flex;align-items:flex-start;justify-content:space-between}.workspace-dialog header p{font-size:10px;letter-spacing:.14em;color:var(--faint);font-weight:700;margin:0 0 7px}.workspace-dialog h2{font-size:16px;margin:0}.workspace-dialog header button{border:0;background:transparent;color:var(--faint);width:26px;height:26px;display:grid;place-items:center;cursor:pointer}.workspace-dialog label{font-size:11px;color:var(--muted);display:block;margin-top:24px}.workspace-dialog label>div{display:flex;gap:6px;margin-top:7px}.workspace-dialog input{height:31px;min-width:0;flex:1;background:var(--bg);border:1px solid var(--border-strong);border-radius:4px;color:var(--text);padding:0 8px;font-size:12px;outline:0}.workspace-dialog label button,.backup-actions button,.danger-actions button{height:31px;border:1px solid var(--border);background:var(--bg);color:var(--muted);border-radius:4px;display:flex;align-items:center;gap:6px;padding:0 9px;font:inherit;font-size:11px;cursor:pointer}.workspace-dialog .path{font-size:10px;color:var(--faint);overflow-wrap:anywhere;margin:10px 0 16px}.backup-actions{display:flex;gap:7px;padding-bottom:15px}.backup-actions button{flex:1;justify-content:center}.backup-actions button:disabled{opacity:.5;cursor:not-allowed}.danger-actions{border-top:1px solid var(--border);padding-top:15px;display:flex;justify-content:space-between}.danger-actions button:last-child{color:#cf7772}
</style>
