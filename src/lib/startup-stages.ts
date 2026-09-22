const STAGE_LABELS: Record<string, string> = {
  "opening project": "Apertura progetto…",
  "saving the active project": "Salvataggio progetto attivo…",
  "loading project assets and tools": "Caricamento asset e strumenti…",
  "restoring the active worktree": "Ripristino sezione di lavoro…",
  "loading chats and animations": "Caricamento chat e animazioni…",
  "syncing generations": "Sincronizzazione generazioni…",
  "restoring chat preview": "Ripristino anteprima chat…",
  "detecting providers": "Verifica provider AI…",
};

/** User-facing Italian label for a workspace-load stage id. */
export function startupStageLabel(stage: string): string {
  return STAGE_LABELS[stage] ?? stage;
}
