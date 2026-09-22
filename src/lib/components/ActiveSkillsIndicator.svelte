<script lang="ts">
  import { Blocks } from "lucide-svelte";
  import type { CustomSkill } from "$lib/library-types";

  let { skills, onOpen }: { skills: CustomSkill[]; onOpen: () => void } = $props();

  const enabledSkills = $derived(skills.filter(skill => skill.enabled && skill.instructions.trim()));
  const label = $derived(
    enabledSkills.length === 1
      ? "1 skill attiva"
      : `${enabledSkills.length} skill attive`,
  );
  const tooltip = $derived(enabledSkills.map(skill => skill.name).join(", "));
</script>

{#if enabledSkills.length}
  <div class="skills-context" title={`Incluse automaticamente nel contesto di generazione: ${tooltip}`}>
    <Blocks size={12}/>
    <span>{label}</span>
    <span class="skill-names">{enabledSkills.map(skill => skill.name).join(" · ")}</span>
    <button type="button" onclick={onOpen}>Gestisci</button>
  </div>
{/if}

<style>
  .skills-context {
    height: 28px;
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: 10px;
    padding: 0 3px;
  }

  .skills-context :global(svg) {
    color: var(--accent);
    flex: 0 0 auto;
  }

  .skill-names {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--faint);
  }

  .skills-context button {
    margin-left: 3px;
    border: 0;
    background: transparent;
    color: var(--faint);
    font: inherit;
    font-size: 10px;
    cursor: pointer;
    flex: 0 0 auto;
  }

  .skills-context button:hover {
    color: var(--text);
  }
</style>
