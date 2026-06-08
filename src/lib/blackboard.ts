import { invoke } from '@tauri-apps/api/core';

export async function openEntity(entityIri: string, conversationId?: string | null): Promise<void> {
  await invoke('widget_blackboard__add_widget', {
    widgetType: '',
    entityId: entityIri,
    position: null,
    size: null,
    conversationId: conversationId ?? null,
  });
}
