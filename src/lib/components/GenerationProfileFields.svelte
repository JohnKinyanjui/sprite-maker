<script lang="ts">
  import { normalizeGenerationProfile, profileForQuality } from "$lib/generation-profiles";
  import type { ChatGenerationProfile, GenerationQuality, ProviderStatus } from "$lib/types";

  let { profile, provider, imageProviders = [], compact = false, disabled = false, onChange }: {
    profile: ChatGenerationProfile;
    provider?: ProviderStatus;
    imageProviders?: ProviderStatus[];
    compact?: boolean;
    disabled?: boolean;
    onChange: (profile: ChatGenerationProfile) => void | Promise<void>;
  } = $props();

  const nativeImageProvider = $derived(provider?.id === "codex" || provider?.id === "cursor" || provider?.id === "antigravity");

  function commit(value: ChatGenerationProfile) {
    if (disabled) return;
    return onChange(normalizeGenerationProfile(value, provider?.modes ?? [], provider?.id));
  }

  function chooseQuality(quality: GenerationQuality) {
    if (quality === "custom") return commit({ ...profile, quality });
    return commit(profileForQuality(quality, profile));
  }

  function numberChange(key: "width" | "height" | "frames" | "fps" | "minFrames" | "maxFrames", value: string) {
    return commit({ ...profile, quality: "custom", [key]: Number(value) });
  }
</script>

<div class="generation-fields" class:compact>
  <div class="preset-grid" aria-label="Generation quality">
    {#each ["low", "mid", "high", "custom"] as quality}
      <button class:active={profile.quality === quality} disabled={disabled} onclick={() => chooseQuality(quality as GenerationQuality)}>
        <strong>{quality === "mid" ? "Mid" : quality[0].toUpperCase() + quality.slice(1)}</strong>
        <span>{quality === "low" ? "64px · 6–8f" : quality === "mid" ? "128px · 8–12f" : quality === "high" ? "256px · 12–16f" : "Your values"}</span>
      </button>
    {/each}
  </div>

  <div class="frame-policy">
    <div><strong>Frames</strong>{#if !compact}<span>{profile.frameMode === "auto" ? "AI uses the smallest count that keeps the motion clear" : "Use an exact count you choose"}</span>{/if}</div>
    <div class="segmented" aria-label="Frame policy">
      <button class:active={profile.frameMode === "auto"} disabled={disabled} onclick={() => commit({...profile, frameMode:"auto", allowAutoAdjust:true})}>Auto</button>
      <button class:active={profile.frameMode === "fixed"} disabled={disabled} onclick={() => commit({...profile, frameMode:"fixed", allowAutoAdjust:false})}>Fixed</button>
    </div>
  </div>

  <div class="fields dimensions">
    <label>Width<input type="number" min="8" max="512" value={profile.width} disabled={disabled} onchange={(event) => numberChange("width", event.currentTarget.value)}/></label>
    <label>Height<input type="number" min="8" max="512" value={profile.height} disabled={disabled} onchange={(event) => numberChange("height", event.currentTarget.value)}/></label>
    <label>FPS<input type="number" min="1" max="60" value={profile.fps} disabled={disabled} onchange={(event) => numberChange("fps", event.currentTarget.value)}/></label>
  </div>
  {#if profile.frameMode === "fixed"}
    <div class="fields fixed-count"><label>Exact frame count<input type="number" min="1" max="64" value={profile.frames} disabled={disabled} onchange={(event) => numberChange("frames", event.currentTarget.value)}/></label></div>
  {:else}
    <div class="fields two auto-range"><label>Minimum<input type="number" min="1" max="64" value={profile.minFrames} disabled={disabled} onchange={(event) => numberChange("minFrames", event.currentTarget.value)}/></label><label>Maximum<input type="number" min="1" max="64" value={profile.maxFrames} disabled={disabled} onchange={(event) => numberChange("maxFrames", event.currentTarget.value)}/></label></div>
    <div class="auto-options">
      <button class="switch-row" role="switch" aria-checked={profile.allowAutoAdjust} disabled={disabled} onclick={()=>commit({...profile,allowAutoAdjust:!profile.allowAutoAdjust})}><span><strong>Allow extra frames</strong><small>Add frames only when the motion cannot read cleanly without them</small></span><i class:on={profile.allowAutoAdjust}><b></b></i></button>
    </div>
  {/if}

  <div class="image-source-heading"><strong>Image API</strong>{#if !compact}<span>Generates the master and every animation frame</span>{/if}</div>
  <div class="fields"><label>Image API<select value={profile.imageProviderId} disabled={disabled} onchange={(event)=>commit({...profile,imageProviderId:event.currentTarget.value})}>
    {#if !nativeImageProvider}<option value="provider-native">Chat only — no image API</option>{/if}
    {#each imageProviders.filter(item=>item.status==="ready" && (item.id!=="imagegen" || provider?.id==="codex") && (item.id!=="cursor-image" || provider?.id==="cursor") && (item.id!=="antigravity-image" || provider?.id==="antigravity")) as imageProvider}<option value={imageProvider.id}>{imageProvider.name}</option>{/each}
  </select></label></div>
  {#if !nativeImageProvider && (profile.imageProviderId === "provider-native" || profile.imageProviderId === "imagegen")}<p class="mode-description">This sends a chat-only request. To create a source image, choose Grok Image, Gemini Image, or another configured image API.</p>{/if}
</div>

<style>
  .preset-grid{display:grid;grid-template-columns:repeat(4,1fr);gap:6px}.preset-grid button{min-width:0;height:54px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--muted);text-align:left;padding:7px 8px;cursor:pointer}.preset-grid button:hover:not(:disabled){border-color:var(--border-strong);color:var(--text)}.preset-grid button.active{border-color:var(--accent);background:var(--accent-dim);color:var(--text)}.preset-grid button:disabled{opacity:.55;cursor:default}.preset-grid strong,.preset-grid span{display:block}.preset-grid strong{font-size:11px}.preset-grid span{font-size:9px;color:var(--faint);margin-top:4px;white-space:nowrap}
  .frame-policy{display:flex;align-items:center;justify-content:space-between;margin-top:14px;padding-top:13px;border-top:1px solid var(--border)}.frame-policy>div:first-child strong,.frame-policy>div:first-child span{display:block}.frame-policy>div:first-child strong{font-size:11px}.frame-policy>div:first-child span{font-size:10px;color:var(--faint);margin-top:3px}.segmented{display:flex;background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:2px}.segmented button{height:25px;min-width:53px;border:0;border-radius:4px;background:transparent;color:var(--faint);font:inherit;font-size:10px;cursor:pointer}.segmented button.active{background:var(--surface-hover);color:var(--text);box-shadow:0 0 0 1px var(--border-strong)}.segmented button:disabled{opacity:.55;cursor:default}
  .fields{display:grid;gap:8px}.fields.dimensions{grid-template-columns:repeat(3,1fr);margin-top:12px}.fields.fixed-count{grid-template-columns:1fr;margin-top:8px}.fields.two{grid-template-columns:1.45fr 1fr;margin-top:10px}.fields.two.auto-range{grid-template-columns:repeat(2,1fr)}.fields label{min-width:0;font-size:10px;color:var(--faint)}input,select{display:block;width:100%;height:31px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px;outline:0}input:focus,select:focus{border-color:var(--accent)}select:disabled,input:disabled{opacity:.55}
  .auto-options{display:grid;grid-template-columns:1fr;gap:7px;margin-top:10px}.switch-row{min-width:0;min-height:50px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--text);padding:8px;display:flex;align-items:center;justify-content:space-between;gap:8px;text-align:left;font:inherit;cursor:pointer}.switch-row:hover:not(:disabled){border-color:var(--border-strong)}.switch-row:disabled{opacity:.55;cursor:default}.switch-row span,.switch-row strong,.switch-row small{display:block}.switch-row strong{font-size:10px}.switch-row small{font-size:9px;line-height:1.3;color:var(--faint);margin-top:3px}.switch-row i{position:relative;flex:0 0 auto;width:30px;height:17px;border-radius:999px;background:var(--border-strong);transition:.18s}.switch-row i b{position:absolute;left:2px;top:2px;width:13px;height:13px;border-radius:50%;background:var(--muted);transition:.18s}.switch-row i.on{background:var(--text)}.switch-row i.on b{transform:translateX(13px);background:var(--bg)}
  .image-source-heading{display:flex;align-items:baseline;justify-content:space-between;margin-top:14px;padding-top:13px;border-top:1px solid var(--border)}.image-source-heading span{font-size:10px;color:var(--faint)}
  .mode-description{font-size:10px;line-height:1.45;color:var(--faint);margin:9px 1px 0}
  .generation-fields.compact .preset-grid button{height:48px}
  .generation-fields.compact .frame-policy{margin-top:10px;padding-top:10px}
  .generation-fields.compact .image-source-heading{margin-top:10px;padding-top:10px}
  .generation-fields.compact .switch-row{min-height:44px}
  @media(max-width:700px){.preset-grid{grid-template-columns:repeat(2,1fr)}.fields.dimensions{grid-template-columns:repeat(2,1fr)}}
</style>
