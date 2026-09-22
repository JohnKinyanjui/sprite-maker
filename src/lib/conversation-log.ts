import { api } from "$lib/api";

type ConversationLogLevel = "debug" | "info" | "warning" | "error";

export function persistConversationLog(
  conversationId: string,
  message: string,
  options?: {
    level?: ConversationLogLevel;
    category?: string;
    eventType?: string;
    requestId?: string;
    details?: Record<string, unknown>;
  },
) {
  return api.appendConversationLogEntry({
    conversationId,
    requestId: options?.requestId,
    level: options?.level ?? "info",
    category: options?.category ?? "frontend",
    eventType: options?.eventType ?? "activity",
    message,
    details: options?.details,
  }).catch((error) => {
    console.warn("Failed to persist conversation log entry:", error);
  });
}

export function persistConversationLogs(
  conversationId: string,
  messages: string[],
  options?: {
    level?: ConversationLogLevel;
    category?: string;
    eventType?: string;
    requestId?: string;
    details?: Record<string, unknown>;
  },
) {
  return Promise.all(messages.map(message => persistConversationLog(conversationId, message, options)));
}
