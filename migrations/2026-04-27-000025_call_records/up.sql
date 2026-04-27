CREATE TABLE call_records (
    id            UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    caller_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    callee_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    call_type     TEXT        NOT NULL CHECK (call_type IN ('voice', 'video')),
    status        TEXT        NOT NULL DEFAULT 'initiated'
                              CHECK (status IN ('initiated', 'answered', 'rejected',
                                                'missed', 'ended', 'failed')),
    started_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    answered_at   TIMESTAMPTZ,
    ended_at      TIMESTAMPTZ
);

CREATE INDEX idx_call_records_caller ON call_records (caller_id, started_at DESC);
CREATE INDEX idx_call_records_callee ON call_records (callee_id, started_at DESC);
