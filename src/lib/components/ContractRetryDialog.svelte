<script lang="ts">
  import { untrack } from "svelte";
  import { X } from "lucide-svelte";
  import type { StripScoreReport } from "$lib/types";

  let {
    title = "Finalize strip import",
    subtitle = "Confirm how many frames to split from the strip.",
    score,
    defaultFrameCount = 4,
    busy = false,
    onConfirm,
    onClose,
  }: {
    title?: string;
    subtitle?: string;
    score?: StripScoreReport;
    defaultFrameCount?: number;
    busy?: boolean;
    onConfirm: (frameCount: number, layout: string) => void | Promise<void>;
    onClose: () => void;
  } = $props();

  let frameCount = $state(untrack(() => defaultFrameCount));
  let layout = $state(untrack(() => score?.layoutRecommended === "dp" ? "dp" : score?.layoutRecommended === "profile" ? "profile" : "auto"));

  const hints = $derived([
    score?.gridInkDetected ? "Grid ink detected — profile layout recommended" : null,
    score && score.minCellWidth < 8 ? `Smallest cell ~${score.minCellWidth}px` : null,
    score?.warnings?.[0] ?? null,
  ].filter(Boolean).join("; "));

  const submit = () => {
    const count = Number(frameCount);
    if (!Number.isFinite(count) || count < 1) return;
    void onConfirm(count, layout);
  };
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <section class="dialog">
    <header>
      <div>
        <p>CONTRACT RETRY</p>
        <h2>{title}</h2>
        <small>{subtitle}</small>
      </div>
      <button onclick={onClose}><X size={15} /></button>
    </header>
    {#if score}
      <div class="score-box">
        <div><strong>Detected {score.suggestedFrameCount} frames</strong><span>{Math.round(score.inferenceConfidence * 100)}% confidence</span></div>
        {#if hints}<p>{hints}</p>{/if}
        <dl>
          <div><dt>Layout</dt><dd>{score.layoutRecommended}</dd></div>
          <div><dt>Motion delta</dt><dd>{score.meanMotionDelta.toFixed(2)}</dd></div>
        </dl>
      </div>
    {/if}
    <div class="form">
      <label>Frame count<input type="number" min="1" max="64" bind:value={frameCount} /></label>
      <label>Split layout<select bind:value={layout}>
        <option value="auto">Auto</option>
        <option value="profile">Profile (alpha gutters)</option>
        <option value="horizontal">Horizontal equal width</option>
        <option value="dp">DP segments</option>
      </select></label>
    </div>
    <footer>
      <button onclick={onClose}>Cancel</button>
      <button class="primary" disabled={busy || !Number.isFinite(Number(frameCount)) || Number(frameCount) < 1} onclick={submit}>{busy ? "Working…" : "Confirm"}</button>
    </footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:70;background:#000a;display:grid;place-items:center}
  .dialog{width:min(480px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 28px 90px #000b;padding:20px}
  header{display:flex;align-items:flex-start;justify-content:space-between;gap:12px}
  header p{font-size:9px;letter-spacing:.15em;font-weight:700;color:var(--accent);margin:0 0 7px}
  h2{font-size:17px;margin:0}
  header small{display:block;font-size:10px;color:var(--faint);margin-top:6px;line-height:1.4}
  header button{border:0;background:transparent;color:var(--faint);cursor:pointer}
  .score-box{margin-top:16px;padding:10px;border:1px solid var(--border);border-radius:6px;background:var(--bg)}
  .score-box strong{display:block;font-size:11px}
  .score-box span{font-size:9px;color:var(--faint)}
  .score-box p{margin:8px 0 0;font-size:9px;color:#d0a04f;line-height:1.4}
  .score-box dl{margin:8px 0 0;display:grid;gap:4px}
  .score-box dl div{display:flex;justify-content:space-between;font-size:9px;color:var(--muted)}
  .form{display:grid;gap:12px;margin-top:16px}
  .form label{font-size:10px;color:var(--muted)}
  input,select{width:100%;margin-top:5px;background:var(--bg);border:1px solid var(--border);border-radius:5px;color:var(--text);font:inherit;font-size:11px;padding:8px;outline:0}
  input{height:32px;padding:0 8px}
  select{height:32px}
  footer{display:flex;justify-content:flex-end;gap:7px;border-top:1px solid var(--border);padding-top:15px;margin-top:18px}
  footer button{height:31px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);font:inherit;font-size:11px;padding:0 11px;cursor:pointer}
  footer button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}
  button:disabled{opacity:.45}
</style>
