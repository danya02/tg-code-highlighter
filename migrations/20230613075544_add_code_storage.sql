CREATE TABLE gist (
    id TEXT PRIMARY KEY,  -- short alphanumeric identifier
    content TEXT NOT NULL,
    sent_by INTEGER NOT NULL,   -- Telegram user ID
    sent_at_unix_time INTEGER NOT NULL,
    language TEXT,  -- can be null if not provided
    is_ephemeral BOOLEAN NOT NULL  -- 0 or 1; if 1, going to delete it soon
);

-- This index allows quickly discovering the gists that are ephemeral and due to be deleted.
CREATE INDEX gist_create_time ON gist (is_ephemeral, sent_at_unix_time) WHERE is_ephemeral>0;
