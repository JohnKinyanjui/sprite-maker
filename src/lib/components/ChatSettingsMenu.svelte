<script lang="ts">
  import { Bone, Check, ChevronDown, CircleHelp, Layers3, SlidersHorizontal, WandSparkles, X } from "lucide-svelte";
  import { ANIMATION_POLISH_MODES, animationPolishModeOption } from "$lib/animation-polish-modes";
  import { normalizeGenerationProfile } from "$lib/generation-profiles";
  import { stylePreset, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import type { CustomArtStyle } from "$lib/library-types";
  import type { AnimationPolishMode, ChatGenerationProfile, ProviderStatus } from "$lib/types";
  import GenerationProfileFields from "$lib/components/GenerationProfileFields.svelte";
  import ProviderLogo from "$lib/components/ProviderLogo.svelte";
  import StylePicker from "$lib/components/StylePicker.svelte";

  type SettingsTab = "provider" | "style" | "animation" | "generation";

  let { conversationId, provider, availableProviders = [], imageProviders = [], workspaceStyle, conversationStyle, customStyles = [], animationMode, generationProfile, projectName, disabled = false, onProviderSwitch, onConversationStyle, onAnimationMode, onGenerationProfile }: {
    conversationId: string;
    provider?: ProviderStatus;
    availableProviders?: ProviderStatus[];
    imageProviders?: ProviderStatus[];
    workspaceStyle: StylePresetId;
    conversationStyle: ConversationStyleId;
    customStyles?: CustomArtStyle[];
    animationMode: AnimationPolishMode;
    generationProfile: ChatGenerationProfile;
    projectName: string;
    disabled?: boolean;
    onProviderSwitch: (providerId: string) => void | Promise<void>;
    onConversationStyle: (style: ConversationStyleId) => void | Promise<void>;
    onAnimationMode: (mode: AnimationPolishMode) => void | Promise<void>;
    onGenerationProfile: (profile: ChatGenerationProfile) => void | Promise<void>;
  } = $props();

  let menu = $state<HTMLDetailsElement>();
  let showHelp = $state(false);
  let activeTab = $state<SettingsTab>("provider");

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "provider", label: "Provider" },
    { id: "style", label: "Stile" },
    { id: "animation", label: "Animazione" },
    { id: "generation", label: "Generazione" },
  ];

  const effectiveStyle = $derived(stylePreset(conversationStyle === "inherit" ? workspaceStyle : conversationStyle, customStyles));
  const qualityLabel = $derived(generationProfile.quality === "mid" ? "Mid" : generationProfile.quality[0].toUpperCase() + generationProfile.quality.slice(1));
  const animationLabel = $derived(animationPolishModeOption(animationMode).shortLabel);
  const selectedAnimation = $derived(animationPolishModeOption(animationMode));
  const selectedMode = $derived(provider?.modes.find(mode => mode.id === generationProfile.model));
  const efforts = $derived(selectedMode?.reasoningEfforts ?? []);
  const agentProviders = $derived(availableProviders.filter(item => item.kind === "agent"));

  $effect(() => {
    if (disabled) menu?.removeAttribute("open");
  });

  async function chooseProvider(providerId: string) {
    if (disabled) return;
    await onProviderSwitch(providerId);
  }

  async function chooseStyle(value: ConversationStyleId) {
    if (disabled) return;
    await onConversationStyle(value);
  }

  function chooseAnimationMode(mode: AnimationPolishMode) {
    if (disabled) return;
    return onAnimationMode(mode);
  }

  function commitProfile(value: ChatGenerationProfile) {
    if (disabled) return;
    return onGenerationProfile(normalizeGenerationProfile(value, provider?.modes ?? [], provider?.id));
  }

  function chooseModel(model: string) {
    const mode = provider?.modes.find(item => item.id === model);
    return commitProfile({ ...generationProfile, model, reasoningEffort: mode?.defaultReasoningEffort ?? "" });
  }

  function animationIcon(id: AnimationPolishMode) {
    if (id === "ai-polish") return WandSparkles;
    if (id === "full-redraw") return Layers3;
    return Bone;
  }
</script>

<details class="chat-settings" bind:this={menu}>
  <summary title="Impostazioni chat per questa conversazione">
    <SlidersHorizontal size={13}/>
    <span>{provider?.name ?? "Provider"} · {effectiveStyle.name} · {qualityLabel} {generationProfile.width}×{generationProfile.height} · {animationLabel}</span>
    <small>{projectName}</small>
    <ChevronDown size={12}/>
  </summary>
  <div class="popover">
    <div class="popover-heading">
      <div>
        <strong>Impostazioni chat</strong>
        <p>Solo per questa conversazione · {projectName}</p>
      </div>
      <button type="button" class="help-button" onclick={() => showHelp = true} title="Spiegazione impostazioni generazione" aria-label="Spiegazione impostazioni generazione">
        <CircleHelp size={15}/>
      </button>
    </div>

    <div class="tabs" role="tablist" aria-label="Sezioni impostazioni chat">
      {#each tabs as tab}
        <button
          type="button"
          role="tab"
          class:active={activeTab === tab.id}
          aria-selected={activeTab === tab.id}
          onclick={() => activeTab = tab.id}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <div class="tab-panel">
      {#if activeTab === "provider"}
        <p class="tab-hint">Cambia provider senza perdere la cronologia della chat.</p>
        <div class="provider-grid">
          {#each agentProviders as item}
            <button class="provider-chip" class:active={item.id === provider?.id} disabled={disabled} onclick={() => chooseProvider(item.id)}>
              <span class={`provider-mark compact ${item.id}`}><ProviderLogo providerId={item.id} label={item.name} size={16}/></span>
              <strong>{item.name}</strong>
              <small>{item.status === "ready" ? "Pronto" : item.status === "detected" ? "Rilevato" : "Offline"}</small>
              {#if item.id === provider?.id}<span class="chip-check"><Check size={12}/></span>{/if}
            </button>
          {/each}
        </div>
        <div class="fields two compact-fields">
          <label>Modello
            <select value={generationProfile.model} onchange={(event) => chooseModel(event.currentTarget.value)} disabled={disabled || !provider?.modes.length}>
              {#if !provider?.modes.length}<option value="">Default provider</option>{/if}
              {#each provider?.modes ?? [] as mode}<option value={mode.id}>{mode.label}</option>{/each}
            </select>
          </label>
          <label>Reasoning
            <select value={generationProfile.reasoningEffort} onchange={(event) => commitProfile({ ...generationProfile, reasoningEffort: event.currentTarget.value })} disabled={disabled || !efforts.length}>
              {#if !efforts.length}<option value="">Default</option>{/if}
              {#each efforts as effort}<option value={effort}>{effort[0].toUpperCase() + effort.slice(1)}</option>{/each}
            </select>
          </label>
        </div>
        {#if provider?.capabilities}
          <div class="capabilities" aria-label="Capacità provider">
            <span class:on={provider.capabilities.imageInput}>Immagini</span>
            <span class:on={provider.capabilities.multipleImageInput}>Multi-ref</span>
            <span class:on={provider.capabilities.structuredOutput}>Strutturato</span>
            <span class:on={provider.capabilities.transparency}>Alpha</span>
            {#if provider.capabilities.imageInput}<small>Max {provider.capabilities.maximumReferenceImages} ref</small>{/if}
          </div>
        {/if}
      {:else if activeTab === "style"}
        <p class="tab-hint">Sovrascrive lo stile del progetto solo in questa chat.</p>
        <div class="style-block" class:locked={disabled} inert={disabled}>
          <StylePicker value={conversationStyle} {customStyles} allowInherit inheritedStyle={workspaceStyle} compact onChange={chooseStyle}/>
        </div>
      {:else if activeTab === "animation"}
        <p class="tab-hint">Per <code>/animate</code> e richieste di movimento in questa chat.</p>
        <div class="segmented animation-segmented" aria-label="Modalità animazione">
          {#each ANIMATION_POLISH_MODES as option}
            {@const Icon = animationIcon(option.id)}
            <button class:active={option.id === animationMode} disabled={disabled} title={option.label} onclick={() => chooseAnimationMode(option.id)}>
              <Icon size={13}/>
              <span>{option.shortLabel}</span>
            </button>
          {/each}
        </div>
        <div class="mode-card">
          <strong>{selectedAnimation.label}{#if animationMode === "rig"} <em>Default</em>{/if}{#if animationMode === "full-redraw"} <em>Sperimentale</em>{/if}</strong>
          <p>{selectedAnimation.description}</p>
        </div>
      {:else}
        <p class="tab-hint">Qualità, canvas, frame e API immagini per questa chat.</p>
        <GenerationProfileFields profile={generationProfile} {provider} {imageProviders} compact {disabled} onChange={onGenerationProfile}/>
      {/if}
    </div>
  </div>
</details>

{#if showHelp}
  <div class="help-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && (showHelp = false)}>
    <div class="help-dialog" role="dialog" aria-modal="true" aria-labelledby="generation-help-title">
      <header>
        <div>
          <strong id="generation-help-title">Impostazioni generazione</strong>
          <p>Valori predefiniti solo per questa chat.</p>
        </div>
        <button onclick={() => showHelp = false} aria-label="Chiudi"><X size={15}/></button>
      </header>
      <dl>
        <div><dt>Qualità e canvas</dt><dd>Low, Mid e High impostano dimensioni utili. Custom sblocca valori specifici.</dd></div>
        <div><dt>Frame auto o fissi</dt><dd>Auto sceglie il conteggio migliore nel range. Fixed usa esattamente il numero impostato.</dd></div>
        <div><dt>FPS</dt><dd>Frame al secondo in riproduzione.</dd></div>
        <div><dt>Frame extra</dt><dd>Consente all'AI di usare più frame nel range per anticipazione e chiusura del loop.</dd></div>
        <div><dt>Modello e reasoning</dt><dd>Dal provider connesso. Più reasoning può migliorare la pianificazione.</dd></div>
        <div><dt>Capacità</dt><dd>Immagini, riferimenti multipli, output strutturato e trasparenza.</dd></div>
      </dl>
    </div>
  </div>
{/if}

<style>
  .chat-settings{position:relative;display:block;width:100%}
  .chat-settings summary{min-height:30px;display:flex;align-items:center;gap:7px;padding:0 9px;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--muted);font-size:11px;font-weight:600;list-style:none;cursor:pointer}
  .chat-settings summary::-webkit-details-marker{display:none}
  .chat-settings summary:hover,.chat-settings[open] summary{color:var(--text);border-color:var(--border-strong);background:var(--surface-hover)}
  .chat-settings summary :global(svg:first-child){color:var(--accent);flex:0 0 auto}
  .chat-settings summary>span{min-width:0;flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .chat-settings summary>small{flex:0 0 auto;max-width:28%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px;font-weight:500;color:var(--faint)}
  .chat-settings summary :global(svg:last-child){flex:0 0 auto;color:var(--faint)}

  .popover{position:absolute;z-index:35;left:0;bottom:38px;width:min(460px,100%);padding:14px;background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 22px 64px #000b}
  .popover-heading{display:flex;justify-content:space-between;align-items:flex-start;gap:10px;margin-bottom:10px}
  .popover-heading strong{font-size:13px}
  .popover-heading p{font-size:10px;color:var(--faint);margin:3px 0 0}
  .help-button{width:28px;height:28px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--faint);display:grid;place-items:center;cursor:pointer;flex:0 0 auto}
  .help-button:hover{color:var(--text);border-color:var(--border-strong)}

  .tabs{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:4px;padding:3px;border:1px solid var(--border);border-radius:8px;background:var(--bg)}
  .tabs button{height:28px;border:0;border-radius:5px;background:transparent;color:var(--faint);font:inherit;font-size:10px;font-weight:600;cursor:pointer;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;padding:0 6px}
  .tabs button:hover{color:var(--muted)}
  .tabs button.active{background:var(--surface-hover);color:var(--text);box-shadow:0 0 0 1px var(--border-strong)}

  .tab-panel{margin-top:12px;max-height:min(52vh,420px);overflow:auto;padding-right:2px}
  .tab-hint{font-size:10px;line-height:1.45;color:var(--faint);margin:0 0 10px}
  .tab-hint code{font-size:10px;color:var(--accent)}

  .provider-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:6px}
  .provider-chip{position:relative;min-width:0;display:flex;flex-direction:column;align-items:center;gap:4px;padding:8px 6px 7px;border:1px solid var(--border);border-radius:8px;background:var(--bg);color:var(--text);font:inherit;text-align:center;cursor:pointer}
  .provider-chip:hover:not(:disabled){border-color:var(--border-strong);background:var(--surface-hover)}
  .provider-chip.active{border-color:var(--accent);background:var(--accent-dim);box-shadow:0 0 0 1px var(--accent-dim)}
  .provider-chip:disabled{opacity:.55;cursor:default}
  .provider-chip strong{font-size:10px;line-height:1.2}
  .provider-chip small{font-size:9px;color:var(--faint);line-height:1.2}
  .chip-check{position:absolute;top:5px;right:5px;color:var(--accent);display:grid;place-items:center}
  .provider-mark{width:30px;height:30px;display:grid;place-items:center;flex:0 0 auto;border:1px solid var(--border-strong);border-radius:9px;background:#303030;line-height:1}
  .provider-mark.codex{background:#303030}.provider-mark.claude{background:#362a26}.provider-mark.gemini{background:#292c35}.provider-mark.grok{background:#2d2d31}.provider-mark.cursor{background:#1a1a1a}.provider-mark.antigravity{background:#1a2332}
  .provider-mark.compact{width:28px;height:28px;border-radius:7px}

  .compact-fields{margin-top:10px}
  .fields{display:grid;gap:8px}
  .fields.two{grid-template-columns:1.45fr 1fr}
  .fields label{min-width:0;font-size:10px;color:var(--faint)}
  select{display:block;width:100%;height:31px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px;outline:0}
  select:focus{border-color:var(--accent)}
  select:disabled{opacity:.55}

  .capabilities{display:flex;align-items:center;gap:5px;flex-wrap:wrap;margin-top:10px}
  .capabilities span{font-size:9px;color:var(--faint);border:1px solid var(--border);border-radius:999px;padding:3px 6px}
  .capabilities span.on{color:var(--muted);border-color:var(--border-strong);background:var(--bg)}
  .capabilities small{font-size:9px;color:var(--faint);margin-left:auto}

  .style-block.locked{pointer-events:none;opacity:.55}

  .segmented{display:flex;background:var(--bg);border:1px solid var(--border);border-radius:7px;padding:3px;gap:3px}
  .animation-segmented button{flex:1;min-width:0;height:34px;border:0;border-radius:5px;background:transparent;color:var(--faint);font:inherit;font-size:10px;font-weight:600;display:flex;align-items:center;justify-content:center;gap:5px;cursor:pointer}
  .animation-segmented button.active{background:var(--surface-hover);color:var(--text);box-shadow:0 0 0 1px var(--border-strong)}
  .animation-segmented button:disabled{opacity:.55;cursor:default}

  .mode-card{margin-top:10px;padding:10px;border:1px solid var(--border);border-radius:8px;background:var(--bg)}
  .mode-card strong{font-size:11px}
  .mode-card em{font-style:normal;font-size:8px;color:var(--accent);margin-left:4px}
  .mode-card p{font-size:10px;line-height:1.45;color:var(--faint);margin:6px 0 0}

  .help-backdrop{position:fixed;inset:0;z-index:70;background:#000a;display:grid;place-items:center;padding:18px}
  .help-dialog{width:min(560px,calc(100vw - 36px));max-height:calc(100vh - 36px);overflow:auto;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 28px 90px #000d;padding:18px}
  .help-dialog header{display:flex;align-items:flex-start;justify-content:space-between;border-bottom:1px solid var(--border);padding-bottom:13px}
  .help-dialog header strong{font-size:15px}
  .help-dialog header p{font-size:10px;line-height:1.45;color:var(--faint);margin:4px 0 0}
  .help-dialog header button{width:28px;height:28px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}
  .help-dialog dl{display:grid;gap:0;margin:0}
  .help-dialog dl>div{display:grid;grid-template-columns:130px minmax(0,1fr);gap:16px;padding:12px 2px;border-bottom:1px solid var(--border)}
  .help-dialog dl>div:last-child{border-bottom:0}
  .help-dialog dt{font-size:11px;font-weight:650;color:var(--text)}
  .help-dialog dd{font-size:10px;line-height:1.5;color:var(--muted);margin:0}

  @media (max-width: 520px) {
    .provider-grid{grid-template-columns:repeat(2,minmax(0,1fr))}
    .tabs button{font-size:9px;padding:0 4px}
  }
</style>
