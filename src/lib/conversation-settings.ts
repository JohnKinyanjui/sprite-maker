import { api } from "$lib/api";
import { normalizeGenerationProfile } from "$lib/generation-profiles";
import type { AnimationPolishMode, ChatGenerationProfile, Conversation, ProviderMode } from "$lib/types";
import type { ConversationStyleId } from "$lib/style-presets";

export type ConversationUiSettings = {
  style: ConversationStyleId;
  animationMode: AnimationPolishMode;
  profile: ChatGenerationProfile;
};

export function snapshotConversationUiSettings(
  style: ConversationStyleId,
  animationMode: AnimationPolishMode,
  profile: ChatGenerationProfile,
): ConversationUiSettings {
  return { style, animationMode, profile };
}

export async function persistConversationUiSettings(
  conversation: Conversation,
  settings: ConversationUiSettings,
  providerModes: ProviderMode[],
): Promise<ChatGenerationProfile> {
  const profile = normalizeGenerationProfile(settings.profile, providerModes, conversation.provider);
  await Promise.all([
    api.setSetting(`conversation-style:${conversation.id}`, settings.style),
    api.setSetting(`conversation-animation-mode:${conversation.id}`, settings.animationMode),
    api.setSetting(`conversation-generation:${conversation.id}`, profile),
  ]);
  return profile;
}
