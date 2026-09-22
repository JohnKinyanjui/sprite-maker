<script lang="ts">
  import LogoMark from "$lib/components/LogoMark.svelte";

  let { stage = "Avvio di Sprite Studio…" }: { stage?: string } = $props();
</script>

<div class="startup-splash" role="status" aria-live="polite" aria-busy="true">
  <div class="startup-card">
    <div class="startup-mark"><LogoMark size={34} /></div>
    <h1>Sprite Studio</h1>
    <p class="startup-tagline">Ambiente locale per asset di gioco con AI</p>
    <div class="progress-track" aria-hidden="true"><span class="progress-bar"></span></div>
    <p class="startup-stage">{stage}</p>
  </div>
</div>

<style>
  .startup-splash {
    width: 100vw;
    height: 100vh;
    display: grid;
    place-items: center;
    background:
      radial-gradient(1200px 600px at 50% -10%, #1a1c22 0%, transparent 60%),
      #090a0a;
    color: var(--text);
  }

  .startup-card {
    width: min(360px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 8px 12px 4px;
  }

  .startup-mark {
    width: 58px;
    height: 58px;
    border: 1px solid #3b3c3d;
    border-radius: 14px;
    display: grid;
    place-items: center;
    color: #f5a524;
    background: #171818;
    box-shadow: 0 0 0 6px #17181880;
  }

  h1 {
    margin: 22px 0 6px;
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.03em;
    color: #f4f4f5;
  }

  .startup-tagline {
    margin: 0;
    font-size: 12px;
    color: #707174;
    line-height: 1.45;
  }

  .progress-track {
    width: 132px;
    height: 2px;
    margin-top: 26px;
    background: #282929;
    border-radius: 999px;
    overflow: hidden;
  }

  .progress-bar {
    display: block;
    width: 42%;
    height: 100%;
    background: linear-gradient(90deg, #8b5cf6, #b7d34b);
    border-radius: inherit;
    animation: startup-progress 1.35s ease-in-out infinite;
  }

  .startup-stage {
    min-height: 18px;
    margin: 14px 0 0;
    font-size: 12px;
    letter-spacing: 0.03em;
    color: #9a9ca1;
  }

  @keyframes startup-progress {
    from { transform: translateX(-120%); }
    to { transform: translateX(320%); }
  }
</style>
