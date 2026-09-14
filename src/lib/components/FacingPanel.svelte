<script lang="ts">
  import { Compass, RefreshCw } from "lucide-svelte";
  import { api } from "$lib/api";
  import { errorMessage, type FacingCheckReport } from "$lib/types";

  let {
    workspaceId,
    slug,
    onError,
    onNotice,
  }: {
    workspaceId: string;
    slug: string;
    onError: (message: string) => void;
    onNotice?: (message: string) => void;
  } = $props();

  let busy = $state(false);
  let report = $state<FacingCheckReport>();
  let history = $state<FacingCheckReport[]>([]);

  async function refresh() {
    busy = true;
    try {
      report = await api.detectAnchorFacing(workspaceId, slug);
      history = await api.listFacingChecks(workspaceId, slug);
      onNotice?.(`Facing ${report.detectedFacing.toUpperCase()} · ${report.status}`);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    if (workspaceId && slug) {
      void api.listFacingChecks(workspaceId, slug).then(entries => history = entries).catch(() => history = []);
    }
  });

  const statusTone = (status: string) => status === "ok" || status === "auto_oriented" ? "good" : status === "mismatch" ? "error" : "warning";
</script>

<section class="facing-panel">
  <header><Compass size={12} /><span>Facing check</span></header>
  {#if report}
    <dl class="report" class:good={statusTone(report.status) === "good"} class:warn={statusTone(report.status) === "warning"} class:error={statusTone(report.status) === "error"}>
      <div><dt>Detected</dt><dd>{report.detectedFacing.toUpperCase()}</dd></div>
      <div><dt>Canonical</dt><dd>{report.canonicalFacing.toUpperCase()}</dd></div>
      <div><dt>Confidence</dt><dd>{Math.round(report.confidence * 100)}%</dd></div>
      <div><dt>Status</dt><dd>{report.status}</dd></div>
    </dl>
  {:else}
    <p class="muted">Run a facing check to verify canonical W orientation before locomotion strips.</p>
  {/if}
  <button class="wide" onclick={refresh} disabled={busy}><RefreshCw size={12} /> {busy ? "Checking…" : "Detect facing"}</button>
  {#if history.length}
    <div class="timeline">
      <strong>History</strong>
      {#each history.slice(0, 8) as entry}
        <article class={statusTone(entry.status)}>
          <span>{entry.detectedFacing.toUpperCase()} → {entry.canonicalFacing.toUpperCase()}</span>
          <small>{entry.status} · {Math.round(entry.confidence * 100)}%{#if entry.checkedAt} · {new Date(entry.checkedAt).toLocaleString()}{/if}</small>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .facing-panel{border-top:1px solid var(--border);padding-top:10px;margin-top:10px}
  header{display:flex;align-items:center;gap:6px;font-size:9px;letter-spacing:.12em;color:var(--accent);font-weight:700;margin-bottom:8px}
  .report{margin:0 0 10px;display:grid;gap:6px;padding:8px;border:1px solid var(--border);border-radius:5px;background:var(--bg)}
  .report.good{border-left:3px solid #5ead7b}
  .report.warn{border-left:3px solid #c89a4b}
  .report.error{border-left:3px solid #cc6863}
  .report div{display:flex;justify-content:space-between;font-size:10px}
  .report dt{color:var(--faint)}
  .muted{font-size:9px;color:var(--faint);line-height:1.4;margin:0 0 8px}
  .wide{width:100%;height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;gap:6px;align-items:center;justify-content:center;font:inherit;font-size:11px;cursor:pointer}
  .wide:disabled{opacity:.55}
  .timeline{margin-top:10px;display:grid;gap:6px}
  .timeline strong{font-size:8px;letter-spacing:.1em;color:var(--faint)}
  .timeline article{padding:6px 8px;border:1px solid var(--border);border-radius:4px;background:var(--bg);font-size:9px}
  .timeline article span{display:block;color:var(--text)}
  .timeline article small{display:block;color:var(--faint);margin-top:3px}
  .timeline article.good{border-left:2px solid #5ead7b}
  .timeline article.warn{border-left:2px solid #c89a4b}
  .timeline article.error{border-left:2px solid #cc6863}
</style>
