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

    daily_action_counters (user_id, action_key, day_bucket) {
        user_id -> Uuid,
        action_key -> Text,
        day_bucket -> Date,
        count -> Int4,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    server_news (id) {
        id -> Uuid,
        title -> Text,
        summary -> Nullable<Text>,
        markdown_content -> Text,
        created_by -> Nullable<Uuid>,
        published_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    encrypted_backups (user_id) {
        user_id -> Uuid,
        encrypted_blob -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    stickers (id) {
        id -> Uuid,
        uploader_id -> Uuid,
        group_name -> Text,
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

    user_trust_stats (user_id) {
        user_id -> Uuid,
        active_days -> Int4,
        contribution_score -> Int4,
        derived_level -> Int4,
        derived_rank -> Text,
        last_active_day -> Nullable<Date>,
        last_human_activity_at -> Nullable<Timestamptz>,
        suspicious_activity_streak -> Int4,
        automation_review_state -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    use diesel::sql_types::*;

    trust_score_events (id) {
        id -> Uuid,
        user_id -> Uuid,
        granter_user_id -> Nullable<Uuid>,
        event_type -> Text,
        delta -> Int4,
        reference_id -> Nullable<Text>,
        metadata -> Jsonb,
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
        message_public_key -> Nullable<Text>,
        role -> Text,
        is_active -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
        device_auth_pubkey -> Nullable<Text>,
        is_approved -> Bool,
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

    device_push_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        platform -> Text,
        token -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_seen_at -> Nullable<Timestamptz>,
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
diesel::joinable!(daily_action_counters -> users (user_id));
diesel::joinable!(stickers -> users (uploader_id));
diesel::joinable!(device_push_tokens -> users (user_id));
diesel::joinable!(encrypted_backups -> users (user_id));
diesel::joinable!(server_news -> users (created_by));
diesel::joinable!(trust_score_events -> users (user_id));
diesel::joinable!(user_trust_stats -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    admin_settings,
    admin_audit_logs,
    daily_action_counters,
    trust_score_events,
    users,
    user_trust_stats,
    encrypted_backups,
    device_push_tokens,
    refresh_tokens,
    messages,
    federation_actor_keys,
    federation_inbox_activities,
    federation_deliveries,
    federation_remote_messages,
    stickers,
    server_news,
);
