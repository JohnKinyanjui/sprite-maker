<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { ArrowUp, Bot, Square, Sparkles, Terminal, Paperclip, AlertTriangle, Check, ChevronDown, X, WandSparkles, Clapperboard, Image, UserRound, Zap, BookImage, Crosshair, Unlock, Boxes } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import SpriteArtifactCard from "$lib/components/SpriteArtifactCard.svelte";
  import PackArtifactCard from "$lib/components/PackArtifactCard.svelte";
  import { contentWithoutSpriteOutputLinks, inferMessageGeneration, inferMessagePack, reportsGenerationFailure, reportsGenerationWarning } from "$lib/message-generations";
  import StylePicker from "$lib/components/StylePicker.svelte";
  import GenerationProfileMenu from "$lib/components/GenerationProfileMenu.svelte";
  import MarkdownMessage from "$lib/components/MarkdownMessage.svelte";
  import ProviderLogo from "$lib/components/ProviderLogo.svelte";
  import { SLASH_COMMANDS } from "$lib/generation-profiles";
  import { stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import type { CustomArtStyle } from "$lib/library-types";
  import type { Animation, Asset, AssetPack, ChatGenerationProfile, Conversation, Message, ProviderStatus, ReferenceImage, SpriteGenerationMetadata } from "$lib/types";

  let { conversation, messages, provider, availableProviders=[], imageProviders=[], customStyles=[], runningRequestId, activity, selectedAsset, assets, animations, packs, references, activeReferenceIds, focusedReferenceId, draftPrompt="", workspacePath, workspaceStyle, conversationStyle, generationProfile, onSend, onCancel, onClearAsset, onEditAsset, onEditAnimation, onViewPack, onExportAsset, onExportAnimation, onConversationStyle, onGenerationProfile, onProviderSwitch, onAttachReferencePaths, onAttachReferenceFiles, onFocusReference, onRemoveReference, onDraftConsumed, onLinkError }: {
    conversation?: Conversation; messages: Message[]; provider?: ProviderStatus; availableProviders?:ProviderStatus[]; imageProviders?:ProviderStatus[]; runningRequestId?: string; activity: string[]; selectedAsset?: Asset; assets: Asset[]; animations: Animation[]; packs: AssetPack[];
    references: ReferenceImage[]; activeReferenceIds: string[]; focusedReferenceId?: string;
    draftPrompt?: string; workspacePath: string;
    workspaceStyle: StylePresetId; conversationStyle: ConversationStyleId; generationProfile: ChatGenerationProfile; customStyles?:CustomArtStyle[];
    onSend: (prompt: string) => Promise<void>; onCancel: () => void; onClearAsset: () => void; onEditAsset: (asset: Asset) => void; onEditAnimation: (animation: Animation) => void; onViewPack: (pack: AssetPack) => void; onExportAsset: (asset: Asset) => Promise<void>; onExportAnimation: (animation: Animation) => Promise<void>; onConversationStyle: (style: ConversationStyleId) => void | Promise<void>;
    onGenerationProfile: (profile: ChatGenerationProfile) => void | Promise<void>; onProviderSwitch: (providerId: string) => void | Promise<void>;
    onAttachReferencePaths: (paths: string[]) => Promise<void>; onAttachReferenceFiles: (files: File[]) => Promise<void>; onFocusReference: (id?: string) => Promise<void>; onRemoveReference: (id: string) => Promise<void>;
    onDraftConsumed: () => void; onLinkError: (message: string) => void;
  } = $props();
  let prompt = $state("");
  let sending = $state(false);
  let attaching = $state(false);
  let styleDetails = $state<HTMLDetailsElement>();
  let providerDetails = $state<HTMLDetailsElement>();
  let textarea = $state<HTMLTextAreaElement>();
  let messagePane = $state<HTMLDivElement>();
  let effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle,customStyles));
  let slashQuery = $derived(prompt.startsWith("/") && !prompt.slice(1).includes(" ") ? prompt.slice(1).toLowerCase() : undefined);
  let matchingCommands = $derived(slashQuery === undefined ? [] : SLASH_COMMANDS.filter(command => command.label.slice(1).startsWith(slashQuery)));
  let activeReferences = $derived(references.filter(reference => activeReferenceIds.includes(reference.id)));
  let latestActivity = $derived(activity.at(-1) ?? "Preparing the generation pipeline");
  let progressStage = $derived(/(?:check|inspect|quality|normaliz|validat)/i.test(latestActivity) ? 2 : /(?:render|frame|draw|generat)/i.test(latestActivity) ? 1 : 0);
  let progressTitle = $derived(progressStage === 2 ? "Checking the result" : progressStage === 1 ? "Rendering sprite frames" : "Planning the sprite");
  let providerMode = $derived(provider?.modes.find(mode => mode.id === generationProfile.model));
  let projectName = $derived(workspacePath.split("/").filter(Boolean).at(-1) ?? "Project");

  $effect(()=>{if(draftPrompt){prompt=draftPrompt;onDraftConsumed();requestAnimationFrame(()=>textarea?.focus());}});
  $effect(()=>{
    conversation?.id;
    messages.length;
    messages.at(-1)?.content;
    messages.at(-1)?.status;
    activity.length;
    requestAnimationFrame(()=>{if(messagePane)messagePane.scrollTop=messagePane.scrollHeight;});
  });

  async function chooseStyle(value: ConversationStyleId) {
    await onConversationStyle(value);
    styleDetails?.removeAttribute("open");
  }

  async function chooseProvider(providerId: string) {
    await onProviderSwitch(providerId);
    providerDetails?.removeAttribute("open");
  }

  async function chooseModel(model: string) {
    const mode = provider?.modes.find(item => item.id === model);
    await onGenerationProfile({ ...generationProfile, model, reasoningEffort: mode?.defaultReasoningEffort ?? "" });
  }

  async function send() {
    if (!prompt.trim() || sending || runningRequestId) return;
    const value = prompt;
    prompt = "";
    sending = true;
    await onSend(value);
    sending = false;
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape" && matchingCommands.length) { prompt = ""; return; }
    if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); send(); }
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
    prompt = `${label} `;
    requestAnimationFrame(() => textarea?.focus());
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
    if (message.role !== "assistant" || !["claude", "gemini", "grok"].includes(provider?.id ?? "")) return value;
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
      {#if conversation}<details class="provider-menu" bind:this={providerDetails}><summary><span class={`provider-mark compact ${provider?.id ?? ""}`}><ProviderLogo providerId={provider?.id ?? "agent"} label={provider?.name} size={14}/></span><span>{provider?.name ?? "Choose provider"}</span><ChevronDown size={12}/></summary><div class="provider-popover"><div class="provider-popover-heading"><strong>Chat provider</strong><p>Switching keeps this chat visible and starts a fresh provider session.</p></div><div class="provider-list">{#each availableProviders.filter(item=>item.kind === "agent") as item}<button class:active={item.id===provider?.id} onclick={()=>chooseProvider(item.id)} disabled={Boolean(runningRequestId)}><span class={`provider-mark compact ${item.id}`}><ProviderLogo providerId={item.id} label={item.name} size={14}/></span><span><strong>{item.name}</strong><small>{item.status === "ready" ? "Ready to use" : item.status === "detected" ? "Detected — checked when you send" : item.detail}</small></span>{#if item.id===provider?.id}<Check size={14}/>{/if}</button>{/each}</div><div class="provider-model"><label>Model<select value={generationProfile.model} onchange={(event)=>chooseModel(event.currentTarget.value)} disabled={!provider?.modes.length || Boolean(runningRequestId)}>{#if !provider?.modes.length}<option value="">Provider default</option>{/if}{#each provider?.modes ?? [] as mode}<option value={mode.id}>{mode.label}</option>{/each}</select></label>{#if providerMode?.description}<p>{providerMode.description}</p>{/if}</div></div></details>{/if}
      {#if conversation}<details class="style-menu" bind:this={styleDetails}><summary>{#if effectiveStyle.thumbnail}<img src={effectiveStyle.thumbnail.startsWith("/")?effectiveStyle.thumbnail:assetUrl(effectiveStyle.thumbnail)} alt=""/>{/if}<span>{effectiveStyle.name}</span><ChevronDown size={12}/></summary><div class="style-popover"><strong>Chat art</strong><p>Override the project art direction for this conversation.</p><StylePicker value={conversationStyle} {customStyles} allowInherit inheritedStyle={workspaceStyle} compact onChange={chooseStyle}/></div></details>{/if}
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
                {#if activity.length}<div class="activity">{#each activity.slice(-5) as line}<div><Terminal size={12} /><span>{line}</span></div>{/each}</div>{/if}
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
    {#if selectedAsset}<div class="context-chip"><img src={assetUrl(selectedAsset.path)} alt=""/><span>Source · {selectedAsset.name}.{selectedAsset.format}</span><button onclick={onClearAsset} title="Remove asset context"><X size={11} /></button></div>{/if}
    {#if sending||runningRequestId}
      <div class="bottom-progress" aria-live="polite">
        <div class="progress-heading"><div class="progress-title"><span class="spinner"></span><div><strong>{sending&&!runningRequestId?"Starting generation…":progressTitle}</strong><small>{latestActivity}</small></div></div>{#if runningRequestId}<button onclick={onCancel}><Square size={10} fill="currentColor"/> Stop</button>{/if}</div>
        <div class="frame-trail" aria-label="Sprite frames are being generated">{#each Array(8) as _, index}<span class:lit={index<5} class:focus={index===5} style={`--frame-delay:${index * 120}ms`}></span>{/each}</div>
        <div class="progress-stages"><span class:complete={progressStage>0} class:current={progressStage===0}><i></i>Plan</span><span class:complete={progressStage>1} class:current={progressStage===1}><i></i>Render</span><span class:current={progressStage===2}><i></i>Check</span><small>Up to {generationProfile.maxFrames} frames · {generationProfile.width}×{generationProfile.height}</small></div>
      </div>
    {/if}
    <div class="creation-toolbar"><div class="composer-context"><span><BookImage size={13}/>{projectName}</span><i></i><span>{provider?.name ?? "Choose provider"}</span></div>{#if conversation}<GenerationProfileMenu profile={generationProfile} {provider} {imageProviders} onChange={onGenerationProfile}/>{/if}</div>
    <div class="composer-anchor">
      {#if matchingCommands.length}
        <div class="slash-menu"><div class="slash-heading"><strong>Sprite commands</strong><span>Choose a workflow</span></div>{#each matchingCommands as command}{@const Icon=commandIcon(command.id)}<button onclick={() => chooseCommand(command.label)}><span><Icon size={15}/></span><div><strong>{command.label}</strong><p>{command.description}</p></div></button>{/each}</div>
      {/if}
    <div class="composer" class:disabled={!["ready","detected"].includes(provider?.status ?? "")}>
      {#if activeReferences.length}<div class="attached-images">{#each activeReferences as reference}<div class="attached-image" class:focused={reference.id===focusedReferenceId}><img src={assetUrl(reference.path)} alt={reference.name}/><span>{reference.id===focusedReferenceId?`Focused · ${reference.name}`:reference.name}</span><button class="focus" class:active={reference.id===focusedReferenceId} onclick={()=>onFocusReference(reference.id===focusedReferenceId?undefined:reference.id)} title={reference.id===focusedReferenceId?"Clear reference focus":"Focus this reference"}>{#if reference.id===focusedReferenceId}<Unlock size={10}/>{:else}<Crosshair size={10}/>{/if}</button><button class="remove" onclick={()=>onRemoveReference(reference.id)} title="Remove reference from this chat" aria-label={`Remove ${reference.name}`}><X size={11}/></button></div>{/each}</div>{/if}
      <textarea bind:this={textarea} bind:value={prompt} onkeydown={keydown} onpaste={paste} rows="2" disabled={!conversation || !["ready","detected"].includes(provider?.status ?? "")} placeholder={["ready","detected"].includes(provider?.status ?? "") ? "Ask for a sprite, paste an image, or type / for commands…" : "Open Settings to install or sign in to this provider"}></textarea>
      <div class="composer-footer"><div class="composer-hints"><button class="attach" onclick={uploadReferences} disabled={!conversation || attaching} title="Attach reference images"><Paperclip size={14}/></button><span>{attaching?"Adding image…":"Paste or attach a reference"}</span></div>
        {#if runningRequestId}<button class="stop" onclick={onCancel} title="Stop request"><Square size={12} fill="currentColor" /></button>{:else}<button class="send" onclick={send} disabled={!prompt.trim() || !conversation || !["ready","detected"].includes(provider?.status ?? "") || sending} title="Send message"><ArrowUp size={15} /></button>{/if}
      </div>
    </div>
    </div>
  </div>
</section>

<style>
  .conversation-view{height:100%;min-width:0;display:flex;flex-direction:column;background:var(--bg)}header{height:56px;min-height:56px;box-sizing:border-box;border-bottom:1px solid var(--border);padding:0 20px;display:flex;align-items:center;justify-content:space-between}.chat-heading{display:flex;align-items:center;gap:10px;min-width:0}header h1{font-size:15px;margin:0;font-weight:650}header p{font-size:12px;color:var(--faint);margin:4px 0 0}.header-actions{display:flex;align-items:center;gap:9px}.status-dot{display:flex;align-items:center;gap:7px;color:var(--faint);font-size:12px}.status-dot span{width:7px;height:7px;background:#666;border-radius:50%}.status-dot.ready span{background:#58a978;box-shadow:0 0 0 3px #58a9781c}.provider-mark{width:30px;height:30px;display:grid;place-items:center;flex:0 0 auto;border:1px solid var(--border-strong);border-radius:9px;background:#303030;line-height:1}.provider-mark.codex{background:#303030}.provider-mark.claude{background:#362a26}.provider-mark.gemini{background:#292c35}.provider-mark.grok{background:#2d2d31}.provider-mark.compact{width:24px;height:24px;border-radius:7px}.provider-mark.hero{width:58px;height:58px;border-radius:17px;box-shadow:0 0 0 6px var(--surface)}.provider-menu,.style-menu{position:relative}.provider-menu summary,.style-menu summary{height:34px;display:flex;align-items:center;gap:8px;border:1px solid var(--border);border-radius:7px;padding:0 9px 0 6px;background:var(--surface);color:var(--muted);font-size:12px;cursor:pointer;list-style:none}.provider-menu summary::-webkit-details-marker,.style-menu summary::-webkit-details-marker{display:none}.provider-menu summary:hover,.provider-menu[open] summary,.style-menu summary:hover,.style-menu[open] summary{border-color:var(--border-strong);color:var(--text)}.style-menu summary img{width:28px;height:22px;border-radius:4px;object-fit:cover}.provider-popover,.style-popover{position:absolute;z-index:30;top:40px;right:0;width:330px;max-height:calc(100vh - 120px);overflow:auto;padding:14px;background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 18px 54px #000a}.style-popover>strong,.provider-popover-heading strong{font-size:13px}.style-popover>p,.provider-popover-heading p{font-size:11px;color:var(--muted);margin:4px 0 0}.provider-list{display:grid;gap:4px;margin-top:12px}.provider-list button{display:grid;grid-template-columns:24px minmax(0,1fr) 16px;align-items:center;gap:9px;width:100%;padding:8px;border:1px solid transparent;border-radius:7px;background:transparent;color:var(--text);font:inherit;text-align:left;cursor:pointer}.provider-list button:hover{background:var(--surface-hover);border-color:var(--border)}.provider-list button.active{background:var(--accent-dim);border-color:#7c8c45}.provider-list button:disabled{cursor:default;opacity:.65}.provider-list strong,.provider-list small{display:block}.provider-list strong{font-size:11px}.provider-list small{margin-top:2px;color:var(--faint);font-size:9px;line-height:1.25}.provider-list :global(svg){color:var(--accent)}.provider-model{border-top:1px solid var(--border);margin-top:11px;padding-top:11px}.provider-model label{font-size:10px;color:var(--faint)}.provider-model select{display:block;width:100%;height:30px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px}.provider-model p{font-size:10px;line-height:1.4;color:var(--faint);margin:7px 1px 0}
  .messages{flex:1;min-width:0;min-height:0;overflow:auto;padding:30px max(28px,calc((100% - 980px)/2)) 22px}.blank{height:100%;min-height:300px;display:flex;flex-direction:column;justify-content:center;align-items:center;text-align:center;color:var(--faint);padding:18px}.blank .monogram{width:94px;height:94px;border-radius:28px;background:transparent;border-color:#535353;color:var(--muted);box-shadow:none}.blank h2{font-size:31px;letter-spacing:-.045em;color:var(--text);margin:25px 0 10px}.blank p{font-size:14px;line-height:1.6;max-width:500px;margin:0}.blank p strong{font-weight:620;color:var(--muted)}.suggestions{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;width:min(740px,100%);margin-top:28px}.suggestions button{min-height:86px;display:flex;align-items:flex-start;gap:10px;border:1px solid var(--border);background:transparent;color:var(--accent);font:inherit;text-align:left;border-radius:10px;padding:14px;cursor:pointer}.suggestions button:hover{border-color:#829638;color:var(--text);background:var(--surface-hover)}.suggestions button span,.suggestions button strong,.suggestions button small{display:block}.suggestions button strong{font-size:12px;color:var(--text)}.suggestions button small{margin-top:5px;font-size:10px;line-height:1.4;color:var(--faint)}.blank-tip{display:flex;align-items:center;gap:7px;margin-top:23px;font-size:11px;color:var(--faint)}.blank-tip :global(svg){color:var(--accent)}
  .message-column{display:flex;flex-direction:column;gap:38px;width:100%;min-width:0}.message-column article{display:grid;grid-template-columns:34px minmax(0,1fr);gap:15px;width:100%;min-width:0}.avatar{width:32px;height:32px;border:1px solid var(--border-strong);border-radius:8px;display:grid;place-items:center;color:var(--muted);background:var(--surface)}article.user .avatar{border:0;background:var(--selected);font-size:12px;color:var(--muted)}.message-body{width:100%;min-width:0;max-width:850px}.message-meta{display:flex;align-items:center;gap:9px;height:32px}.message-meta strong{font-size:14px}.message-meta time{font-size:12px;color:var(--faint)}.content{display:block;width:100%;min-width:0;padding-top:7px;overflow-wrap:break-word;word-break:normal}.content :global(.markdown){display:block;width:100%;max-width:100%;min-width:0;white-space:normal;word-break:normal;overflow-wrap:break-word}.working,.message-state{font-size:12px;color:var(--muted);display:flex;gap:7px;align-items:center;margin-top:10px}.spinner{width:11px;height:11px;border-radius:50%;border:1.5px solid var(--border-strong);border-top-color:var(--accent);animation:spin .8s linear infinite}.activity{margin-top:12px;border-left:1px solid var(--border);padding-left:12px;display:flex;flex-direction:column;gap:7px}.activity div{display:flex;gap:8px;align-items:center;color:var(--faint);font-size:12px}.activity span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.failed .content{color:#df918c}.message-state.subtle{opacity:.66}@keyframes spin{to{transform:rotate(360deg)}}
  .composer-wrap{padding:0 max(26px,calc((100% - 980px)/2)) 24px}.reference-context{height:28px;display:flex;align-items:center;gap:7px;color:var(--muted);font-size:10px;padding:0 3px}.reference-context :global(svg){color:var(--accent)}.reference-context button{margin-left:3px;border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;display:flex;align-items:center;gap:4px;cursor:pointer}.reference-context button:hover{color:var(--text)}.bottom-progress{margin-bottom:8px;padding:11px 12px 10px;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 8px 26px #0002}.progress-heading{display:flex;align-items:center;justify-content:space-between;gap:14px}.progress-title{min-width:0;display:flex;align-items:center;gap:9px}.bottom-progress .spinner{margin:0}.bottom-progress strong,.bottom-progress small{display:block}.bottom-progress strong{font-size:11px;color:var(--text)}.bottom-progress small{font-size:10px;color:var(--faint);margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.bottom-progress button{height:26px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:10px;cursor:pointer}.bottom-progress button:hover{background:var(--surface-hover);color:var(--text)}.frame-trail{height:20px;margin:10px 0 8px;display:flex;align-items:center;gap:4px;overflow:hidden}.frame-trail span{width:15px;height:15px;border:1px solid var(--border-strong);border-radius:3px;background:var(--bg);opacity:.42;transform:translateY(2px);animation:frame-pulse 1.6s ease-in-out infinite;animation-delay:var(--frame-delay)}.frame-trail span.lit{background:var(--accent-dim);border-color:#83963e;opacity:.9}.frame-trail span.focus{background:var(--accent);border-color:var(--accent);box-shadow:0 0 0 2px var(--accent-dim);animation-name:frame-focus}.progress-stages{display:flex;align-items:center;gap:13px}.progress-stages>span{display:flex;align-items:center;gap:5px;color:var(--faint);font-size:10px}.progress-stages i{width:7px;height:7px;border:1px solid var(--border-strong);border-radius:50%;display:block}.progress-stages .current{color:var(--text)}.progress-stages .current i{border-color:var(--accent);background:var(--accent);box-shadow:0 0 0 3px var(--accent-dim);animation:stage-pulse 1.2s ease-in-out infinite}.progress-stages .complete{color:var(--muted)}.progress-stages .complete i{border-color:var(--accent);background:var(--accent)}.progress-stages small{margin:0 0 0 auto;font-size:9px}.creation-toolbar{height:33px;display:flex;align-items:flex-start;justify-content:space-between;padding:0 4px}.composer-context{height:30px;display:flex;align-items:center;gap:9px;color:var(--muted);font-size:11px}.composer-context span{display:flex;align-items:center;gap:6px}.composer-context :global(svg){color:var(--accent)}.composer-context i{width:1px;height:14px;background:var(--border)}.context-chip{display:inline-flex;height:34px;align-items:center;gap:7px;background:var(--surface);border:1px solid var(--border);border-bottom:0;padding:0 8px 0 5px;margin-left:8px;border-radius:7px 7px 0 0;font-size:11px;color:var(--muted)}.context-chip>img{width:27px;height:27px;border-radius:4px;object-fit:contain;image-rendering:pixelated;background:var(--preview)}.context-chip button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;padding:0;cursor:pointer}.composer-anchor{position:relative}.composer{border:1px solid var(--border-strong);border-radius:16px;background:var(--composer);box-shadow:0 18px 42px #0004;overflow:hidden}.composer:focus-within{border-color:#829638;box-shadow:0 0 0 1px #b7d34b2b,0 18px 42px #0005}.composer.disabled{opacity:.7}.attached-images{display:flex;gap:7px;overflow-x:auto;padding:10px 14px 0}.attached-image{position:relative;width:92px;min-width:92px;height:58px;border:1px solid var(--border);border-radius:7px;overflow:hidden;background:var(--bg)}.attached-image.focused{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.attached-image img{width:100%;height:100%;object-fit:cover;image-rendering:auto}.attached-image span{position:absolute;left:0;right:0;bottom:0;padding:10px 5px 4px;background:linear-gradient(transparent,#000c);font-size:9px;color:white;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.attached-image button{position:absolute;top:3px;width:18px;height:18px;border:0;border-radius:4px;background:#111d;color:white;display:grid;place-items:center;cursor:pointer}.attached-image .focus{left:3px}.attached-image .focus.active{color:var(--accent)}.attached-image .remove{right:3px}.composer textarea{display:block;width:100%;box-sizing:border-box;resize:none;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:16px;line-height:1.55;padding:18px 20px 8px;min-height:94px}.composer textarea::placeholder{color:var(--faint)}.composer-footer{height:49px;display:flex;align-items:center;justify-content:space-between;padding:0 12px}.composer-hints{display:flex;align-items:center;gap:8px}.composer-hints span{font-size:11px;color:var(--faint)}.attach{width:32px;height:32px;border:1px solid transparent;border-radius:7px;background:transparent;color:var(--muted);display:grid;place-items:center;cursor:pointer}.attach:hover{background:var(--surface-hover);color:var(--text)}.attach:disabled{opacity:.4}.send,.stop{width:36px;height:36px;display:grid;place-items:center;border:0;border-radius:10px;cursor:pointer}.send{background:var(--accent);color:#171717}.send:disabled{opacity:.3;cursor:not-allowed}.stop{background:#a55353;color:white}@keyframes frame-pulse{0%,100%{transform:translateY(2px);opacity:.42}50%{transform:translateY(-2px);opacity:.82}}@keyframes frame-focus{0%,100%{transform:translateY(0);box-shadow:0 0 0 2px var(--accent-dim)}50%{transform:translateY(-3px);box-shadow:0 0 0 4px var(--accent-dim)}}@keyframes stage-pulse{0%,100%{box-shadow:0 0 0 3px var(--accent-dim)}50%{box-shadow:0 0 0 5px var(--accent-dim)}}
  .slash-menu{position:absolute;z-index:31;left:0;bottom:calc(100% + 8px);width:min(500px,100%);background:var(--surface);border:1px solid var(--border-strong);border-radius:9px;box-shadow:0 18px 54px #000a;padding:6px}.slash-heading{display:flex;align-items:baseline;justify-content:space-between;padding:8px 9px 7px}.slash-heading strong{font-size:11px}.slash-heading span{font-size:10px;color:var(--faint)}.slash-menu button{width:100%;display:grid;grid-template-columns:31px minmax(0,1fr);gap:9px;align-items:center;border:0;border-radius:6px;background:transparent;color:var(--text);padding:8px;text-align:left;cursor:pointer}.slash-menu button:hover{background:var(--surface-hover)}.slash-menu button>span{width:31px;height:31px;display:grid;place-items:center;border:1px solid var(--border);border-radius:6px;color:var(--accent);background:var(--bg)}.slash-menu button strong{font-size:12px}.slash-menu button p{font-size:10px;line-height:1.4;color:var(--muted);margin:3px 0 0}
</style>
