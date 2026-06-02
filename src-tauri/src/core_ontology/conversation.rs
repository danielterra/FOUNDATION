use crate::owl::{Connection, Result, Individual};

/// Returns the IRI of the conversation with the most recent user message
/// among conversations that have a `foundation:handledBy` triple.
pub fn find_conversation_by_last_user_message(conn: &Connection) -> Result<Option<String>> {
    Individual::find_class_instance_ordered_by_child_timestamp(
        conn,
        "foundation:AIConversation",
        "foundation:handledBy",
        "foundation:partOfConversation",
        "foundation:sentAt",
        "foundation:role",
        "user",
    )
}
