<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { ArrowUp, Bot, Square, Sparkles, Terminal, Paperclip, AlertTriangle, Check, ChevronDown, X, WandSparkles, Clapperboard, Image, UserRound, Zap, BookImage, Crosshair, Unlock, Boxes } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import SpriteArtifactCard from "$lib/components/SpriteArtifactCard.svelte";
  import PackArtifactCard from "$lib/components/PackArtifactCard.svelte";
  import { contentWithoutSpriteOutputLinks, inferMessageGeneration, inferMessagePack } from "$lib/message-generations";
  import StylePicker from "$lib/components/StylePicker.svelte";
  import GenerationProfileMenu from "$lib/components/GenerationProfileMenu.svelte";
  import MarkdownMessage from "$lib/components/MarkdownMessage.svelte";
  import { SLASH_COMMANDS } from "$lib/generation-profiles";
  import { stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import type { Animation, Asset, AssetPack, ChatGenerationProfile, Conversation, Message, ProviderStatus, ReferenceImage, SpriteGenerationMetadata } from "$lib/types";

  let { conversation, messages, provider, runningRequestId, activity, selectedAsset, assets, animations, packs, references, activeReferenceIds, focusedReferenceId, draftPrompt="", workspacePath, workspaceStyle, conversationStyle, generationProfile, onSend, onCancel, onClearAsset, onEditAsset, onEditAnimation, onViewPack, onExportAsset, onExportAnimation, onConversationStyle, onGenerationProfile, onAttachReferencePaths, onAttachReferenceFiles, onFocusReference, onRemoveReference, onDraftConsumed, onLinkError }: {
    conversation?: Conversation; messages: Message[]; provider?: ProviderStatus; runningRequestId?: string; activity: string[]; selectedAsset?: Asset; assets: Asset[]; animations: Animation[]; packs: AssetPack[];
    references: ReferenceImage[]; activeReferenceIds: string[]; focusedReferenceId?: string;
    draftPrompt?: string; workspacePath: string;
    workspaceStyle: StylePresetId; conversationStyle: ConversationStyleId; generationProfile: ChatGenerationProfile;
    onSend: (prompt: string) => Promise<void>; onCancel: () => void; onClearAsset: () => void; onEditAsset: (asset: Asset) => void; onEditAnimation: (animation: Animation) => void; onViewPack: (pack: AssetPack) => void; onExportAsset: (asset: Asset) => Promise<void>; onExportAnimation: (animation: Animation) => Promise<void>; onConversationStyle: (style: ConversationStyleId) => void | Promise<void>;
    onGenerationProfile: (profile: ChatGenerationProfile) => void | Promise<void>;
    onAttachReferencePaths: (paths: string[]) => Promise<void>; onAttachReferenceFiles: (files: File[]) => Promise<void>; onFocusReference: (id?: string) => Promise<void>; onRemoveReference: (id: string) => Promise<void>;
    onDraftConsumed: () => void; onLinkError: (message: string) => void;
  } = $props();
  let prompt = $state("");
  let sending = $state(false);
  let attaching = $state(false);
  let styleDetails = $state<HTMLDetailsElement>();
  let textarea = $state<HTMLTextAreaElement>();
  let messagePane = $state<HTMLDivElement>();
  let effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle));
  let slashQuery = $derived(prompt.startsWith("/") && !prompt.slice(1).includes(" ") ? prompt.slice(1).toLowerCase() : undefined);
  let matchingCommands = $derived(slashQuery === undefined ? [] : SLASH_COMMANDS.filter(command => command.label.slice(1).startsWith(slashQuery)));
  let activeReferences = $derived(references.filter(reference => activeReferenceIds.includes(reference.id)));

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
</script>

<section class="conversation-view">
  <header>
    <div><h1>{conversation?.title ?? "Agent"}</h1><p>{provider?.name ?? "No provider selected"} · {provider?.status === "ready" ? "Ready" : "Unavailable"}</p></div>
    <div class="header-actions">
      {#if conversation}<details class="style-menu" bind:this={styleDetails}><summary><img src={effectiveStyle.thumbnail} alt=""/><span>{effectiveStyle.name}</span><ChevronDown size={12}/></summary><div class="style-popover"><strong>Chat style</strong><p>Override the workspace default for this conversation.</p><StylePicker value={conversationStyle} allowInherit inheritedStyle={workspaceStyle} compact onChange={chooseStyle}/></div></details>{/if}
      <div class:ready={provider?.status === "ready"} class="status-dot"><span></span>{provider?.status === "ready" ? "Connected" : "Offline"}</div>
    </div>
  </header>

  <div class="messages" bind:this={messagePane} aria-live="polite">
    {#if !conversation}
      <div class="blank"><Bot size={29} strokeWidth={1.35} /><h2>Start a workspace conversation</h2><p>Create a conversation to work with an installed AI agent in this asset folder.</p></div>
    {:else if !messages.length}
      <div class="blank"><Sparkles size={27} strokeWidth={1.4} /><h2>Create a sprite with Codex</h2><p>Describe the game asset you want. Codex will infer practical sprite settings, draw real PNG frames, and show an animated preview directly in the conversation.</p><div class="suggestions"><button class="generate" onclick={() => prompt = "Generate a polished 4-frame 32x32 pixel-art blue knight idle animation with a transparent background."}><WandSparkles size={12}/> Generate animated sprite</button><button onclick={() => prompt = "Generate a 32x32 pixel-art glowing health potion prop with a transparent background."}>Generate one asset</button></div></div>
    {:else}
      <div class="message-column">
        {#each messages as message}
          {@const packResult = inferMessagePack(message,packs)}
          {@const generation = packResult ? undefined : generationFor(message)}
          <article class:user={message.role === "user"} class:failed={message.status === "failed"}>
            <div class="avatar">{#if message.role === "user"}<span>You</span>{:else}<Bot size={15} />{/if}</div>
            <div class="message-body">
              <div class="message-meta"><strong>{message.role === "user" ? "You" : "Codex"}</strong><time>{new Date(message.createdAt).toLocaleTimeString([], {hour:"2-digit",minute:"2-digit"})}</time></div>
              {#if message.content}<div class="content"><MarkdownMessage content={visibleContent(message,Boolean(generation||packResult))} {workspacePath} {onLinkError}/></div>{/if}
              {#if generation}<SpriteArtifactCard {generation} {assets} {animations} {onEditAsset} {onEditAnimation} {onExportAsset} {onExportAnimation}/>{/if}
              {#if packResult}<PackArtifactCard pack={packResult.pack} {assets} onView={onViewPack}/>{/if}
              {#if message.status === "running"}
                <div class="working"><span class="spinner"></span> Working in workspace</div>
                {#if activity.length}<div class="activity">{#each activity.slice(-5) as line}<div><Terminal size={12} /><span>{line}</span></div>{/each}</div>{/if}
              {:else if message.status === "failed"}<div class="message-state"><AlertTriangle size={12} /> Failed</div>
              {:else if message.status === "cancelled"}<div class="message-state"><X size={12} /> Cancelled</div>
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
    {#if sending||runningRequestId}<div class="bottom-progress" aria-live="polite"><span class="spinner"></span><div><strong>{sending&&!runningRequestId?"Starting generation…":"Generating in this chat"}</strong><small>{activity.at(-1)??"Codex is working in the active workspace"}</small></div>{#if runningRequestId}<button onclick={onCancel}><Square size={10} fill="currentColor"/> Stop</button>{/if}</div>{/if}
    <div class="creation-toolbar"><div class="creation-note"><WandSparkles size={11}/><span>Sprite generation</span><kbd>/</kbd><small>commands</small></div>{#if conversation}<GenerationProfileMenu profile={generationProfile} {provider} onChange={onGenerationProfile}/>{/if}</div>
    <div class="composer-anchor">
      {#if matchingCommands.length}
        <div class="slash-menu"><div class="slash-heading"><strong>Sprite commands</strong><span>Choose a workflow</span></div>{#each matchingCommands as command}{@const Icon=commandIcon(command.id)}<button onclick={() => chooseCommand(command.label)}><span><Icon size={15}/></span><div><strong>{command.label}</strong><p>{command.description}</p></div></button>{/each}</div>
      {/if}
    <div class="composer" class:disabled={provider?.status !== "ready"}>
      {#if activeReferences.length}<div class="attached-images">{#each activeReferences as reference}<div class="attached-image" class:focused={reference.id===focusedReferenceId}><img src={assetUrl(reference.path)} alt={reference.name}/><span>{reference.id===focusedReferenceId?`Focused · ${reference.name}`:reference.name}</span><button class="focus" class:active={reference.id===focusedReferenceId} onclick={()=>onFocusReference(reference.id===focusedReferenceId?undefined:reference.id)} title={reference.id===focusedReferenceId?"Clear reference focus":"Focus this reference"}>{#if reference.id===focusedReferenceId}<Unlock size={10}/>{:else}<Crosshair size={10}/>{/if}</button><button class="remove" onclick={()=>onRemoveReference(reference.id)} title="Remove reference from this chat" aria-label={`Remove ${reference.name}`}><X size={11}/></button></div>{/each}</div>{/if}
      <textarea bind:this={textarea} bind:value={prompt} onkeydown={keydown} onpaste={paste} rows="2" disabled={!conversation || provider?.status !== "ready"} placeholder={provider?.status === "ready" ? "Ask for a sprite, paste an image, or type / for commands…" : "Install Codex CLI to enable agent conversations"}></textarea>
      <div class="composer-footer"><div class="composer-hints"><button class="attach" onclick={uploadReferences} disabled={!conversation || attaching} title="Attach reference images"><Paperclip size={14}/></button><span>{attaching?"Adding image…":"Paste or attach a reference"}</span></div>
        {#if runningRequestId}<button class="stop" onclick={onCancel} title="Stop request"><Square size={12} fill="currentColor" /></button>{:else}<button class="send" onclick={send} disabled={!prompt.trim() || !conversation || provider?.status !== "ready" || sending} title="Send message"><ArrowUp size={15} /></button>{/if}
      </div>
    </div>
    </div>
  </div>
</section>

<style>
  .conversation-view{height:100%;min-width:0;display:flex;flex-direction:column;background:var(--bg)}header{height:56px;min-height:56px;box-sizing:border-box;border-bottom:1px solid var(--border);padding:0 20px;display:flex;align-items:center;justify-content:space-between}header h1{font-size:15px;margin:0;font-weight:650}header p{font-size:12px;color:var(--faint);margin:4px 0 0}.header-actions{display:flex;align-items:center;gap:13px}.status-dot{display:flex;align-items:center;gap:7px;color:var(--faint);font-size:12px}.status-dot span{width:7px;height:7px;background:#666;border-radius:50%}.status-dot.ready span{background:#58a978;box-shadow:0 0 0 3px #58a9781c}.style-menu{position:relative}.style-menu summary{height:34px;display:flex;align-items:center;gap:8px;border:1px solid var(--border);border-radius:7px;padding:0 9px 0 6px;background:var(--surface);color:var(--muted);font-size:12px;cursor:pointer;list-style:none}.style-menu summary::-webkit-details-marker{display:none}.style-menu summary:hover,.style-menu[open] summary{border-color:var(--border-strong);color:var(--text)}.style-menu summary img{width:28px;height:22px;border-radius:4px;object-fit:cover}.style-popover{position:absolute;z-index:30;top:40px;right:0;width:330px;max-height:calc(100vh - 120px);overflow:auto;padding:14px;background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 18px 54px #000a}.style-popover>strong{font-size:13px}.style-popover>p{font-size:11px;color:var(--muted);margin:4px 0 0}
  .messages{flex:1;overflow:auto;padding:40px max(28px,calc((100% - 980px)/2)) 28px}.blank{height:100%;min-height:300px;display:flex;flex-direction:column;justify-content:center;align-items:center;text-align:center;color:var(--faint)}.blank h2{font-size:21px;color:var(--text);margin:18px 0 9px}.blank p{font-size:14px;line-height:1.6;max-width:540px;margin:0}.suggestions{display:flex;gap:9px;margin-top:24px}.suggestions button{border:1px solid var(--border);background:var(--surface);color:var(--muted);font:inherit;font-size:12px;border-radius:7px;padding:9px 13px;cursor:pointer}.suggestions button:hover{border-color:var(--border-strong);background:var(--surface-hover);color:var(--text)}
  .message-column{display:flex;flex-direction:column;gap:38px}article{display:grid;grid-template-columns:34px minmax(0,1fr);gap:15px}.avatar{width:32px;height:32px;border:1px solid var(--border-strong);border-radius:8px;display:grid;place-items:center;color:var(--muted);background:var(--surface)}article.user .avatar{border:0;background:var(--selected);font-size:12px;color:var(--muted)}.message-meta{display:flex;align-items:center;gap:9px;height:32px}.message-meta strong{font-size:14px}.message-meta time{font-size:12px;color:var(--faint)}.content{padding-top:7px;overflow-wrap:anywhere}.working,.message-state{font-size:12px;color:var(--muted);display:flex;gap:7px;align-items:center;margin-top:10px}.spinner{width:11px;height:11px;border-radius:50%;border:1.5px solid var(--border-strong);border-top-color:var(--accent);animation:spin .8s linear infinite}.activity{margin-top:12px;border-left:1px solid var(--border);padding-left:12px;display:flex;flex-direction:column;gap:7px}.activity div{display:flex;gap:8px;align-items:center;color:var(--faint);font-size:12px}.activity span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.failed .content{color:#df918c}.message-state.subtle{opacity:.66}@keyframes spin{to{transform:rotate(360deg)}}
  .composer-wrap{padding:0 max(26px,calc((100% - 920px)/2)) 22px}.reference-context{height:28px;display:flex;align-items:center;gap:7px;color:var(--muted);font-size:10px;padding:0 3px}.reference-context :global(svg){color:var(--accent)}.reference-context button{margin-left:3px;border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;display:flex;align-items:center;gap:4px;cursor:pointer}.reference-context button:hover{color:var(--text)}.bottom-progress{min-height:43px;margin-bottom:8px;padding:7px 9px;border:1px solid var(--border-strong);border-radius:8px;background:var(--surface);display:grid;grid-template-columns:15px minmax(0,1fr) auto;align-items:center;gap:9px;box-shadow:0 8px 26px #0002}.bottom-progress .spinner{margin:0}.bottom-progress strong,.bottom-progress small{display:block}.bottom-progress strong{font-size:11px}.bottom-progress small{font-size:10px;color:var(--faint);margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.bottom-progress button{height:26px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:10px;cursor:pointer}.creation-toolbar{height:38px;display:flex;align-items:flex-start;justify-content:space-between;padding:0 2px}.creation-note{height:30px;display:flex;align-items:center;gap:7px;color:var(--faint);font-size:12px}.creation-note :global(svg){color:var(--accent)}.creation-note kbd{margin-left:4px}.creation-note small{font-size:10px;color:var(--faint)}.context-chip{display:inline-flex;height:34px;align-items:center;gap:7px;background:var(--surface);border:1px solid var(--border);border-bottom:0;padding:0 8px 0 5px;margin-left:8px;border-radius:7px 7px 0 0;font-size:11px;color:var(--muted)}.context-chip>img{width:27px;height:27px;border-radius:4px;object-fit:contain;image-rendering:pixelated;background:var(--preview)}.context-chip button{border:0;background:transparent;color:var(--faint);display:grid;place-items:center;padding:0;cursor:pointer}.composer-anchor{position:relative}.composer{border:1px solid var(--border-strong);border-radius:11px;background:var(--composer);box-shadow:0 10px 34px #0003;overflow:hidden}.composer:focus-within{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim),0 12px 36px #0004}.composer.disabled{opacity:.7}.attached-images{display:flex;gap:7px;overflow-x:auto;padding:10px 12px 0}.attached-image{position:relative;width:92px;min-width:92px;height:58px;border:1px solid var(--border);border-radius:7px;overflow:hidden;background:var(--bg)}.attached-image.focused{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.attached-image img{width:100%;height:100%;object-fit:cover;image-rendering:auto}.attached-image span{position:absolute;left:0;right:0;bottom:0;padding:10px 5px 4px;background:linear-gradient(transparent,#000c);font-size:9px;color:white;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.attached-image button{position:absolute;top:3px;width:18px;height:18px;border:0;border-radius:4px;background:#111d;color:white;display:grid;place-items:center;cursor:pointer}.attached-image .focus{left:3px}.attached-image .focus.active{color:var(--accent)}.attached-image .remove{right:3px}.composer textarea{display:block;width:100%;box-sizing:border-box;resize:none;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:15px;line-height:1.55;padding:16px 17px 6px;min-height:72px}.composer textarea::placeholder{color:var(--faint)}.composer-footer{height:42px;display:flex;align-items:center;justify-content:space-between;padding:0 9px}.composer-hints{display:flex;align-items:center;gap:8px}.composer-hints span{font-size:11px;color:var(--faint)}.attach{width:30px;height:30px;border:1px solid transparent;border-radius:6px;background:transparent;color:var(--muted);display:grid;place-items:center;cursor:pointer}.attach:hover{background:var(--surface-hover);color:var(--text)}.attach:disabled{opacity:.4}.send,.stop{width:32px;height:32px;display:grid;place-items:center;border:0;border-radius:7px;cursor:pointer}.send{background:var(--text);color:var(--bg)}.send:disabled{opacity:.3;cursor:not-allowed}.stop{background:#a55353;color:white}
  .slash-menu{position:absolute;z-index:31;left:0;bottom:calc(100% + 8px);width:min(500px,100%);background:var(--surface);border:1px solid var(--border-strong);border-radius:9px;box-shadow:0 18px 54px #000a;padding:6px}.slash-heading{display:flex;align-items:baseline;justify-content:space-between;padding:8px 9px 7px}.slash-heading strong{font-size:11px}.slash-heading span{font-size:10px;color:var(--faint)}.slash-menu button{width:100%;display:grid;grid-template-columns:31px minmax(0,1fr);gap:9px;align-items:center;border:0;border-radius:6px;background:transparent;color:var(--text);padding:8px;text-align:left;cursor:pointer}.slash-menu button:hover{background:var(--surface-hover)}.slash-menu button>span{width:31px;height:31px;display:grid;place-items:center;border:1px solid var(--border);border-radius:6px;color:var(--accent);background:var(--bg)}.slash-menu button strong{font-size:12px}.slash-menu button p{font-size:10px;line-height:1.4;color:var(--muted);margin:3px 0 0}
</style>
