<script lang="ts">
  import { api, assetUrl } from "$lib/api";
  import type { Animation, GenerationSession } from "$lib/types";
  import { errorMessage } from "$lib/types";
  import { Clapperboard, RefreshCw } from "lucide-svelte";

  let {
    workspaceId,
    worktreeId,
    animations,
    onOpenAnimation,
    onError,
  }: {
    workspaceId: string;
    worktreeId?: string;
    animations: Animation[];
    onOpenAnimation: (animationId: string) => void;
    onError: (message: string) => void;
  } = $props();

  let sessions = $state<GenerationSession[]>([]);
  let loading = $state(false);

  async function refresh() {
    if (!workspaceId) return;
    loading = true;
    try {
      sessions = await api.listGenerationSessions(workspaceId, worktreeId);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (workspaceId) void refresh();
  });

  function animationName(session: GenerationSession) {
    return animations.find(animation => animation.id === session.animationId)?.name ?? session.kind;
  }
</script>

<section class="sessions">
  <header>
    <div><span>SESSIONS</span><strong>Production archive</strong></div>
    <button disabled={loading} onclick={refresh}><RefreshCw size={12} />{loading ? "…" : "Refresh"}</button>
  </header>
  {#if !sessions.length}
    <p class="empty">Nessuna sessione ancora. Harden, motion batch e contract retry creano voci qui.</p>
  {:else}
    <div class="timeline">
      {#each sessions as session}
        <article>
          <div class="thumb">
            {#if session.thumbnailPath}<img src={assetUrl(session.thumbnailPath)} alt="" />{:else}<Clapperboard size={18} />{/if}
          </div>
          <div class="meta">
            <strong>{animationName(session)}</strong>
            <small>{session.kind} · {new Date(session.createdAt).toLocaleString()}</small>
            {#if session.score !== undefined}<span class="score">{Math.round(session.score)}</span>{/if}
          </div>
          {#if session.animationId}
            <button onclick={() => onOpenAnimation(session.animationId!)}>Apri</button>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .sessions{height:100%;padding:16px;overflow:auto}
  header{display:flex;justify-content:space-between;align-items:center;margin-bottom:14px}
  header span{display:block;font-size:8px;letter-spacing:.15em;color:var(--accent);font-weight:700}
  header strong{display:block;font-size:12px;margin-top:4px}
  header button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;align-items:center;gap:6px;padding:0 8px;font:inherit;font-size:10px;cursor:pointer}
  .empty{font-size:12px;color:var(--faint);max-width:420px;line-height:1.5}
  .timeline{display:flex;flex-direction:column;gap:8px}
  article{display:grid;grid-template-columns:48px minmax(0,1fr) auto;gap:10px;align-items:center;padding:10px;border:1px solid var(--border);border-radius:6px;background:var(--sidebar)}
  .thumb{width:48px;height:48px;border-radius:4px;background:var(--bg);display:grid;place-items:center;overflow:hidden;color:var(--faint)}
  .thumb img{width:100%;height:100%;object-fit:contain;image-rendering:pixelated}
  .meta strong{display:block;font-size:12px}.meta small{display:block;font-size:10px;color:var(--faint);margin-top:2px}
  .score{font-size:10px;color:#5ead7b}
  article button{height:26px;border:1px solid var(--border);background:var(--bg);color:var(--text);border-radius:4px;font:inherit;font-size:10px;padding:0 8px;cursor:pointer}
</style>
