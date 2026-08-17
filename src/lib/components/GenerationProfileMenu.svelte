<script lang="ts">
  import { ChevronDown, CircleHelp, Gauge, SlidersHorizontal, X } from "lucide-svelte";
  import { normalizeGenerationProfile, profileForQuality } from "$lib/generation-profiles";
  import type { ChatGenerationProfile, GenerationQuality, ProviderStatus } from "$lib/types";

  let { profile, provider, imageProviders=[], onChange }: {
    profile: ChatGenerationProfile;
    provider?: ProviderStatus;
    imageProviders?: ProviderStatus[];
    onChange: (profile: ChatGenerationProfile) => void | Promise<void>;
  } = $props();

  let menu = $state<HTMLDetailsElement>();
  let showHelp = $state(false);
  const selectedMode = $derived(provider?.modes.find(mode => mode.id === profile.model));
  const efforts = $derived(selectedMode?.reasoningEfforts ?? []);
  const qualityLabel = $derived(profile.quality === "mid" ? "Mid" : profile.quality[0].toUpperCase() + profile.quality.slice(1));
  const frameLabel = $derived(profile.frameMode === "auto" ? `Auto ${profile.minFrames}–${profile.maxFrames}f` : `${profile.frames}f`);

  function commit(value: ChatGenerationProfile) {
    return onChange(normalizeGenerationProfile(value, provider?.modes ?? []));
  }

  function chooseQuality(quality: GenerationQuality) {
    if (quality === "custom") return commit({ ...profile, quality });
    return commit(profileForQuality(quality, profile));
  }

  function numberChange(key: "width" | "height" | "frames" | "fps" | "minFrames" | "maxFrames", value: string) {
    return commit({ ...profile, quality: "custom", [key]: Number(value) });
  }

  function modelChange(model: string) {
    const mode = provider?.modes.find(item => item.id === model);
    return commit({ ...profile, model, reasoningEffort: mode?.defaultReasoningEffort ?? "" });
  }
</script>

<details class="generation-menu" bind:this={menu}>
  <summary title="Generation settings for this chat"><Gauge size={13}/><span>{qualityLabel} · {profile.width}×{profile.height} · {frameLabel}</span><ChevronDown size={12}/></summary>
  <div class="popover">
    <div class="popover-heading"><div><strong>Generation profile</strong><p>Saved only for this chat.</p></div><button class="help-button" onclick={()=>showHelp=true} title="Explain generation settings" aria-label="Explain generation settings"><CircleHelp size={15}/></button></div>

    <div class="preset-grid" aria-label="Generation quality">
      {#each ["low", "mid", "high", "custom"] as quality}
        <button class:active={profile.quality === quality} onclick={() => chooseQuality(quality as GenerationQuality)}>
          <strong>{quality === "mid" ? "Mid" : quality[0].toUpperCase() + quality.slice(1)}</strong>
      <span>{quality === "low" ? "64px · 6–8f" : quality === "mid" ? "128px · 8–12f" : quality === "high" ? "256px · 12–16f" : "Your values"}</span>
        </button>
      {/each}
    </div>

    <div class="frame-policy">
      <div><strong>Frames</strong><span>{profile.frameMode === "auto" ? "AI uses the smallest count that keeps the motion clear" : "Use an exact count you choose"}</span></div>
      <div class="segmented" aria-label="Frame policy">
        <button class:active={profile.frameMode === "auto"} onclick={() => commit({...profile, frameMode:"auto", allowAutoAdjust:true})}>Auto</button>
        <button class:active={profile.frameMode === "fixed"} onclick={() => commit({...profile, frameMode:"fixed", allowAutoAdjust:false})}>Fixed</button>
      </div>
    </div>

    <div class="fields dimensions">
      <label>Width<input type="number" min="8" max="512" value={profile.width} onchange={(event) => numberChange("width", event.currentTarget.value)}/></label>
      <label>Height<input type="number" min="8" max="512" value={profile.height} onchange={(event) => numberChange("height", event.currentTarget.value)}/></label>
      <label>FPS<input type="number" min="1" max="60" value={profile.fps} onchange={(event) => numberChange("fps", event.currentTarget.value)}/></label>
    </div>
    {#if profile.frameMode === "fixed"}
      <div class="fields fixed-count"><label>Exact frame count<input type="number" min="1" max="64" value={profile.frames} onchange={(event) => numberChange("frames", event.currentTarget.value)}/></label></div>
    {:else}
      <div class="fields two auto-range"><label>Minimum<input type="number" min="1" max="64" value={profile.minFrames} onchange={(event) => numberChange("minFrames", event.currentTarget.value)}/></label><label>Maximum<input type="number" min="1" max="64" value={profile.maxFrames} onchange={(event) => numberChange("maxFrames", event.currentTarget.value)}/></label></div>
      <div class="auto-options">
        <button class="switch-row" role="switch" aria-checked={profile.allowAutoAdjust} onclick={()=>commit({...profile,allowAutoAdjust:!profile.allowAutoAdjust})}><span><strong>Allow extra frames</strong><small>Add frames only when the motion cannot read cleanly without them</small></span><i class:on={profile.allowAutoAdjust}><b></b></i></button>
      </div>
    {/if}

    <div class="provider-section">
      <div><strong>Provider mode</strong><span>{provider?.modes.length ?? 0} available from {provider?.name ?? "provider"}</span></div>
      <div class="fields two">
        <label>Model<select value={profile.model} onchange={(event) => modelChange(event.currentTarget.value)} disabled={!provider?.modes.length}>
          {#if !provider?.modes.length}<option value="">Provider default</option>{/if}
          {#each provider?.modes ?? [] as mode}<option value={mode.id}>{mode.label}</option>{/each}
        </select></label>
        <label>Reasoning<select value={profile.reasoningEffort} onchange={(event) => commit({ ...profile, reasoningEffort: event.currentTarget.value })} disabled={!efforts.length}>
          {#if !efforts.length}<option value="">Default</option>{/if}
          {#each efforts as effort}<option value={effort}>{effort[0].toUpperCase() + effort.slice(1)}</option>{/each}
        </select></label>
      </div>
      {#if selectedMode?.description}<p class="mode-description">{selectedMode.description}</p>{/if}
      {#if provider?.capabilities}
        <div class="capabilities" aria-label="Provider capabilities">
          <span class:on={provider.capabilities.imageInput}>Images</span>
          <span class:on={provider.capabilities.multipleImageInput}>Multi-reference</span>
          <span class:on={provider.capabilities.structuredOutput}>Structured</span>
          <span class:on={provider.capabilities.transparency}>Alpha</span>
          {#if provider.capabilities.imageInput}<small>Up to {provider.capabilities.maximumReferenceImages} refs</small>{/if}
        </div>
      {/if}
      <div class="image-source-heading"><strong>Animation image provider</strong><span>Generates the master and every animation frame</span></div>
      <div class="fields"><label>Image API<select value={profile.imageProviderId} onchange={(event)=>commit({...profile,imageProviderId:event.currentTarget.value})}>
        {#if provider?.id !== "codex"}<option value="provider-native">Chat only — no image API</option>{/if}
        {#each imageProviders.filter(item=>item.status==="ready" && (item.id!=="imagegen" || provider?.id==="codex")) as imageProvider}<option value={imageProvider.id}>{imageProvider.name}</option>{/each}
      </select></label></div>
      {#if provider?.id !== "codex" && (profile.imageProviderId === "provider-native" || profile.imageProviderId === "imagegen")}<p class="mode-description">This sends a chat-only request. To create a source image, choose Grok Image, Gemini Image, or another configured image API.</p>{/if}
    </div>
    {#if showHelp}
      <div class="help-backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&(showHelp=false)}>
        <div class="help-dialog" role="dialog" aria-modal="true" aria-labelledby="generation-help-title">
          <header><div><strong id="generation-help-title">Generation settings</strong><p>These defaults apply only to this chat and can be changed any time.</p></div><button onclick={()=>showHelp=false} aria-label="Close help"><X size={15}/></button></header>
          <dl>
            <div><dt>Quality and canvas</dt><dd>Low, Mid, and High choose useful starting dimensions. Custom unlocks project-specific values.</dd></div>
            <div><dt>Auto or Fixed frames</dt><dd>Auto favors smooth motion inside your range. Fixed uses exactly your chosen count, so you can lower cost or create intentionally stepped animation.</dd></div>
            <div><dt>FPS</dt><dd>Playback frames per second. Higher FPS is faster unless frame durations are retimed in the animation editor.</dd></div>
            <div><dt>Use more frames</dt><dd>Allows the AI sequence planner to use more of the selected range for smaller pose changes, anticipation, follow-through, and loop closure.</dd></div>
            <div><dt>Model and reasoning</dt><dd>Modes come from the connected provider. More reasoning may improve planning but can take longer.</dd></div>
            <div><dt>Capabilities</dt><dd>Shows whether the selected provider supports images, multiple references, structured output, and transparency.</dd></div>
          </dl>
        </div>
      </div>
    {/if}
  </div>
</details>

<style>
  .generation-menu{position:relative}.generation-menu summary{height:30px;display:flex;align-items:center;gap:7px;padding:0 9px;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--muted);font-size:11px;font-weight:600;list-style:none;cursor:pointer}.generation-menu summary::-webkit-details-marker{display:none}.generation-menu summary:hover,.generation-menu[open] summary{color:var(--text);border-color:var(--border-strong);background:var(--surface-hover)}.generation-menu summary :global(svg:first-child){color:var(--accent)}
  .popover{position:absolute;z-index:35;right:0;bottom:38px;width:min(460px,calc(100vw - 40px));padding:16px;background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 22px 64px #000b}.popover-heading{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:13px}.popover-heading strong,.provider-section strong{font-size:13px}.popover-heading p{font-size:11px;color:var(--faint);margin:3px 0 0}.help-button{width:28px;height:28px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--faint);display:grid;place-items:center;cursor:pointer}.help-button:hover{color:var(--text);border-color:var(--border-strong)}
  .preset-grid{display:grid;grid-template-columns:repeat(4,1fr);gap:6px}.preset-grid button{min-width:0;height:54px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--muted);text-align:left;padding:7px 8px;cursor:pointer}.preset-grid button:hover{border-color:var(--border-strong);color:var(--text)}.preset-grid button.active{border-color:var(--accent);background:var(--accent-dim);color:var(--text)}.preset-grid strong,.preset-grid span{display:block}.preset-grid strong{font-size:11px}.preset-grid span{font-size:9px;color:var(--faint);margin-top:4px;white-space:nowrap}
  .frame-policy{display:flex;align-items:center;justify-content:space-between;margin-top:14px;padding-top:13px;border-top:1px solid var(--border)}.frame-policy>div:first-child strong,.frame-policy>div:first-child span{display:block}.frame-policy>div:first-child strong{font-size:11px}.frame-policy>div:first-child span{font-size:10px;color:var(--faint);margin-top:3px}.segmented{display:flex;background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:2px}.segmented button{height:25px;min-width:53px;border:0;border-radius:4px;background:transparent;color:var(--faint);font:inherit;font-size:10px;cursor:pointer}.segmented button.active{background:var(--surface-hover);color:var(--text);box-shadow:0 0 0 1px var(--border-strong)}
  .fields{display:grid;gap:8px}.fields.dimensions{grid-template-columns:repeat(3,1fr);margin-top:12px}.fields.fixed-count{grid-template-columns:1fr;margin-top:8px}.fields.two{grid-template-columns:1.45fr 1fr;margin-top:10px}.fields.two.auto-range{grid-template-columns:repeat(2,1fr)}.fields label{min-width:0;font-size:10px;color:var(--faint)}input,select{display:block;width:100%;height:31px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px;outline:0}input:focus,select:focus{border-color:var(--accent)}select:disabled{opacity:.55}.auto-options{display:grid;grid-template-columns:1fr;gap:7px;margin-top:10px}.switch-row{min-width:0;min-height:50px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--text);padding:8px;display:flex;align-items:center;justify-content:space-between;gap:8px;text-align:left;font:inherit;cursor:pointer}.switch-row:hover{border-color:var(--border-strong)}.switch-row span,.switch-row strong,.switch-row small{display:block}.switch-row strong{font-size:10px}.switch-row small{font-size:9px;line-height:1.3;color:var(--faint);margin-top:3px}.switch-row i{position:relative;flex:0 0 auto;width:30px;height:17px;border-radius:999px;background:var(--border-strong);transition:.18s}.switch-row i b{position:absolute;left:2px;top:2px;width:13px;height:13px;border-radius:50%;background:var(--muted);transition:.18s}.switch-row i.on{background:var(--text)}.switch-row i.on b{transform:translateX(13px);background:var(--bg)}
  .provider-section{border-top:1px solid var(--border);margin-top:15px;padding-top:14px}.provider-section>div:first-child{display:flex;align-items:baseline;justify-content:space-between}.provider-section>div:first-child span{font-size:10px;color:var(--faint)}.mode-description{font-size:10px;line-height:1.45;color:var(--faint);margin:9px 1px 0}
  .image-source-heading{display:flex;align-items:baseline;justify-content:space-between;margin-top:14px;padding-top:13px;border-top:1px solid var(--border)}.image-source-heading span{font-size:10px;color:var(--faint)}
  .capabilities{display:flex;align-items:center;gap:5px;flex-wrap:wrap;margin-top:10px}.capabilities span{font-size:9px;color:var(--faint);border:1px solid var(--border);border-radius:999px;padding:3px 6px}.capabilities span.on{color:var(--muted);border-color:var(--border-strong);background:var(--bg)}.capabilities small{font-size:9px;color:var(--faint);margin-left:auto}
  .help-backdrop{position:fixed;inset:0;z-index:70;background:#000a;display:grid;place-items:center;padding:18px}.help-dialog{width:min(560px,calc(100vw - 36px));max-height:calc(100vh - 36px);overflow:auto;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 28px 90px #000d;padding:18px}.help-dialog header{display:flex;align-items:flex-start;justify-content:space-between;border-bottom:1px solid var(--border);padding-bottom:13px}.help-dialog header strong{font-size:15px}.help-dialog header p{font-size:10px;line-height:1.45;color:var(--faint);margin:4px 0 0}.help-dialog header button{width:28px;height:28px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}.help-dialog dl{display:grid;gap:0;margin:0}.help-dialog dl>div{display:grid;grid-template-columns:130px minmax(0,1fr);gap:16px;padding:12px 2px;border-bottom:1px solid var(--border)}.help-dialog dl>div:last-child{border-bottom:0}.help-dialog dt{font-size:11px;font-weight:650;color:var(--text)}.help-dialog dd{font-size:10px;line-height:1.5;color:var(--muted);margin:0}
  @media(max-width:700px){.preset-grid{grid-template-columns:repeat(2,1fr)}.fields.dimensions{grid-template-columns:repeat(2,1fr)}}
</style>
