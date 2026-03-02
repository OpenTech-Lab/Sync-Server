// @generated — maintained by hand to match migrations.
// Regenerate with: diesel print-schema (requires live DB connection)

diesel::table! {
    use diesel::sql_types::*;

    admin_settings (key) {
        key -> Text,
        value -> Text,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    stickers (id) {
        id -> Uuid,
        uploader_id -> Uuid,
        name -> Text,
        mime_type -> Text,
        content_base64 -> Text,
        size_bytes -> Int4,
        status -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    admin_audit_logs (id) {
        id -> Uuid,
        actor_user_id -> Nullable<Uuid>,
        action -> Text,
        target -> Nullable<Text>,
        details -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    users (id) {
        id -> Uuid,
        username -> Text,
        email -> Text,
        password_hash -> Text,
        avatar_base64 -> Nullable<Text>,
        role -> Text,
        is_active -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    refresh_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        token_hash -> Text,
        family -> Uuid,
        revoked -> Bool,
        issued_at -> Timestamptz,
        expires_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    messages (id) {
        id -> Uuid,
        sender_id -> Uuid,
        recipient_id -> Uuid,
        content -> Text,
        delivered_at -> Nullable<Timestamptz>,
        read_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    federation_actor_keys (id) {
        id -> Uuid,
        actor_username -> Text,
        key_id -> Text,
        public_key_pem -> Text,
        private_key_pkcs8 -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    federation_inbox_activities (id) {
        id -> Uuid,
        activity_id -> Text,
        actor -> Text,
        recipient_username -> Text,
        activity_type -> Text,
        payload -> Jsonb,
        received_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    federation_deliveries (id) {
        id -> Uuid,
        activity_id -> Text,
        sender_username -> Text,
        destination -> Text,
        status -> Text,
        attempts -> Int4,
        last_error -> Nullable<Text>,
        next_attempt_at -> Nullable<Timestamptz>,
        delivered_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    federation_remote_messages (id) {
        id -> Uuid,
        activity_id -> Text,
        object_id -> Nullable<Text>,
        actor -> Text,
        recipient_username -> Text,
        content -> Text,
        received_at -> Timestamptz,
    }
}

diesel::joinable!(refresh_tokens -> users (user_id));
diesel::joinable!(messages -> users (sender_id));
diesel::joinable!(admin_audit_logs -> users (actor_user_id));
diesel::joinable!(stickers -> users (uploader_id));

diesel::allow_tables_to_appear_in_same_query!(
    admin_settings,
    admin_audit_logs,
    users,
    refresh_tokens,
    messages,
    federation_actor_keys,
    federation_inbox_activities,
    federation_deliveries,
    federation_remote_messages,
    stickers,
);
