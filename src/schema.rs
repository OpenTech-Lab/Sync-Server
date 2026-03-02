// @generated — maintained by hand to match migrations.
// Regenerate with: diesel print-schema (requires live DB connection)

diesel::table! {
    use diesel::sql_types::*;

    users (id) {
        id -> Uuid,
        username -> Text,
        email -> Text,
        password_hash -> Text,
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

diesel::joinable!(refresh_tokens -> users (user_id));
diesel::joinable!(messages -> users (sender_id));

diesel::allow_tables_to_appear_in_same_query!(users, refresh_tokens, messages,);
