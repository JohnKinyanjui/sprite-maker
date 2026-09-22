<script lang="ts">
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { ArrowUp, Bot, Square, Terminal, Paperclip, AlertTriangle, Check, X, WandSparkles, Clapperboard, Image, UserRound, Zap, BookImage, Crosshair, Unlock, Boxes, Languages, FileDown } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import SpriteArtifactCard from "$lib/components/SpriteArtifactCard.svelte";
  import PackArtifactCard from "$lib/components/PackArtifactCard.svelte";
  import { contentWithoutSpriteOutputLinks, inferMessageGeneration, inferMessagePack, reportsGenerationFailure, reportsGenerationWarning } from "$lib/message-generations";
  import ActiveSkillsIndicator from "$lib/components/ActiveSkillsIndicator.svelte";
  import ChatSettingsMenu from "$lib/components/ChatSettingsMenu.svelte";
  import MarkdownMessage from "$lib/components/MarkdownMessage.svelte";
  import ProviderLogo from "$lib/components/ProviderLogo.svelte";
  import { SLASH_COMMANDS } from "$lib/generation-profiles";
  import { type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import type { CustomArtStyle, CustomSkill } from "$lib/library-types";
  import { errorMessage, studioErrorCode, type Animation, type AnimationPolishMode, type Asset, type AssetPack, type ChatGenerationProfile, type Conversation, type Message, type ProviderStatus, type ReferenceImage, type SpriteGenerationMetadata } from "$lib/types";
  import { projectNameFromPath } from "$lib/display-path";
  import { formatElapsedDuration } from "$lib/elapsed-duration";
  import type { ActiveChatRequest } from "$lib/chat-generation-finalize";
  import type { GenerationActivityEntry } from "$lib/generation-status";
  import { latestGenerationIssue, resolveGenerationProgress } from "$lib/generation-status";

  let { conversation, messages, provider, availableProviders=[], imageProviders=[], customStyles=[], customSkills=[], runningRequestId, generationStartedAt, generationRequest, activity, selectedAsset, assets, animations, packs, references, activeReferenceIds, focusedReferenceId, draftPrompt="", workspacePath, workspaceStyle, conversationStyle, animationMode, generationProfile, onSend, onCancel, onClearAsset, onEditAsset, onEditAnimation, onViewPack, onExportAsset, onExportAnimation, onConversationStyle, onAnimationMode, onGenerationProfile, onProviderSwitch, onAttachReferencePaths, onAttachReferenceFiles, onFocusReference, onRemoveReference, onDraftConsumed, onLinkError, onRefinePrompt, onCancelRefine, onOpenSkills, onExportDebugLog }: {
    conversation?: Conversation; messages: Message[]; provider?: ProviderStatus; availableProviders?:ProviderStatus[]; imageProviders?:ProviderStatus[]; runningRequestId?: string; generationStartedAt?: number; generationRequest?: ActiveChatRequest; activity: GenerationActivityEntry[]; selectedAsset?: Asset; assets: Asset[]; animations: Animation[]; packs: AssetPack[];
    references: ReferenceImage[]; activeReferenceIds: string[]; focusedReferenceId?: string;
    draftPrompt?: string; workspacePath: string;
    workspaceStyle: StylePresetId; conversationStyle: ConversationStyleId; animationMode: AnimationPolishMode; generationProfile: ChatGenerationProfile; customStyles?:CustomArtStyle[]; customSkills?: CustomSkill[];
    onSend: (prompt: string) => Promise<void>; onCancel: () => void; onClearAsset: () => void; onEditAsset: (asset: Asset) => void; onEditAnimation: (animation: Animation) => void; onViewPack: (pack: AssetPack) => void; onExportAsset: (asset: Asset) => Promise<void>; onExportAnimation: (animation: Animation) => Promise<void>; onConversationStyle: (style: ConversationStyleId) => void | Promise<void>;
    onAnimationMode: (mode: AnimationPolishMode) => void | Promise<void>;
    onGenerationProfile: (profile: ChatGenerationProfile) => void | Promise<void>; onProviderSwitch: (providerId: string) => void | Promise<void>;
    onAttachReferencePaths: (paths: string[]) => Promise<void>; onAttachReferenceFiles: (files: File[]) => Promise<void>; onFocusReference: (id?: string) => Promise<void>; onRemoveReference: (id: string) => Promise<void>;
    onDraftConsumed: () => void; onLinkError: (message: string) => void;
    onRefinePrompt: (draft: string) => Promise<string>;
    onCancelRefine: (conversationId: string) => void | Promise<void>;
    onOpenSkills: () => void;
    onExportDebugLog: (path: string) => void | Promise<void>;
  } = $props();
  let prompt = $state("");
  let sending = $state(false);
  let attaching = $state(false);
  let refining = $state(false);
  let refineGeneration = $state(0);
  let exportingLog = $state(false);
  let localStartedAt = $state<number | undefined>();
  let elapsedNow = $state(Date.now());
  let textarea = $state<HTMLTextAreaElement>();
  let messagePane = $state<HTMLDivElement>();
  let slashQuery = $derived(prompt.startsWith("/") && !prompt.slice(1).includes(" ") ? prompt.slice(1).toLowerCase() : undefined);
  let matchingCommands = $derived(slashQuery === undefined ? [] : SLASH_COMMANDS.filter(command => command.label.slice(1).startsWith(slashQuery)));
  let activeReferences = $derived(references.filter(reference => activeReferenceIds.includes(reference.id)));
  let latestActivity = $derived(activity.at(-1)?.text ?? "Preparing the generation pipeline");
  let generationProgress = $derived(resolveGenerationProgress(generationRequest, activity, sending));
  let generationIssue = $derived(latestGenerationIssue(activity));
  let projectName = $derived(projectNameFromPath(workspacePath));
  let runningAssistantMessage = $derived(
    [...messages].reverse().find(message => message.role === "assistant" && message.status === "running"),
  );
  let generationActive = $derived(Boolean(sending || runningRequestId || runningAssistantMessage));
  let effectiveStartedAt = $derived(
    generationStartedAt
      ?? localStartedAt
      ?? (runningAssistantMessage ? Date.parse(runningAssistantMessage.createdAt) : undefined),
  );
  let elapsedLabel = $derived(
    effectiveStartedAt && generationActive
      ? formatElapsedDuration(elapsedNow - effectiveStartedAt)
      : "",
  );
  let canStopGeneration = $derived(Boolean(runningRequestId));

  $effect(() => {
    if (!generationActive) {
      localStartedAt = undefined;
      return;
    }
    elapsedNow = Date.now();
    const interval = window.setInterval(() => {
      elapsedNow = Date.now();
    }, 1000);
    return () => window.clearInterval(interval);
  });

  $effect(()=>{if(draftPrompt){prompt=draftPrompt;onDraftConsumed();requestAnimationFrame(()=>textarea?.focus());}});
  $effect(()=>{
    conversation?.id;
    messages.length;
    messages.at(-1)?.content;
    messages.at(-1)?.status;
    activity.length;
    requestAnimationFrame(()=>{if(messagePane)messagePane.scrollTop=messagePane.scrollHeight;});
  });

  async function send() {
    if (!prompt.trim() || sending || refining || runningRequestId) return;
    const value = prompt;
    prompt = "";
    localStartedAt = Date.now();
    sending = true;
    try { await onSend(value); }
    catch { prompt = value; }
    finally { sending = false; }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape" && matchingCommands.length) { prompt = ""; return; }
    if (event.key === "Enter" && !event.shiftKey && !refining) { event.preventDefault(); send(); }
  }

  async function refinePrompt() {
    if (!conversation) return;
    if (refining) {
      const generation = refineGeneration;
      try {
        await onCancelRefine(conversation.id);
      } catch (error) {
        const code = studioErrorCode(error);
        if (code !== "refine_not_running") {
          onLinkError(errorMessage(error));
          return;
        }
        if (refineGeneration !== generation) return;
        refineGeneration += 1;
      }
      if (refineGeneration === generation) {
        refining = false;
      }
      return;
    }
    const draft = prompt.trim();
    if (!draft || sending) return;
    const generation = ++refineGeneration;
    refining = true;
    try {
      const refined = await onRefinePrompt(draft);
      if (refineGeneration !== generation) return;
      prompt = refined;
      requestAnimationFrame(() => textarea?.focus());
    } catch (error) {
      if (refineGeneration !== generation) return;
      if (studioErrorCode(error) === "request_cancelled") return;
      onLinkError(errorMessage(error));
    } finally {
      if (refineGeneration === generation) {
        refining = false;
      }
    }
  }

  async function uploadReferences() {
    if (!conversation || attaching) return;
    const selected = await open({multiple:true,directory:false,title:"Attach reference images",filters:[{name:"Images",extensions:["png","jpg","jpeg","webp","gif"]}]});
    const paths = typeof selected === "string" ? [selected] : selected ?? [];
    if (!paths.length) return;
    attaching = true;
    try { await onAttachReferencePaths(paths); } finally { attaching = false; }
  }

  async function paste(event: ClipboardEvent) {
    if (refining) return;
    const files = Array.from(event.clipboardData?.items ?? [])
      .filter(item => item.kind === "file" && item.type.startsWith("image/"))
      .map(item => item.getAsFile())
      .filter((file): file is File => Boolean(file));
    if (!files.length) return;
    event.preventDefault();
    attaching = true;
    try { await onAttachReferenceFiles(files); } finally { attaching = false; }
  }

  function chooseCommand(label: string) {
    if (refining) return;
    prompt = `${label} `;
    requestAnimationFrame(() => textarea?.focus());
  }

  async function exportDebugLog() {
    if (!conversation || exportingLog) return;
    const destination = await save({
      title: "Save chat debug log",
      defaultPath: `${conversation.title.replace(/[^\w.-]+/g, "_")}-debug.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!destination) return;
    exportingLog = true;
    try {
      const path = await api.exportConversationDebugLog({
        conversationId: conversation.id,
        destinationPath: destination,
      });
      await onExportDebugLog(path);
    } catch (error) {
      onLinkError(errorMessage(error));
    } finally {
      exportingLog = false;
    }
  }

  function commandIcon(id: string) {
    return id === "animate" ? Clapperboard : id === "sprite" ? Image : id === "character" ? UserRound : id === "pack" ? Boxes : Zap;
  }

  function generationFor(message: Message): SpriteGenerationMetadata | undefined {
    return inferMessageGeneration(message, assets, animations);
  }

  function visibleContent(message: Message, hasArtifact: boolean): string {
    if (!hasArtifact) return message.content;
    return contentWithoutSpriteOutputLinks(message.content);
  }

  function readableContent(message: Message, hasArtifact: boolean): string {
    const value = visibleContent(message, hasArtifact);
    if (message.role !== "assistant" || !["claude", "gemini", "grok", "cursor", "antigravity"].includes(provider?.id ?? "")) return value;
    const lines = value.split("\n");
    if (lines.length < 10 || lines.some(line => !line.trim() || /^\s*(?:[-*+]|\d+\.|#{1,6}|>|```)/.test(line))) return value;
    const shortLines = lines.filter(line => line.trim().split(/\s+/).length <= 3).length;
    if (shortLines / lines.length < 0.8) return value;
    return lines.map(line => line.trim()).join(" ").replace(/\s+([,.;!?])/g, "$1");
  }
</script>

<section class="conversation-view">
  <header>
    <div class="chat-heading">{#if provider}<span class={`provider-mark ${provider.id}`}><ProviderLogo providerId={provider.id} label={provider.name} size={18}/></span>{/if}<div><h1>{conversation?.title ?? "Agent"}</h1><p>{provider?.name ?? "No provider selected"} · {provider?.status === "ready" ? "Ready" : provider?.status === "detected" ? "Authentication checked on first request" : "Unavailable"}</p></div></div>
    <div class="header-actions">
      {#if conversation}<button class="log-export" onclick={exportDebugLog} disabled={exportingLog} title="Download full chat debug log (.md)"><FileDown size={14}/><span>{exportingLog ? "Exporting…" : "Debug log"}</span></button>{/if}
      <div class:ready={["ready","detected"].includes(provider?.status ?? "")} class="status-dot"><span></span>{provider?.status === "ready" ? "Connected" : provider?.status === "detected" ? "Detected" : "Offline"}</div>
    </div>
  </header>

  <div class="messages" bind:this={messagePane} aria-live="polite">
    {#if !conversation}
      <div class="blank"><Bot size={29} strokeWidth={1.35} /><h2>Start a project conversation</h2><p>Create a conversation to work with an installed AI agent in this project folder.</p></div>
    {:else if !messages.length}
      <div class="blank"><span class={`provider-mark hero monogram ${provider?.id ?? ""}`}><ProviderLogo providerId={provider?.id ?? "agent"} label={provider?.name} size={48}/></span><h2>Ready to make something?</h2><p>Describe a sprite, animation, or effect. {provider?.name ?? "Your selected provider"} will work directly in <strong>{projectName}</strong>.</p><div class="suggestions"><button onclick={() => prompt = "Generate a polished 4-frame 32x32 pixel-art blue knight idle animation with a transparent background."}><WandSparkles size={17}/><span><strong>Create sprites</strong><small>Characters, enemies, items, UI, and more.</small></span></button><button onclick={() => prompt = "/animate Create a smooth 6-frame run cycle for a 32x32 pixel-art forest ranger."}><Clapperboard size={17}/><span><strong>Animate</strong><small>Idle, run, attack, effects, or full loops.</small></span></button><button onclick={() => prompt = "Create a coordinated set of health, mana, and stamina potion sprites for a pixel RPG."}><Boxes size={17}/><span><strong>Build a pack</strong><small>Design a matching set for your game.</small></span></button></div><div class="blank-tip"><Paperclip size={13}/> Attach a sketch or reference image to guide the result.</div></div>
    {:else}
      <div class="message-column">
        {#each messages as message}
          {@const packResult = inferMessagePack(message,packs)}
          {@const generation = packResult ? undefined : generationFor(message)}
          {@const generationFailed = message.role === "assistant" && reportsGenerationFailure(message.content)}
          {@const generationWarning = message.role === "assistant" && reportsGenerationWarning(message.content)}
          <article class:user={message.role === "user"} class:failed={message.status === "failed" || generationFailed}>
            <div class="avatar">{#if message.role === "user"}<span>You</span>{:else}<span class={`provider-mark compact ${provider?.id ?? ""}`}><ProviderLogo providerId={provider?.id ?? "agent"} label={provider?.name} size={14}/></span>{/if}</div>
            <div class="message-body">
              <div class="message-meta"><strong>{message.role === "user" ? "You" : provider?.name ?? "Assistant"}</strong><time>{new Date(message.createdAt).toLocaleTimeString([], {hour:"2-digit",minute:"2-digit"})}</time></div>
              {#if message.content}<div class="content"><MarkdownMessage content={readableContent(message,Boolean(generation||packResult))} {workspacePath} {onLinkError}/></div>{/if}
              {#if generation}<SpriteArtifactCard {generation} {assets} {animations} {onEditAsset} {onEditAnimation} {onExportAsset} {onExportAnimation}/>{/if}
              {#if packResult}<PackArtifactCard pack={packResult.pack} {assets} onView={onViewPack}/>{/if}
              {#if message.status === "running"}
                <div class="working"><span class="spinner"></span> Working in project</div>
                {#if activity.length}<div class="activity">{#each activity.slice(-5) as entry}<div class={entry.level}><Terminal size={12} /><span>{entry.text}</span></div>{/each}</div>{/if}
              {:else if message.status === "failed" || generationFailed}<div class="message-state"><AlertTriangle size={12} /> Failed</div>
              {:else if message.status === "cancelled"}<div class="message-state"><X size={12} /> Cancelled</div>
              {:else if generationWarning}<div class="message-state"><AlertTriangle size={12} /> Completed with warning</div>
              {:else if message.role === "assistant"}<div class="message-state subtle"><Check size={11} /> Completed</div>{/if}
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </div>

  <div class="composer-wrap">
    {#if activeReferenceIds.length}<div class="reference-context">{#if focusedReferenceId}<Crosshair size={12}/><span>Focused reference · {activeReferenceIds.length} attached</span><button onclick={()=>onFocusReference(undefined)}><Unlock size={11}/> Clear focus</button>{:else}<BookImage size={12}/><span>{activeReferenceIds.length} reference image{activeReferenceIds.length===1?"":"s"} attached · choose one to focus</span>{/if}</div>{/if}
    <ActiveSkillsIndicator skills={customSkills ?? []} onOpen={onOpenSkills}/>
    {#if selectedAsset}<div class="context-chip"><img src={assetUrl(selectedAsset.path)} alt=""/><span>Source · {selectedAsset.name}.{selectedAsset.format}</span><button onclick={onClearAsset} title="Remove asset context"><X size={11} /></button></div>{/if}
    {#if generationActive}
      <div class="bottom-progress" class:has-issue={Boolean(generationIssue)} class:issue-error={generationIssue?.level === "error"} aria-live="polite">
        <div class="progress-heading"><div class="progress-title"><span class="spinner"></span><div><strong>{sending&&!runningRequestId?"Starting generation…":generationProgress.title}</strong><small>{latestActivity}</small></div></div><div class="progress-actions">{#if elapsedLabel}<span class="elapsed-time" aria-live="off">{elapsedLabel}</span>{/if}{#if canStopGeneration}<button onclick={onCancel}><Square size={10} fill="currentColor"/> Stop</button>{/if}</div></div>
        {#if generationIssue}<div class="progress-alert" class:warning={generationIssue.level === "warning"} class:error={generationIssue.level === "error"}><AlertTriangle size={12}/><span><strong>{generationIssue.level === "error" ? "Something went wrong" : "Warning"}</strong> {generationIssue.message}</span></div>{/if}
        <div class="frame-trail" aria-label="Sprite frames are being generated">{#each Array(8) as _, index}<span class:lit={index<generationProgress.currentIndex + 2} class:focus={index===generationProgress.currentIndex + 2} style={`--frame-delay:${index * 120}ms`}></span>{/each}</div>
        <div class="progress-stages">{#each generationProgress.stages as stage, index}<span class:complete={generationProgress.currentIndex > index} class:current={generationProgress.currentIndex === index}><i></i>{stage.label}</span>{/each}<small>{generationProfile.frameMode==="fixed"?`${generationProfile.frames} frames`:`Up to ${generationProfile.maxFrames} frames`} · {generationProfile.width}×{generationProfile.height}</small></div>
        {#if activity.length}<div class="progress-activity" aria-label="Generation activity">{#each activity.slice(-6) as entry}<div class={entry.level}><Terminal size={11}/><span>{entry.text}</span></div>{/each}</div>{/if}
      </div>
    {/if}
    {#if conversation}
      <div class="creation-toolbar">
        {#key conversation.id}<ChatSettingsMenu conversationId={conversation.id} {provider} {availableProviders} {imageProviders} {workspaceStyle} {conversationStyle} {customStyles} {animationMode} {generationProfile} {projectName} disabled={Boolean(sending || runningRequestId) || !["ready","detected"].includes(provider?.status ?? "")} onProviderSwitch={onProviderSwitch} onConversationStyle={onConversationStyle} onAnimationMode={onAnimationMode} onGenerationProfile={onGenerationProfile}/>{/key}
      </div>
    {/if}
    <div class="composer-anchor">
      {#if matchingCommands.length}
        <div class="slash-menu"><div class="slash-heading"><strong>Sprite commands</strong><span>Choose a workflow</span></div>{#each matchingCommands as command}{@const Icon=commandIcon(command.id)}<button onclick={() => chooseCommand(command.label)}><span><Icon size={15}/></span><div><strong>{command.label}</strong><p>{command.description}</p></div></button>{/each}</div>
      {/if}
    <div class="composer" class:disabled={!["ready","detected"].includes(provider?.status ?? "")} class:refining>
      {#if activeReferences.length}<div class="attached-images">{#each activeReferences as reference}<div class="attached-image" class:focused={reference.id===focusedReferenceId}><img src={assetUrl(reference.path)} alt={reference.name}/><span>{reference.id===focusedReferenceId?`Focused · ${reference.name}`:reference.name}</span><button class="focus" class:active={reference.id===focusedReferenceId} onclick={()=>onFocusReference(reference.id===focusedReferenceId?undefined:reference.id)} title={reference.id===focusedReferenceId?"Clear reference focus":"Focus this reference"}>{#if reference.id===focusedReferenceId}<Unlock size={10}/>{:else}<Crosshair size={10}/>{/if}</button><button class="remove" onclick={()=>onRemoveReference(reference.id)} title="Remove reference from this chat" aria-label={`Remove ${reference.name}`}><X size={11}/></button></div>{/each}</div>{/if}
      <textarea bind:this={textarea} bind:value={prompt} onkeydown={keydown} onpaste={paste} rows="2" readonly={refining} disabled={!conversation || refining || !["ready","detected"].includes(provider?.status ?? "")} placeholder={refining ? "Refining prompt in English…" : ["ready","detected"].includes(provider?.status ?? "") ? "Ask for a sprite, paste an image, or type / for commands…" : "Open Settings to install or sign in to this provider"}></textarea>
      <div class="composer-footer"><div class="composer-hints"><button class="attach" onclick={uploadReferences} disabled={!conversation || attaching || refining} title="Attach reference images"><Paperclip size={14}/></button><button class="attach refine" class:refining onclick={refinePrompt} disabled={!conversation || (!refining && (!prompt.trim() || sending)) || !["ready","detected"].includes(provider?.status ?? "")} title={refining ? "Cancel prompt refinement" : "Translate and refine the draft in English for precise generation"}>{#if refining}<Square size={12} fill="currentColor"/>{:else}<Languages size={14}/>{/if}</button><span>{refining?"Refining prompt… click again to cancel":attaching?"Adding image…":"Paste, refine, or attach a reference"}</span></div>
        {#if canStopGeneration}<button class="stop" onclick={onCancel} title="Stop request"><Square size={12} fill="currentColor" /></button>{:else}<button class="send" onclick={send} disabled={!prompt.trim() || !conversation || refining || !["ready","detected"].includes(provider?.status ?? "") || sending || generationActive} title="Send message"><ArrowUp size={15} /></button>{/if}
      </div>
    </div>
    </div>
  </div>
</section>

<style>
  .conversation-view{height:100%;min-width:0;display:flex;flex-direction:column;background:var(--bg)}header{height:56px;min-height:56px;box-sizing:border-box;border-bottom:1px solid var(--border);padding:0 20px;display:flex;align-items:center;justify-content:space-between}.chat-heading{display:flex;align-items:center;gap:10px;min-width:0}header h1{font-size:15px;margin:0;font-weight:650}header p{font-size:12px;color:var(--faint);margin:4px 0 0}.header-actions{display:flex;align-items:center;gap:9px}.log-export{height:30px;border:1px solid var(--border);border-radius:7px;background:var(--surface);color:var(--muted);display:flex;align-items:center;gap:6px;padding:0 10px;font:inherit;font-size:11px;cursor:pointer}.log-export:hover:not(:disabled){border-color:var(--accent);color:var(--text)}.log-export:disabled{opacity:.55;cursor:wait}.status-dot{display:flex;align-items:center;gap:7px;color:var(--faint);font-size:12px}.status-dot span{width:7px;height:7px;background:#666;border-radius:50%}.status-dot.ready span{background:#58a978;box-shadow:0 0 0 3px #58a9781c}.provider-mark{width:30px;height:30px;display:grid;place-items:center;flex:0 0 auto;border:1px solid var(--border-strong);border-radius:9px;background:#303030;line-height:1}.provider-mark.codex{background:#303030}.provider-mark.claude{background:#362a26}.provider-mark.gemini{background:#292c35}.provider-mark.grok{background:#2d2d31}.provider-mark.cursor{background:#1a1a1a}.provider-mark.antigravity{background:#1a2332}.provider-mark.compact{width:24px;height:24px;border-radius:7px}.provider-mark.hero{width:58px;height:58px;border-radius:17px;box-shadow:0 0 0 6px var(--surface)}
  .messages{flex:1;min-width:0;min-height:0;overflow:auto;padding:30px max(28px,calc((100% - 980px)/2)) 22px}.blank{height:100%;min-height:300px;display:flex;flex-direction:column;justify-content:center;align-items:center;text-align:center;color:var(--faint);padding:18px}.blank .monogram{width:94px;height:94px;border-radius:28px;background:transparent;border-color:#535353;color:var(--muted);box-shadow:none}.blank h2{font-size:31px;letter-spacing:-.045em;color:var(--text);margin:25px 0 10px}.blank p{font-size:14px;line-height:1.6;max-width:500px;margin:0}.blank p strong{font-weight:620;color:var(--muted)}.suggestions{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;width:min(740px,100%);margin-top:28px}.suggestions button{min-height:86px;display:flex;align-items:flex-start;gap:10px;border:1px solid var(--border);background:transparent;color:var(--accent);font:inherit;text-align:left;border-radius:10px;padding:14px;cursor:pointer}.suggestions button:hover{border-color:#829638;color:var(--text);background:var(--surface-hover)}.suggestions button span,.suggestions button strong,.suggestions button small{display:block}.suggestions button strong{font-size:12px;color:var(--text)}.suggestions button small{margin-top:5px;font-size:10px;line-height:1.4;color:var(--faint)}.blank-tip{display:flex;align-items:center;gap:7px;margin-top:23px;font-size:11px;color:var(--faint)}.blank-tip :global(svg){color:var(--accent)}
  .message-column{display:flex;flex-direction:column;gap:38px;width:100%;min-width:0}.message-column article{display:grid;grid-template-columns:34px minmax(0,1fr);gap:15px;width:100%;min-width:0}.avatar{width:32px;height:32px;border:1px solid var(--border-strong);border-radius:8px;display:grid;place-items:center;color:var(--muted);background:var(--surface)}article.user .avatar{border:0;background:var(--selected);font-size:12px;color:var(--muted)}.message-body{width:100%;min-width:0;max-width:850px}.message-meta{display:flex;align-items:center;gap:9px;height:32px}.message-meta strong{font-size:14px}.message-meta time{font-size:12px;color:var(--faint)}.content{display:block;width:100%;min-width:0;padding-top:7px;overflow-wrap:break-word;word-break:normal}.content :global(.markdown){display:block;width:100%;max-width:100%;min-width:0;white-space:normal;word-break:normal;overflow-wrap:break-word}.working,.message-state{font-size:12px;color:var(--muted);display:flex;gap:7px;align-items:center;margin-top:10px}.spinner{width:11px;height:11px;border-radius:50%;border:1.5px solid var(--border-strong);border-top-color:var(--accent);animation:spin .8s linear infinite}.activity{margin-top:12px;border-left:1px solid var(--border);padding-left:12px;display:flex;flex-direction:column;gap:7px}.activity div{display:flex;gap:8px;align-items:center;color:var(--faint);font-size:12px}.activity div.warning{color:#d8b26a}.activity div.error{color:#df918c}.activity span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.failed .content{color:#df918c}.message-state.subtle{opacity:.66}@keyframes spin{to{transform:rotate(360deg)}}
  .composer-wrap{padding:0 max(26px,calc((100% - 980px)/2)) 24px}.reference-context{height:28px;display:flex;align-items:center;gap:7px;color:var(--muted);font-size:10px;padding:0 3px}.reference-context :global(svg){color:var(--accent)}.reference-context button{margin-left:3px;border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;display:flex;align-items:center;gap:4px;cursor:pointer}.reference-context button:hover{color:var(--text)}.bottom-progress{margin-bottom:8px;padding:11px 12px 10px;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 8px 26px #0002}.bottom-progress.has-issue.issue-error{border-color:#8f4f4f;box-shadow:0 8px 26px #8f4f4f33}.bottom-progress.has-issue:not(.issue-error){border-color:#8a7440}.progress-alert{display:flex;align-items:flex-start;gap:7px;margin:8px 0 2px;padding:7px 8px;border-radius:7px;font-size:10px;line-height:1.45}.progress-alert.warning{background:#3a3422;color:#e0c27a}.progress-alert.error{background:#3a2424;color:#efb0ab}.progress-alert strong{font-weight:650;margin-right:4px}.progress-activity{margin-top:8px;padding-top:8px;border-top:1px solid var(--border);display:flex;flex-direction:column;gap:5px;max-height:92px;overflow:auto}.progress-activity div{display:flex;gap:7px;align-items:flex-start;color:var(--faint);font-size:10px;line-height:1.35}.progress-activity div.warning{color:#d8b26a}.progress-activity div.error{color:#df918c}.progress-activity span{min-width:0}.progress-heading{display:flex;align-items:center;justify-content:space-between;gap:14px}.progress-title{min-width:0;display:flex;align-items:center;gap:9px}.progress-actions{display:flex;align-items:center;gap:8px;flex:0 0 auto}.elapsed-time{min-width:38px;font-variant-numeric:tabular-nums;font-size:11px;color:var(--muted);letter-spacing:.02em}.bottom-progress .spinner{margin:0}.bottom-progress strong,.bottom-progress small{display:block}.bottom-progress strong{font-size:11px;color:var(--text)}.bottom-progress small{font-size:10px;color:var(--faint);margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.bottom-progress button{height:26px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:10px;cursor:pointer}.bottom-progress button:hover{background:var(--surface-hover);color:var(--text)}.frame-trail{height:20px;margin:10px 0 8px;display:flex;align-items:center;gap:4px;overflow:hidden}.frame-trail span{width:15px;height:15px;border:1px solid var(--border-strong);border-radius:3px;background:var(--bg);opacity:.42;transform:translateY(2px);animation:frame-pulse 1.6s ease-in-out infinite;animation-delay:var(--frame-delay)}.frame-trail span.lit{background:var(--accent-dim);border-color:#83963e;opacity:.9}.frame-trail span.focus{background:var(--accent);border-color:var(--accent);box-shadow:0 0 0 2px var(--accent-dim);animation-name:frame-focus}.progress-stages{display:flex;align-items:center;gap:13px}.progress-stages>span{display:flex;align-items:center;gap:5px;color:var(--faint);font-size:10px}.progress-stages i{width:7px;height:7px;border:1px solid var(--border-strong);border-radius:50%;display:block}.progress-stages .current{color:var(--text)}.progress-stages .current i{border-color:var(--accent);background:var(--accent);box-shadow:0 0 0 3px var(--accent-dim);animation:stage-pulse 1.2s ease-in-out infinite}.progress-stages .complete{color:var(--muted)}.progress-stages .complete i{border-color:var(--accent);background:var(--accent)}.progress-stages small{margin:0 0 0 auto;font-size:9px}.creation-toolbar{height:33px;display:flex;align-items:center;padding:0 4px}.context-chip{display:inline-flex;height:34px;align-items:center;gap:7px;background:var(--surface);border:1px solid var(--border);border-bottom:0;padding:0 8px 0 5px;margin-left:8px;border-radius:7px 7px 0 0;font-size:11px;color:var(--muted)}.context-chip>img{width:27px;height:27px;border-radius:4px;object-fit:contain;image-rendering:pixelated;background:var(--preview)}.context-chip button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;padding:0;cursor:pointer}.composer-anchor{position:relative}.composer{border:1px solid var(--border-strong);border-radius:16px;background:var(--composer);box-shadow:0 18px 42px #0004;overflow:hidden}.composer:focus-within{border-color:#829638;box-shadow:0 0 0 1px #b7d34b2b,0 18px 42px #0005}.composer.disabled{opacity:.7}.composer.refining textarea{color:var(--muted);cursor:default}.attach.refine.refining{color:#df918c}.attach.refine.refining:hover{color:#efb0ab;background:#3a2424}.attached-images{display:flex;gap:7px;overflow-x:auto;padding:10px 14px 0}.attached-image{position:relative;width:92px;min-width:92px;height:58px;border:1px solid var(--border);border-radius:7px;overflow:hidden;background:var(--bg)}.attached-image.focused{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.attached-image img{width:100%;height:100%;object-fit:cover;image-rendering:auto}.attached-image span{position:absolute;left:0;right:0;bottom:0;padding:10px 5px 4px;background:linear-gradient(transparent,#000c);font-size:9px;color:white;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.attached-image button{position:absolute;top:3px;width:18px;height:18px;border:0;border-radius:4px;background:#111d;color:white;display:grid;place-items:center;cursor:pointer}.attached-image .focus{left:3px}.attached-image .focus.active{color:var(--accent)}.attached-image .remove{right:3px}.composer textarea{display:block;width:100%;box-sizing:border-box;resize:none;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:16px;line-height:1.55;padding:18px 20px 8px;min-height:94px}.composer textarea::placeholder{color:var(--faint)}.composer-footer{height:49px;display:flex;align-items:center;justify-content:space-between;padding:0 12px}.composer-hints{display:flex;align-items:center;gap:8px}.composer-hints span{font-size:11px;color:var(--faint)}.attach{width:32px;height:32px;border:1px solid transparent;border-radius:7px;background:transparent;color:var(--muted);display:grid;place-items:center;cursor:pointer}.attach:hover{background:var(--surface-hover);color:var(--text)}.attach:disabled{opacity:.4}.attach.refine:hover{color:var(--accent)}.send,.stop{width:36px;height:36px;display:grid;place-items:center;border:0;border-radius:10px;cursor:pointer}.send{background:var(--accent);color:#171717}.send:disabled{opacity:.3;cursor:not-allowed}.stop{background:#a55353;color:white}@keyframes frame-pulse{0%,100%{transform:translateY(2px);opacity:.42}50%{transform:translateY(-2px);opacity:.82}}@keyframes frame-focus{0%,100%{transform:translateY(0);box-shadow:0 0 0 2px var(--accent-dim)}50%{transform:translateY(-3px);box-shadow:0 0 0 4px var(--accent-dim)}}@keyframes stage-pulse{0%,100%{box-shadow:0 0 0 3px var(--accent-dim)}50%{box-shadow:0 0 0 5px var(--accent-dim)}}
  .slash-menu{position:absolute;z-index:31;left:0;bottom:calc(100% + 8px);width:min(500px,100%);background:var(--surface);border:1px solid var(--border-strong);border-radius:9px;box-shadow:0 18px 54px #000a;padding:6px}.slash-heading{display:flex;align-items:baseline;justify-content:space-between;padding:8px 9px 7px}.slash-heading strong{font-size:11px}.slash-heading span{font-size:10px;color:var(--faint)}.slash-menu button{width:100%;display:grid;grid-template-columns:31px minmax(0,1fr);gap:9px;align-items:center;border:0;border-radius:6px;background:transparent;color:var(--text);padding:8px;text-align:left;cursor:pointer}.slash-menu button:hover{background:var(--surface-hover)}.slash-menu button>span{width:31px;height:31px;display:grid;place-items:center;border:1px solid var(--border);border-radius:6px;color:var(--accent);background:var(--bg)}.slash-menu button strong{font-size:12px}.slash-menu button p{font-size:10px;line-height:1.4;color:var(--muted);margin:3px 0 0}
</style>
