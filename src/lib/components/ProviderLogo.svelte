<script lang="ts">
  let { providerId, size = 18, label }: { providerId: string; size?: number; label?: string } = $props();
  let id = $derived(providerId.toLowerCase());
  let fallback = $derived((providerId.trim()[0] ?? "?").toUpperCase());
  let logoSrc = $derived(
    id === "codex" || id === "openai"
      ? "/provider-logos/openai.png"
      : ["claude", "gemini", "grok"].includes(id)
        ? `/provider-logos/${id}.png`
        : undefined,
  );
</script>

<span class={`provider-logo ${id}`} style={`--provider-logo-size:${size}px`} role="img" aria-label={label ?? `${providerId} logo`}>
  {#if logoSrc}
    <img src={logoSrc} alt="" aria-hidden="true" />
  {:else}
    <span class="fallback" aria-hidden="true">{fallback}</span>
  {/if}
</span>

<style>
  .provider-logo{width:var(--provider-logo-size);height:var(--provider-logo-size);display:inline-grid;place-items:center;flex:0 0 auto;line-height:1;overflow:hidden;border-radius:22%}.provider-logo img{display:block;width:100%;height:100%;object-fit:cover}.provider-logo.gemini img{object-fit:contain}.fallback{font:700 calc(var(--provider-logo-size)*.58)/1 ui-monospace,SFMono-Regular,monospace;color:var(--muted)}
</style>
