<script lang="ts">
  let { size = 24, title = "Sprite Studio" }: { size?: number; title?: string } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 96 96" role="img" aria-label={title}>
  <g fill="none" stroke="currentColor" stroke-width="8" stroke-linecap="square">
    <path d="M30 16H16v14M66 16h14v14M80 66v14H66M30 80H16V66" />
  </g>
  <g fill="var(--logo-pixel, #f4f4f5)">
    <rect x="29" y="32" width="14" height="14" rx="2" />
    <rect x="29" y="51" width="14" height="14" rx="2" />
    <rect x="48" y="51" width="14" height="14" rx="2" />
  </g>
  <path fill="var(--accent, #8b5cf6)" d="M58 28l3.2 9.8L71 41l-9.8 3.2L58 54l-3.2-9.8L45 41l9.8-3.2z" />
</svg>
